//! Synthetic notes and prompts. The notes match the #29 Phi Silica investigation and
//! `crates/noteiq-core/tests/tidy_tests.rs`, so results are directly comparable.

pub const ROUGH: &str = "q4 roadmap review w/ alex, priya
- alex: ship sync beta by dec 5?? depends on api freeze
- priya worried abt perf on arm devices, wants benchmarks first
- decided: no new features after nov 28
- todo me: send benchmark plan to priya by fri
- open q: who owns docs update";

pub const BUDGET: &str = "budget call w/ dana
- we did NOT agree on pricing yet
- budget is 40k, not 400k (typo in last deck)
- dana declined the 2nd workshop; maybe q1??
- ravi to fix deck numbers by tue
- legal review still pending, no date";

pub const LONG: &str = "Customer call - Contoso onboarding, 14 Nov

Attendees: Sam Lee (Contoso IT), Alex Morgan, me

Sam said their rollout to 2,000 devices is blocked on the proxy configuration; their network team needs the list of endpoints we call. Alex mentioned the list exists in the admin guide but is outdated since the October release. Sam also asked whether notes can stay on-device only, because legal won't approve cloud processing of meeting content yet. We explained the current app stores Markdown locally in OneDrive and doesn't send note text anywhere.

There was some back and forth about pilot size. Sam prefers 50 users in the finance department first, then the rest in January. Alex thought 200 would surface more issues, but we agreed to go with Sam's 50-user pilot.

Sam's biggest pain point is that people forget to write anything after meetings, so the reminders feature was well received. He asked if templates can be pushed centrally; we don't support that today.

Next steps: Alex will update the endpoint list in the admin guide. I will send Sam the pilot checklist. Nobody took the action to answer the central templates question.";

pub const TIDY_SYSTEM: &str = "You tidy up rough meeting notes. Rewrite the NOTE as clean Markdown bullets: \
fix spelling and capitalization and expand abbreviations. Keep every fact, name, number, date, \
negation, and owner. Keep uncertainty visible (keep '??' or write 'tentative'). Keep 'me' and 'I' \
as written. Do not add facts, names, dates, owners, currency symbols, or headings that are not in \
the NOTE. Output only the rewritten note.";

pub const SUMMARY_SYSTEM: &str = "Summarize the NOTE in at most 4 short Markdown bullets for someone \
who missed the meeting. Use only facts stated in the NOTE; never infer or add details. Output only \
the bullets.";

pub const FOLLOWUPS_SYSTEM: &str = "Extract follow-ups from the NOTE as JSON. action_items: tasks \
someone must do. decisions: things that were agreed. open_questions: unresolved questions. For each \
item, 'quote' must be copied exactly from the NOTE. Use null for an owner or due date the NOTE does \
not state. 'me' or 'I' is the note's author; write 'me' as the owner. Do not invent items.";

pub const FOLLOWUPS_SCHEMA: &str = r#"{"type":"object","additionalProperties":false,"properties":{"action_items":{"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"task":{"type":"string"},"owner":{"type":["string","null"]},"due":{"type":["string","null"]},"quote":{"type":"string"}},"required":["task","owner","due","quote"]}},"decisions":{"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"text":{"type":"string"},"quote":{"type":"string"}},"required":["text","quote"]}},"open_questions":{"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"text":{"type":"string"},"quote":{"type":"string"}},"required":["text","quote"]}}},"required":["action_items","decisions","open_questions"]}"#;

/// The realistic search corpus from #29, where Phi Silica embeddings scored top-1 1/5.
pub const CORPUS: [&str; 6] = [
    "Q4 roadmap review with Alex and Priya. Alex wants to ship the sync beta by Dec 5 if the API freezes. Priya is worried about performance on ARM devices and battery drain; she wants benchmarks first. No new features after Nov 28.",
    "Customer call with Contoso onboarding. Their rollout is blocked on proxy configuration and they need our endpoint list. Legal won't approve cloud processing of meeting content, so notes must stay on the device. Pilot with 50 finance users.",
    "1:1 with Sam. Career goals: wants to lead the release process next half. Feedback: great customer empathy, improve written status updates.",
    "Release retro for 0.3.0. Tagging went fine but the SBOM upload failed on ARM64; checksums were published manually. Action: automate attestation retry.",
    "Offsite planning. 14 people, vegetarian lunch Thursday, room booked in building 4. Budget approved by Dana.",
    "Security review of the WorkIQ integration. Tokens never written to disk; logs redact meeting titles. Open: threat model for the URI protocol handler.",
];

/// Query and the index of the note it should find.
pub const QUERIES: [(&str, usize); 5] = [
    ("battery and power concerns on laptops", 0),
    ("customer privacy requirements", 1),
    ("problems publishing a new version", 3),
    ("feedback for a team member", 2),
    ("how is the noteiq:// link handler protected", 5),
];

/// A harder set: 20 notes that share vocabulary (customers, releases, ARM, budget), and
/// queries paraphrased to avoid the target note's keywords.
pub const HARD_CORPUS: [&str; 20] = [
    "Contoso pilot kickoff: 50 finance users start Monday; Sam owns comms; rollback plan is to uninstall via Intune.",
    "Fabrikam renewal call: they want a 3-year term but legal asked for a data residency addendum for EU tenants.",
    "Release 0.4.0 planning: ARM64 and x64 builds, cut the branch on Dec 2, freeze strings a week earlier.",
    "Perf triage: cold start on the Surface is 2.1 s, mostly font loading; Priya will profile the Slint renderer.",
    "Budget sync with Dana: travel is 12% over; conferences frozen until Q1; hardware refresh still approved.",
    "Incident review: OneDrive sync conflict duplicated two notes; root cause was clock skew; add conflict banner.",
    "1:1 with Ravi: wants to move to the platform team next half; needs mentoring on design docs.",
    "Accessibility audit: three contrast failures in settings and no keyboard focus ring on the tidy button.",
    "Customer call with Northwind: they asked for SSO with Okta; we only support Entra today; follow up in Jan.",
    "Security sync: tokens are stored with DPAPI; nothing sensitive in logs; pen test booked for February.",
    "Hiring loop debrief: strong yes on system design, mixed on coding; second interview with Alex next week.",
    "Docs sprint: rewrite the setup guide, add screenshots of the tray menu, remove the old CLI section.",
    "Team offsite agenda: roadmap in the morning, hackathon after lunch, dinner booked for 14 people.",
    "Telemetry review: we collect crash counts only, no note content; product wants feature usage, privacy said no.",
    "Localization: Dutch and German strings at 80%; date formats wrong in the meeting header for en-GB.",
    "Battery test on Snapdragon: idle drain doubled when the tray icon polls the calendar every 30 seconds.",
    "Partner demo prep: show capture hotkey, meeting reminders, and the tidy preview; skip search for now.",
    "Contoso escalation: rollout to 2,000 devices blocked on proxy allowlist; endpoints doc is outdated.",
    "Code review norms: two approvals for core crates, one for docs, and CI must be green before merge.",
    "Retro: the 0.3.0 release slipped two days because SBOM attestation failed on the ARM runner.",
];

pub const HARD_QUERIES: [(&str, usize); 10] = [
    ("why is the laptop losing charge so quickly", 15),
    ("what stops the big customer deployment", 17),
    ("which identity provider does a customer want", 8),
    ("what was wrong with keyboard navigation", 7),
    ("career growth conversation", 6),
    ("why did the last version ship late", 19),
    ("what data do we gather about usage", 13),
    ("translation progress", 14),
    ("how are credentials protected at rest", 9),
    ("app takes too long to open", 3),
];
