mod common;

use chrono::{DateTime, Duration, FixedOffset};
use common::{capture, instant, invitee, meeting, raw_capture};
use noteiq_core::{
    destination::{Destination, save_capture},
    markdown::render_markdown,
    model::{Capture, CaptureSource, Invitee, InviteeRole, Meeting, Rsvp},
};
use tempfile::tempdir;

fn at(text: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(text).expect("valid test time")
}

fn render(capture: &Capture) -> String {
    render_markdown(capture, capture.created_at).expect("renders")
}

fn front_matter(note: &str) -> &str {
    let rest = note
        .strip_prefix("---\n")
        .expect("front matter opens the note");
    &rest[..=rest.find("\n---\n").expect("front matter is closed")]
}

fn property_keys(note: &str) -> Vec<&str> {
    front_matter(note)
        .lines()
        .filter(|line| !line.starts_with(' '))
        .map(|line| line.split_once(':').expect("a key: value line").0)
        .collect()
}

/// Returns a property's raw YAML text, such as `"[[Alex]]"` with its quotes.
fn property<'a>(note: &'a str, key: &str) -> Option<&'a str> {
    front_matter(note)
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(": "))
}

fn title(note: &str) -> &str {
    note.lines()
        .find_map(|line| line.strip_prefix("# "))
        .expect("the note has a title")
}

fn when_line(note: &str) -> &str {
    note.lines()
        .find_map(|line| line.strip_prefix("- **When:** "))
        .expect("the meeting has a time")
}

fn meeting_at(start: Option<&str>, end: Option<&str>) -> Meeting {
    Meeting {
        start: start.map(at),
        end: end.map(at),
        ..meeting()
    }
}

fn connected(meeting: Meeting) -> Capture {
    let mut capture = capture();
    capture.set_body("Typed text");
    capture.connect(meeting);
    capture
}

fn captured_at(time: &str, timezone: &str) -> Capture {
    let mut capture = Capture::new(CaptureSource::Shortcut, at(time), timezone.to_owned());
    capture.set_body("Typed text");
    capture
}

#[test]
fn properties_follow_the_vault_capture_template_order() {
    let mut capture = connected(Meeting {
        your_rsvp: Some(Rsvp::Accepted),
        invitees: [
            InviteeRole::Required,
            InviteeRole::Optional,
            InviteeRole::Resource,
            InviteeRole::Unknown,
        ]
        .into_iter()
        .map(|role| Invitee {
            role,
            ..invitee("Person")
        })
        .collect(),
        ..meeting()
    });
    capture.author = Some("Alex Example".to_owned());
    capture.tags = vec!["follow-up".to_owned()];

    assert_eq!(
        property_keys(&render(&capture)),
        [
            "kind",
            "source",
            "from",
            "date",
            "captured",
            "processing",
            "ai_refined",
            "status",
            "tags",
            "meeting",
            "meeting_start",
            "meeting_end",
            "organizer",
            "your_rsvp",
            "required_invitees",
            "optional_invitees",
            "resource_invitees",
            "other_invitees",
            "calendar_event_id",
            "id",
            "updated",
            "timezone",
            "captured_via",
            "noteiq_schema",
        ]
    );
}

#[test]
fn capture_times_keep_milliseconds_and_the_local_date() {
    let capture = captured_at("2026-11-04T00:30:07.123456+01:00", "Europe/Amsterdam");
    let note = render_markdown(&capture, at("2026-11-04T00:31:00+01:00")).unwrap();

    assert_eq!(
        property(&note, "date"),
        Some("2026-11-04"),
        "the local date, not the UTC date"
    );
    assert_eq!(
        property(&note, "captured"),
        Some("2026-11-04T00:30:07.123+01:00")
    );
    assert_eq!(
        property(&note, "updated"),
        Some("2026-11-04T00:31:00.000+01:00")
    );
    assert!(note.contains("\n**Captured:** 2026-11-04 00:30\n"));
}

#[test]
fn body_sections_follow_the_vault_capture_template_order() {
    let mut capture = connected(meeting());
    capture.author = Some("Alex Example".to_owned());
    let note = render(&capture);

    let sections = [
        "\n---\n\n# Project review\n\n",
        "**From:** [[Alex Example]] · **Captured:** 2026-11-03 14:15\n\n<!-- Captured with NoteIQ.",
        "\n## Meeting details\n\n- **When:** ",
        "\n## Summary\n\n## Follow-ups\n\n---\n\n<details>\n<summary>Raw capture (frozen — evidence, non-AI)</summary>\n",
        "\n\nTyped text\n\n</details>\n",
    ];
    let mut position = 0;
    for section in sections {
        let found = note[position..]
            .find(section)
            .unwrap_or_else(|| panic!("{section:?} follows the previous section"));
        position += found + section.len();
    }
    assert_eq!(position, note.len(), "the raw capture block ends the note");
}

#[test]
fn the_title_prefers_the_meeting_then_the_first_meaningful_line_then_a_fallback() {
    assert_eq!(title(&render(&connected(meeting()))), "Project review");

    let mut untitled_meeting = connected(Meeting {
        title: Some(" -- ".to_owned()),
        ..meeting()
    });
    untitled_meeting.set_body("\n  \n## - [ ] Call the bank\nSecond line");
    assert_eq!(title(&render(&untitled_meeting)), "Call the bank");

    let mut blank = capture();
    blank.set_body("  \n> \n- [ ]\n");
    assert_eq!(title(&render(&blank)), "Quick note");
}

#[test]
fn long_first_lines_are_shortened_to_an_eighty_character_title() {
    let cases = [
        ("a".repeat(80), "a".repeat(80)),
        ("a".repeat(81), format!("{}…", "a".repeat(79))),
        ("é".repeat(120), format!("{}…", "é".repeat(79))),
        (
            format!("{} tail of the line", "b".repeat(78)),
            format!("{}…", "b".repeat(78)),
        ),
    ];
    for (line, expected) in cases {
        let mut capture = capture();
        capture.set_body(format!("{line}\nSecond line"));
        let note = render(&capture);

        assert_eq!(title(&note), expected);
        assert!(title(&note).chars().count() <= 80);
        assert!(
            raw_capture(&note).starts_with(&format!("{line}\n")),
            "the typed line is kept in full"
        );
    }
}

#[test]
fn calendar_titles_render_literally_in_the_heading() {
    let cases = [
        ("Planning <details>", r"Planning \<details\>"),
        ("Planning <!-- agenda", r"Planning \<!-- agenda"),
        (
            "%%hidden%% #tag ==mark==",
            r"\%\%hidden\%\% \#tag \=\=mark\=\=",
        ),
        (
            "[[Link]] *bold* _it_ `code` ~~gone~~ $x$ a|b",
            r"\[\[Link\]\] \*bold\* \_it\_ \`code\` \~\~gone\~\~ \$x\$ a\|b",
        ),
        ("AT&T &amp; R&D", r"AT&T \&amp; R&D"),
        ("&#38; &#x26; &; & ;", r"\&\#38; \&\#x26; &; & ;"),
        ("1. Intro", r"1\. Intro"),
        ("2) Wrap-up", r"2\) Wrap-up"),
        ("- Draft", r"\- Draft"),
        ("+ Extra", r"\+ Extra"),
        ("Line one\nLine two", "Line one Line two"),
    ];
    for (raw, escaped) in cases {
        let note = render(&connected(Meeting {
            title: Some(raw.to_owned()),
            ..meeting()
        }));

        assert_eq!(title(&note), escaped, "{raw:?}");
        assert!(
            note.contains(&format!(
                "\n# {escaped}\n\n**Captured:** 2026-11-03 14:15\n"
            )),
            "{raw:?}"
        );
        assert_eq!(
            property(&note, "meeting"),
            Some(serde_json::to_string(raw).unwrap().as_str()),
            "the property keeps the calendar title as is"
        );
    }
}

#[test]
fn the_author_is_linked_when_known_and_left_out_otherwise() {
    let cases = [
        (Some("Alex Example"), Some("Alex Example")),
        (
            Some("  Doe, Jane [Contoso] | #1\n"),
            Some("Doe, Jane Contoso 1"),
        ),
        (Some("Ann/Bo: R^D?*\"<x>\\"), Some("Ann Bo R D x")),
        (Some(" [[]] | "), None),
        (None, None),
    ];
    for (author, link) in cases {
        let mut capture = capture();
        capture.author = author.map(str::to_owned);
        capture.set_body("Typed text");
        let note = render(&capture);

        match link {
            Some(link) => {
                assert_eq!(
                    property(&note, "from"),
                    Some(format!("\"[[{link}]]\"").as_str())
                );
                assert!(note.contains(&format!(
                    "\n**From:** [[{link}]] · **Captured:** 2026-11-03 14:15\n"
                )));
            }
            None => {
                assert_eq!(property(&note, "from"), None, "{author:?}");
                assert!(!note.contains("**From:**"), "{author:?}");
                assert!(note.contains("\n**Captured:** 2026-11-03 14:15\n"));
            }
        }
    }
}

#[test]
fn meeting_times_are_shown_on_the_local_clock_with_a_duration() {
    let cases = [
        (
            "2026-11-03T14:00:00+01:00",
            Some("2026-11-03T14:30:00+01:00"),
            "2026-11-03 14:00–14:30 (30 min)",
        ),
        (
            "2026-11-03T13:00:00Z",
            Some("2026-11-03T14:30:00Z"),
            "2026-11-03 14:00–15:30 (1 h 30 min)",
        ),
        (
            "2026-11-03T09:00:00-05:00",
            Some("2026-11-03T11:00:00-05:00"),
            "2026-11-03 15:00–17:00 (2 h)",
        ),
        (
            "2026-11-03T22:30:00Z",
            Some("2026-11-04T00:00:00Z"),
            "2026-11-03 23:30 – 2026-11-04 01:00 (1 h 30 min)",
        ),
        ("2026-11-03T14:00:00+01:00", None, "2026-11-03 14:00"),
        (
            "2026-11-03T14:00:00+01:00",
            Some("2026-11-03T13:00:00+01:00"),
            "2026-11-03 14:00",
        ),
    ];
    for (start, end, when) in cases {
        let note = render(&connected(meeting_at(Some(start), end)));
        assert_eq!(when_line(&note), when, "{start} to {end:?}");
    }

    let utc = render(&connected(meeting_at(
        Some("2026-11-03T13:00:00Z"),
        Some("2026-11-03T14:30:00Z"),
    )));
    assert_eq!(
        property(&utc, "meeting_start"),
        Some("2026-11-03T14:00:00+01:00")
    );
    assert_eq!(
        property(&utc, "meeting_end"),
        Some("2026-11-03T15:30:00+01:00")
    );

    let no_start = render(&connected(meeting_at(
        None,
        Some("2026-11-03T14:30:00+01:00"),
    )));
    assert!(!no_start.contains("**When:**"));
    assert_eq!(property(&no_start, "meeting_start"), None);
    assert_eq!(
        property(&no_start, "meeting_end"),
        Some("2026-11-03T14:30:00+01:00")
    );
}

#[test]
fn meetings_across_a_daylight_saving_change_keep_their_real_wall_clock_times() {
    let spring = captured_at("2026-03-29T01:50:00+01:00", "Europe/Amsterdam");
    for end in [
        "2026-03-29T03:30:00+02:00",
        "2026-03-29T02:30:00+01:00",
        "2026-03-29T01:30:00Z",
    ] {
        let mut capture = spring.clone();
        capture.connect(meeting_at(Some("2026-03-29T01:30:00+01:00"), Some(end)));
        let note = render(&capture);

        assert_eq!(when_line(&note), "2026-03-29 01:30–03:30 (1 h)", "{end}");
        assert_eq!(
            property(&note, "meeting_start"),
            Some("2026-03-29T01:30:00+01:00")
        );
        assert_eq!(
            property(&note, "meeting_end"),
            Some("2026-03-29T03:30:00+02:00")
        );
    }

    let mut autumn = captured_at("2026-10-25T01:50:00+02:00", "Europe/Amsterdam");
    autumn.connect(meeting_at(
        Some("2026-10-25T01:30:00+02:00"),
        Some("2026-10-25T02:30:00+01:00"),
    ));
    let note = render(&autumn);
    assert_eq!(
        when_line(&note),
        "2026-10-25 01:30–02:30 (2 h)",
        "the repeated hour still counts"
    );
    assert_eq!(
        property(&note, "meeting_end"),
        Some("2026-10-25T02:30:00+01:00")
    );
}

#[test]
fn an_unknown_or_inconsistent_zone_falls_back_to_the_capture_offset() {
    let mut unknown = captured_at("2026-03-29T01:50:00+01:00", "local");
    unknown.connect(meeting_at(
        Some("2026-03-29T01:30:00+01:00"),
        Some("2026-03-29T03:30:00+02:00"),
    ));
    let note = render(&unknown);
    assert_eq!(when_line(&note), "2026-03-29 01:30–02:30 (1 h)");
    assert_eq!(
        property(&note, "meeting_end"),
        Some("2026-03-29T02:30:00+01:00")
    );
    assert_eq!(property(&note, "timezone"), Some("\"local\""));

    // London is on +00:00 in November, so a +01:00 capture was not taken on its clock.
    let mut mismatched = captured_at("2026-11-03T14:15:00+01:00", "Europe/London");
    mismatched.connect(meeting_at(
        Some("2026-11-03T13:00:00Z"),
        Some("2026-11-03T13:30:00Z"),
    ));
    let note = render(&mismatched);
    assert_eq!(when_line(&note), "2026-11-03 14:00–14:30 (30 min)");
    assert_eq!(
        property(&note, "meeting_start"),
        Some("2026-11-03T14:00:00+01:00")
    );
}

#[test]
fn invitee_names_render_literally_and_an_unknown_rsvp_is_left_out() {
    let note = render(&connected(Meeting {
        organizer: Some("*Ops* <ops@example.com>".to_owned()),
        your_rsvp: Some(Rsvp::Unknown),
        invitees: vec![
            Invitee {
                name: None,
                email: Some("kim_lee@example.com".to_owned()),
                rsvp: Rsvp::NotResponded,
                has_attended: Some(false),
                ..invitee("")
            },
            Invitee {
                rsvp: Rsvp::Declined,
                ..invitee("1. [Pat](x) <b>")
            },
            invitee("   "),
        ],
        ..meeting()
    }));

    assert_eq!(property(&note, "your_rsvp"), None);
    assert!(!note.contains("**Your RSVP:**"));
    assert!(note.contains("\norganizer: \"*Ops* <ops@example.com>\"\n"));
    assert!(note.contains("\n- **Organizer:** \\*Ops\\*\n"));
    assert!(note.contains(
        "\nrequired_invitees:\n  - \"kim_lee@example.com\"\n  - \"1. [Pat](x) <b>\"\noptional_invitees: []\ncalendar_event_id: "
    ));
    assert!(note.contains(
        "\n- **Required invitees:**\n  - kim\\_lee@example.com — no response, did not attend\n  - 1\\. \\[Pat\\](x) \\<b\\> — declined\n\n## Summary\n"
    ));
}

#[test]
fn user_tags_follow_the_base_tags_once_each() {
    let mut capture = capture();
    capture.tags = [
        "Capture",
        "NOTE",
        "follow-up",
        " Follow-Up ",
        "  ",
        " meeting-note ",
        "日本",
    ]
    .map(str::to_owned)
    .to_vec();

    assert_eq!(
        property(&render(&capture), "tags"),
        Some(r#"[capture, note, "follow-up", "meeting-note", "日本"]"#)
    );
}

#[test]
fn a_long_multibyte_note_is_still_recognised_as_its_own_file_on_the_next_save() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let long_body = "日本語のメモ — ".repeat(80_000);
    assert!(
        long_body.len() > 1 << 20,
        "longer than the ownership check reads"
    );
    let mut capture = capture();
    capture.set_body(long_body.clone());
    let first = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();

    capture.set_body(format!("{long_body}\nThe end."));
    let second = save_capture(&mut capture, &destination, instant() + Duration::minutes(1))
        .expect("the note still owns its file")
        .unwrap();

    assert_eq!(first, second);
    let note = std::fs::read_to_string(&second).unwrap();
    assert!(raw_capture(&note).ends_with("\nThe end.\n"));
    assert_eq!(
        std::fs::read_dir(first.parent().unwrap()).unwrap().count(),
        1,
        "no temporary or duplicate files are left behind"
    );
}
