# Local WorkIQ MCP Contract

## Process discovery

On Windows, prefer the architecture-matched WorkIQ executable packaged with
the installed Scout application. A locally configured executable path overrides
discovery. Never install, download, or copy a WorkIQ executable.

Expected entry point: `workiq.exe mcp`, using stdin/stdout for MCP and stderr
for diagnostics. Do not pass note body text to the meeting lookup.

## Capability discovery

1. Start the process with a bounded lifetime and captured standard streams.
2. Send MCP `initialize` with a supported protocol version, client identity, and
   client capabilities.
3. Validate the successful JSON-RPC response and send
   `notifications/initialized`.
4. Request `tools/list`; require a valid list of unique tool names and schemas.
5. Prefer a discovered meeting-search/retrieval capability. Validate its input
   schema before invoking it; never guess required parameters.
6. If no compatible meeting search is available, show an actionable
   unsupported/unavailable state and keep note capture usable.

## Meeting retrieval

Call the discovered `retrieve` tool with the query `My calendar meetings today`,
the `Meetings` capability, and the `grounding` strategy. Never send note text.
Parse `structuredContent["application/vnd.ms-workiq.retrieval"].results[]`
(see research.md for the field format) into candidate meetings with validated
start/end times, then select the current meeting locally. Older or alternative
builds that return plain meeting objects (`id`/`eventId`, RFC 3339
`start`/`end`) are still accepted. If the actual tool does not expose bounded
meeting search, do not call a general natural-language question that returns
unrelated email or chat content; show the user a clear limitation instead.

Timeouts: 15 seconds per handshake request, 45 seconds for the meeting search.

Selection rule: zero current candidates is No meeting; one candidate is
auto-selected only when enabled; more than one candidate requires user choice.
An explicit connect/disconnect decision takes precedence over late results.

## Failure classes

Distinguish executable missing, process start failure, initialization failure,
capability missing, authentication/consent, timeout, malformed response, and
unexpected exit. Log sanitized correlation ID, phase, duration, and failure
class. Never log note bodies, access tokens, raw calendar results, attendee
lists, or meeting titles.

## Development fixtures

Tests use local JSON-RPC streams or fixture tools for handshake, discovery,
one/zero/multiple meetings, malformed output, timeout, and subprocess failure.
`crates/noteiq-core/tests/fixtures/workiq/retrieval_today.json` is a synthetic
response that reproduces the real escaping; `workiq_process_tests.rs` runs a
scripted fake `workiq mcp` server on Unix. Live meeting contents are not
required for the platform-independent suite.
