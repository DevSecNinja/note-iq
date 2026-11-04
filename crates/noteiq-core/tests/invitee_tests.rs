mod common;

use common::{capture, instant, meeting, raw_capture};
use noteiq_core::{
    markdown::render_markdown,
    model::{InviteeRole, Rsvp},
    workiq::{apply_meeting_details, parse_meeting_results},
};
use serde_json::json;
use yaml_rust2::YamlLoader;

fn event(people: serde_json::Value) -> serde_json::Value {
    json!({
        "id": "event-42", "start": instant(), "end": instant() + chrono::TimeDelta::hours(1),
        "organizer": {"emailAddress": {"name": "Owner", "address": "owner@example.test"}},
        "attendees": people,
        "responseStatus": {"response": "accepted"}
    })
}

#[test]
fn retains_invitation_roles_contact_addresses_rsvp_and_explicit_attendance() {
    let data = event(json!([
        {"type":"required","emailAddress":{"name":"Pat \"Q\" O'Brien","address":"pat@example.test"},
         "status":{"response":"accepted"},"hasAttended":true},
        {"type":"optional","emailAddress":{"name":"Zoë","address":"zoe@example.test"},
         "status":{"response":"tentativelyAccepted"},"hasAttended":false},
        {"type":"resource","emailAddress":{"name":"Room","address":"room@example.test"}},
        {"type":"unknown","emailAddress":{"address":"unknown@example.test"}}
    ]));
    let parsed = parse_meeting_results(&json!({"meetings":[data]}), instant()).unwrap();
    let meeting = &parsed[0];
    assert_eq!(
        meeting.organizer.as_deref(),
        Some("Owner <owner@example.test>")
    );
    assert_eq!(meeting.your_rsvp, Some(Rsvp::Accepted));
    assert_eq!(meeting.invitees[0].role, InviteeRole::Required);
    assert_eq!(meeting.invitees[1].role, InviteeRole::Optional);
    assert_eq!(meeting.invitees[2].role, InviteeRole::Resource);
    assert_eq!(meeting.invitees[3].role, InviteeRole::Unknown);

    let mut note = capture();
    note.connect(meeting.clone());
    note.set_body("# Unchanged body\n\nWith <angle brackets>.");
    let text = render_markdown(&note, instant()).unwrap();
    let (_, rest) = text.split_once("---\n").unwrap();
    let (yaml, _) = rest.split_once("\n---\n\n").unwrap();
    let yaml = YamlLoader::load_from_str(yaml).unwrap().remove(0);
    assert_eq!(
        yaml["organizer"].as_str(),
        Some("Owner <owner@example.test>")
    );
    assert_eq!(yaml["your_rsvp"].as_str(), Some("accepted"));
    assert_eq!(
        yaml["required_invitees"][0].as_str(),
        Some("Pat \"Q\" O'Brien <pat@example.test>")
    );
    assert_eq!(
        yaml["optional_invitees"][0].as_str(),
        Some("Zoë <zoe@example.test>")
    );
    assert_eq!(
        yaml["resource_invitees"][0].as_str(),
        Some("Room <room@example.test>")
    );
    assert_eq!(
        yaml["other_invitees"][0].as_str(),
        Some("unknown@example.test")
    );
    assert!(!text.contains("attendees:"));
    assert!(text.contains("- **Organizer:** Owner\n"));
    assert!(text.contains("- **Your RSVP:** Accepted\n"));
    assert!(
        text.contains("- **Required invitees:**\n  - Pat \"Q\" O'Brien — accepted, attended\n")
    );
    assert!(text.contains("- **Optional invitees:**\n  - Zoë — tentative, did not attend\n"));
    assert!(text.contains("- **Resources:**\n  - Room\n"));
    assert!(text.contains("- **Other invitees:**\n  - unknown@example.test\n"));
    assert_eq!(
        raw_capture(&text),
        "# Unchanged body\n\nWith <angle brackets>.\n"
    );
}

#[test]
fn accepted_and_declined_invitations_do_not_imply_attendance() {
    let people = json!([
        {"type":"required","name":"Accepted","status":{"response":"accepted"}},
        {"type":"optional","name":"Declined","status":{"response":"declined"}},
        {"name":"No response","status":{"response":"notResponded"}},
        {"name":"Missing status"},
        {"name":"Unrecognized status","status":{"response":"future-value"}},
        {"name":"Invalid attendance","hasAttended":"false"},
        {}
    ]);
    let parsed = parse_meeting_results(&json!({"meetings":[event(people)]}), instant()).unwrap();
    let invitees = &parsed[0].invitees;
    assert_eq!(invitees.len(), 6);
    assert_eq!(invitees[0].rsvp, Rsvp::Accepted);
    assert_eq!(invitees[1].rsvp, Rsvp::Declined);
    assert_eq!(invitees[2].rsvp, Rsvp::NotResponded);
    assert_eq!(invitees[3].rsvp, Rsvp::Unknown);
    assert_eq!(invitees[4].rsvp, Rsvp::Unknown);
    assert!(invitees.iter().all(|person| person.has_attended.is_none()));
}

#[test]
fn merging_rsvp_details_preserves_attendance_by_email_not_display_name() {
    let original = event(json!([{
        "type":"required","emailAddress":{"name":"Old name","address":"pat@example.test"},
        "hasAttended":true
    }]));
    let mut parsed = parse_meeting_results(&json!({"meetings":[original]}), instant()).unwrap();
    apply_meeting_details(
        &mut parsed[0],
        &event(json!([
            {"type":"optional","emailAddress":{"name":"New name","address":"PAT@example.test"},
             "status":{"response":"accepted"}},
            {"type":"required","emailAddress":{"name":"New invitee","address":"new@example.test"}}
        ])),
    );
    assert_eq!(parsed[0].invitees.len(), 2);
    assert_eq!(parsed[0].invitees[0].has_attended, Some(true));
    assert_eq!(parsed[0].invitees[0].rsvp, Rsvp::Accepted);
    assert_eq!(parsed[0].invitees[0].role, InviteeRole::Optional);
    assert_eq!(parsed[0].invitees[1].has_attended, None);
}

#[test]
fn missing_details_preserve_invitees_but_an_explicit_empty_list_clears_them() {
    let mut meeting = meeting();
    let original = meeting.invitees.clone();
    apply_meeting_details(&mut meeting, &json!({}));
    assert_eq!(meeting.invitees, original);
    apply_meeting_details(&mut meeting, &json!({"attendees":[]}));
    assert!(meeting.invitees.is_empty());
}

#[test]
fn retrieval_keeps_attendance_and_unknown_roles_without_dropping_email_addresses() {
    let response = json!({"structuredContent":{"application/vnd.ms-workiq.retrieval":{"results":[{
        "resourceId":"event-42","entityType":"Meetings",
        "markdown": "- **Review**\n  - **time:** today at 1 PM to today at 2 PM\n  - **invitees:** [{\"type\":\"Required\",\"emailAddress\":{\"name\":\"<person>Pat</person>\",\"address\":\"pat@example.test\"},\"hasAttended\":true},{\"type\":\"Optional\",\"emailAddress\":{\"address\":\"optional@example.test\"},\"hasAttended\":false},{\"emailAddress\":{\"name\":\"Name only\"}}]"
    }]}}});
    let parsed = parse_meeting_results(&response, instant()).unwrap();
    assert_eq!(
        parsed[0].invitees[0].display_contact(),
        "Pat <pat@example.test>"
    );
    assert_eq!(parsed[0].invitees[0].has_attended, Some(true));
    assert_eq!(parsed[0].invitees[1].has_attended, Some(false));
    assert_eq!(parsed[0].invitees[2].display_contact(), "Name only");
    assert_eq!(parsed[0].invitees[2].role, InviteeRole::Unknown);
}
