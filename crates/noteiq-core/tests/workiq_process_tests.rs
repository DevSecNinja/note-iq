//! Drives `WorkiqClient` against a scripted stand-in for Scout's `workiq mcp` process.

use chrono::{DateTime, FixedOffset};
use noteiq_core::workiq::{WorkiqClient, WorkiqError};

fn now() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2026-11-03T14:15:00+01:00").unwrap()
}

#[test]
fn reports_a_missing_executable_without_starting_anything() {
    let directory = tempfile::tempdir().unwrap();
    let client = WorkiqClient::new(directory.path().join("workiq.exe"));

    assert!(matches!(
        client.discover_meetings(now()),
        Err(WorkiqError::ExecutableMissing)
    ));
}

#[cfg(unix)]
mod fake_server {
    use super::now;
    use chrono::DateTime;
    use noteiq_core::workiq::{WorkiqClient, WorkiqError};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::{Mutex, MutexGuard},
        time::Duration,
    };
    use tempfile::TempDir;

    /// Serializes script creation and process spawning so no forked child inherits a
    /// script file descriptor that is still open for writing (ETXTBSY).
    static SPAWN_LOCK: Mutex<()> = Mutex::new(());

    const INITIALIZE: &str = r#"printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fake-workiq","version":"0"}}}\n' "$id""#;
    const TOOLS: &str = r#"printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"retrieve","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"strategy":{"type":["string","null"]},"capabilities":{"type":["array","null"],"items":{"anyOf":[{"properties":{"name":{"const":"Meetings"}}}]}}},"required":["query"]}}]}}\n' "$id""#;

    struct FakeServer {
        _directory: TempDir,
        executable: PathBuf,
        _guard: MutexGuard<'static, ()>,
    }

    impl FakeServer {
        fn client(&self) -> WorkiqClient {
            WorkiqClient::new(self.executable.clone()).with_timeout(Duration::from_secs(5))
        }
    }

    fn fixture_path() -> String {
        format!(
            "{}/tests/fixtures/workiq/retrieval_today.json",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn fake_server(body: &str) -> FakeServer {
        let guard = SPAWN_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("workiq");
        let script = format!(
            "#!/bin/sh\n[ \"$1\" = mcp ] || exit 64\nfixture='{}'\n{body}\n",
            fixture_path()
        );
        fs::write(&executable, script).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        FakeServer {
            _directory: directory,
            executable,
            _guard: guard,
        }
    }

    /// Answers each JSON-RPC request with the shell snippet for its method.
    fn responding(initialize: &str, tools: &str, call: &str) -> String {
        format!(
            r#"while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  [ -n "$id" ] || continue
  case "$line" in
    *'"method":"initialize"'*) {initialize} ;;
    *'"method":"tools/list"'*) {tools} ;;
    *'"method":"tools/call"'*) {call} ;;
    *) printf '{{"jsonrpc":"2.0","id":%s,"error":{{"code":-32601,"message":"Method not found"}}}}\n' "$id" ;;
  esac
done"#
        )
    }

    fn tool_result(result_json: &str) -> String {
        format!(r#"printf '{{"jsonrpc":"2.0","id":%s,"result":{result_json}}}\n' "$id""#)
    }

    fn event_fetch_tools(extra_required: bool) -> String {
        let required = if extra_required {
            vec!["entityUrls", "agentId"]
        } else {
            vec!["entityUrls"]
        };
        tool_result(
            &serde_json::json!({"tools":[
                {"name":"retrieve","inputSchema":{"type":"object",
                    "properties":{"query":{"type":"string"},
                        "capabilities":{"type":["array","null"],"items":{"anyOf":[
                            {"properties":{"name":{"const":"Meetings"}}}
                        ]}}},"required":["query"]}},
                {"name":"fetch","inputSchema":{"type":"object",
                    "properties":{"entityUrls":{"type":"array","items":{"type":"string"}}},
                    "required":required}}
            ]})
            .to_string(),
        )
    }

    fn event_fetch_call(fetch_result: &str) -> String {
        format!(
            r#"case "$line" in
      *'"name":"fetch"'*) {fetch_result} ;;
      *) printf '{{"jsonrpc":"2.0","id":%s,"result":' "$id"; tr -d '\r\n' < "$fixture"; printf '}}\n' ;;
    esac"#
        )
    }

    #[test]
    fn fetches_calendar_rsvp_after_discovering_the_fetch_schema() {
        let result = tool_result(
            r#"{"structuredContent":{"results":[{"statusCode":200,"data":{"id":"AAMkAGFakeEventId02AAA=","attendees":[{"type":"required","emailAddress":{"name":"Attendee One","address":"one@example.test"},"status":{"response":"accepted"}}]}}]}}"#,
        );
        let server = fake_server(&responding(
            INITIALIZE,
            &event_fetch_tools(false),
            &event_fetch_call(&result),
        ));
        let meetings = server.client().discover_meetings(now()).unwrap();
        assert_eq!(meetings[1].invitees.len(), 1);
        assert_eq!(
            meetings[1].invitees[0].rsvp,
            noteiq_core::model::Rsvp::Accepted
        );
        assert_eq!(meetings[1].invitees[0].has_attended, None);
    }

    #[test]
    fn preserves_successful_rsvp_rows_when_a_batch_is_partially_throttled() {
        let result = tool_result(
            r#"{"isError":true,"structuredContent":{"results":[{"statusCode":429,"data":null},{"statusCode":200,"data":{"id":"AAMkAGFakeEventId02AAA=","attendees":[{"type":"required","emailAddress":{"name":"Attendee One","address":"one@example.test"},"status":{"response":"accepted"}}]}}]}}"#,
        );
        let server = fake_server(&responding(
            INITIALIZE,
            &event_fetch_tools(false),
            &event_fetch_call(&result),
        ));
        let meetings = server.client().discover_meetings(now()).unwrap();
        assert_eq!(meetings.len(), 3);
        assert_eq!(meetings[1].invitees.len(), 1);
        assert_eq!(
            meetings[1].invitees[0].rsvp,
            noteiq_core::model::Rsvp::Accepted
        );
    }

    #[test]
    fn failed_rsvp_fetch_keeps_retrieved_meetings_and_invitees() {
        let result = tool_result(
            r#"{"isError":false,"structuredContent":{"results":[{"statusCode":403,"data":null}]}}"#,
        );
        let server = fake_server(&responding(
            INITIALIZE,
            &event_fetch_tools(false),
            &event_fetch_call(&result),
        ));
        let meetings = server.client().discover_meetings(now()).unwrap();
        assert_eq!(meetings.len(), 3);
        assert_eq!(meetings[1].invitees.len(), 2);
        assert_eq!(
            meetings[1].invitees[0].rsvp,
            noteiq_core::model::Rsvp::Unknown
        );
    }

    #[test]
    fn does_not_guess_required_arguments_for_an_incompatible_fetch_tool() {
        let result = tool_result(
            r#"{"structuredContent":{"results":[{"statusCode":200,"data":{"id":"AAMkAGFakeEventId02AAA=","attendees":[]}}]}}"#,
        );
        let server = fake_server(&responding(
            INITIALIZE,
            &event_fetch_tools(true),
            &event_fetch_call(&result),
        ));
        let meetings = server.client().discover_meetings(now()).unwrap();
        assert_eq!(
            meetings[1].invitees.len(),
            2,
            "the incompatible fetch tool must not be called"
        );
    }

    #[test]
    fn discovers_todays_meetings_through_the_mcp_handshake() {
        let call = r#"case "$line" in
      *'"query":"My calendar meetings today"'*)
        printf '{"jsonrpc":"2.0","id":%s,"result":' "$id"; tr -d '\r\n' < "$fixture"; printf '}\n' ;;
      *) printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32602,"message":"unexpected query"}}\n' "$id" ;;
    esac"#;
        let server = fake_server(&responding(INITIALIZE, TOOLS, call));

        let meetings = server.client().discover_meetings(now()).unwrap();

        assert_eq!(meetings.len(), 3);
        assert_eq!(meetings[1].id, "AAMkAGFakeEventId02AAA=");
        assert_eq!(
            meetings[1].title.as_deref(),
            Some("Project review & planning")
        );
        assert_eq!(
            meetings[1].invitees[0].display_contact(),
            "Attendee One <one@example.test>"
        );
        assert_eq!(
            meetings[1].invitees[1].display_contact(),
            "Attendee Two <two@example.test>"
        );
    }

    #[test]
    fn merges_and_dedupes_meetings_from_every_query() {
        let upcoming = r#"{"content":[],"structuredContent":{"application/vnd.ms-workiq.retrieval":{"results":[{"markdown":"- **Next sync**\n  - **time:** today at 1:30 PM to today at 2 PM\n  - **timeUntilMeeting:** 15 minutes","resourceId":"AAMkNextEvent=","entityType":"Meetings"}]}},"isError":false}"#;
        let call = format!(
            r#"case "$line" in
      *'"query":"My calendar meetings starting in the next hour"'*)
        printf '{{"jsonrpc":"2.0","id":%s,"result":%s}}\n' "$id" '{upcoming}' ;;
      *)
        printf '{{"jsonrpc":"2.0","id":%s,"result":' "$id"; tr -d '\r\n' < "$fixture"; printf '}}\n' ;;
    esac"#
        );
        let server = fake_server(&responding(INITIALIZE, TOOLS, &call));

        let meetings = server.client().discover_meetings(now()).unwrap();

        let ids = meetings
            .iter()
            .map(|meeting| meeting.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "AAMkAGFakeEventId01AAA=",
                "AAMkAGFakeEventId02AAA=",
                "AAMkAGFakeEventId03AAA=",
                "AAMkNextEvent="
            ],
            "the fixture answers two queries but each meeting appears once"
        );
        assert_eq!(
            meetings[3].start,
            Some(DateTime::parse_from_rfc3339("2026-11-03T14:30:00+01:00").unwrap())
        );
    }

    #[test]
    fn keeps_the_meetings_found_when_some_queries_fail() {
        let call = r#"case "$line" in
      *'"query":"My calendar meetings happening now"'*)
        printf '{"jsonrpc":"2.0","id":%s,"result":' "$id"; tr -d '\r\n' < "$fixture"; printf '}\n' ;;
      *'"query":"My calendar meetings today"'*)
        printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"Service unavailable"}],"isError":true}}\n' "$id" ;;
      *) : ;;
    esac"#;
        let server = fake_server(&responding(INITIALIZE, TOOLS, call));
        let client = server.client().with_timeout(Duration::from_secs(2));

        let meetings = client.discover_meetings(now()).unwrap();

        assert_eq!(
            meetings.len(),
            3,
            "the unanswered query times out on its own"
        );
    }

    #[test]
    fn prefers_sign_in_required_when_every_query_fails() {
        let call = r#"case "$line" in
      *'"query":"My calendar meetings happening now"'*)
        printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"Token expired. Sign in again."}],"isError":true}}\n' "$id" ;;
      *)
        printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"Service unavailable"}],"isError":true}}\n' "$id" ;;
    esac"#;
        let server = fake_server(&responding(INITIALIZE, TOOLS, call));

        assert!(matches!(
            server.client().discover_meetings(now()),
            Err(WorkiqError::AuthenticationRequired)
        ));
    }

    #[test]
    fn reports_unsupported_capability_when_no_meeting_search_tool_exists() {
        let tools = r#"printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"ask","inputSchema":{"type":"object","properties":{"question":{"type":"string"}}}}]}}\n' "$id""#;
        let server = fake_server(&responding(INITIALIZE, tools, ":"));

        assert!(matches!(
            server.client().discover_meetings(now()),
            Err(WorkiqError::UnsupportedCapability)
        ));
    }

    #[test]
    fn reports_sign_in_required_when_initialize_is_unauthorized() {
        let initialize = r#"printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32001,"message":"Authentication required: sign in to Microsoft 365"}}\n' "$id""#;
        let server = fake_server(&responding(initialize, TOOLS, ":"));

        assert!(matches!(
            server.client().discover_meetings(now()),
            Err(WorkiqError::AuthenticationRequired)
        ));
    }

    #[test]
    fn reports_sign_in_required_when_the_tool_reports_an_expired_token() {
        let call = tool_result(
            r#"{"content":[{"type":"text","text":"Token expired. Sign in again."}],"isError":true}"#,
        );
        let server = fake_server(&responding(INITIALIZE, TOOLS, &call));

        assert!(matches!(
            server.client().discover_meetings(now()),
            Err(WorkiqError::AuthenticationRequired)
        ));
    }

    #[test]
    fn reports_lookup_failed_for_other_tool_errors() {
        let call = tool_result(
            r#"{"content":[{"type":"text","text":"Service unavailable"}],"isError":true}"#,
        );
        let server = fake_server(&responding(INITIALIZE, TOOLS, &call));

        assert!(matches!(
            server.client().discover_meetings(now()),
            Err(WorkiqError::LookupFailed)
        ));
    }

    #[test]
    fn reports_invalid_response_for_non_json_output() {
        let server = fake_server(&responding("printf 'not json\\n'", TOOLS, ":"));

        assert!(matches!(
            server.client().discover_meetings(now()),
            Err(WorkiqError::InvalidResponse)
        ));
    }

    #[test]
    fn reports_invalid_response_when_initialize_omits_the_protocol_version() {
        let initialize =
            r#"printf '{"jsonrpc":"2.0","id":%s,"result":{"capabilities":{}}}\n' "$id""#;
        let server = fake_server(&responding(initialize, TOOLS, ":"));

        assert!(matches!(
            server.client().discover_meetings(now()),
            Err(WorkiqError::InvalidResponse)
        ));
    }

    #[test]
    fn reports_process_exited_when_workiq_stops_immediately() {
        let server = fake_server("exit 0");

        assert!(matches!(
            server.client().discover_meetings(now()),
            Err(WorkiqError::ProcessExited)
        ));
    }

    #[test]
    fn reports_process_exited_when_workiq_stops_mid_lookup() {
        let server = fake_server(&responding(INITIALIZE, TOOLS, "exit 3"));

        assert!(matches!(
            server.client().discover_meetings(now()),
            Err(WorkiqError::ProcessExited)
        ));
    }

    #[test]
    fn times_out_and_stops_a_process_that_never_answers() {
        let server = fake_server("exec sleep 30");
        let client = server.client().with_timeout(Duration::from_millis(300));

        let started = std::time::Instant::now();
        let result = client.discover_meetings(now());

        assert!(matches!(result, Err(WorkiqError::TimedOut)));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the hung process is killed after the shutdown grace period"
        );
    }
}
