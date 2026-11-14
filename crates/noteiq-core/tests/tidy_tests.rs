use noteiq_core::tidy::{
    AI_WATERMARK, Change, ChangeKind, FactWarning, MAX_TIDY_CHARS, TidyBlock, check_eligible,
    clean_output, diff_markdown, diff_words, fact_warnings,
};

const ROUGH: &str = "q4 roadmap review w/ alex, priya
- alex: ship sync beta by dec 5?? depends on api freeze
- priya worried abt perf on arm devices, wants benchmarks first
- decided: no new features after nov 28
- todo me: send benchmark plan to priya by fri
- open q: who owns docs update";

// Responses recorded from the Windows on-device model on a Snapdragon X Elite NPU.
const CONCISE: &str = "- Review roadmap with Alex and Priya.
- Alex: Ship sync beta by Dec 5, depending on API freeze.
- Priya: Concerned about performance on ARM devices, wants benchmarks first.
- Decision: No new features after Nov 28.
- Task: Send benchmark plan to Priya by Friday.
- Open question: Who is responsible for updating documents.";

const DRIFTED: &str = "## Discussion
- Talked about Q4 roadmap.
- Syncing beta on ship with Alex.

## Decisions
- Dependency on API freeze (tentative).
- Decision made: no new features post-Nov 28.

## Action items
- (Todo) - Send benchmark plan to Priya by Friday.

## Open questions
- Ownership: Who is responsible for updating the documents?";

const BUDGET: &str = "budget call w/ dana
- we did NOT agree on pricing yet
- budget is 40k, not 400k (typo in last deck)
- dana declined the 2nd workshop; maybe q1??
- ravi to fix deck numbers by tue
- legal review still pending, no date";

const BUDGET_CONCISE: &str = "- Budget call with Dana: Pricing not agreed upon.
- Budget: $40k, not $400k (typo in last deck).
- Dana declined the 2nd workshop; possible in Q1?
- Ravi to update deck numbers by Tuesday.
- Legal review pending, no date.";

#[test]
fn only_short_notes_with_text_can_be_tidied() {
    assert_eq!(check_eligible(" \n "), Err(TidyBlock::Empty));
    assert_eq!(check_eligible(ROUGH), Ok(()));
    let long = "é".repeat(MAX_TIDY_CHARS + 1);
    assert_eq!(
        check_eligible(&long),
        Err(TidyBlock::TooLong {
            limit: MAX_TIDY_CHARS
        })
    );
    assert_eq!(check_eligible(&long[..long.len() - 2]), Ok(()));
}

#[test]
fn a_faithful_rewrite_flags_only_the_lost_hedge_and_owner() {
    assert_eq!(
        fact_warnings(ROUGH, CONCISE),
        vec![
            FactWarning::UncertaintyRemoved,
            FactWarning::FirstPersonRemoved
        ]
    );
}

#[test]
fn a_rewrite_that_drops_a_date_is_flagged() {
    let warnings = fact_warnings(ROUGH, DRIFTED);
    assert!(warnings.contains(&FactWarning::NumberRemoved("5".into())));
    assert!(warnings.contains(&FactWarning::FirstPersonRemoved));
}

#[test]
fn negations_and_amounts_that_survive_are_not_flagged_but_an_added_currency_is() {
    assert_eq!(
        fact_warnings(BUDGET, BUDGET_CONCISE),
        vec![FactWarning::SymbolAdded('$')]
    );
}

#[test]
fn equal_amounts_in_another_format_match() {
    assert!(fact_warnings("40k budget, 2,000 devices", "Budget 40,000; 2000 devices.").is_empty());
}

#[test]
fn removed_or_added_negations_are_flagged_including_contractions() {
    assert_eq!(
        fact_warnings("we did not agree on pricing", "We agreed on pricing."),
        vec![FactWarning::NegationRemoved]
    );
    assert_eq!(
        fact_warnings("we didn't agree on pricing", "We agreed on pricing."),
        vec![FactWarning::NegationRemoved]
    );
    assert_eq!(
        fact_warnings("ship it friday", "Do not ship it on Friday."),
        vec![FactWarning::NegationAdded]
    );
}

#[test]
fn new_names_are_flagged_but_capitalized_known_words_and_expansions_are_not() {
    assert_eq!(
        fact_warnings("call with sam on tue", "Call with Sam and Dana on Tuesday."),
        vec![FactWarning::NameAdded("Dana".into())]
    );
}

#[test]
fn changed_numbers_are_reported_as_written() {
    assert_eq!(
        fact_warnings("pilot with 50 users", "Pilot with 200 users."),
        vec![
            FactWarning::NumberRemoved("50".into()),
            FactWarning::NumberAdded("200".into())
        ]
    );
}

#[test]
fn a_much_shorter_rewrite_is_flagged() {
    assert_eq!(
        fact_warnings(
            "alex will update the endpoint list and sam will review the pilot checklist today",
            "Update list."
        ),
        vec![FactWarning::MuchShorter]
    );
}

#[test]
fn every_warning_has_a_message() {
    for warning in fact_warnings(ROUGH, DRIFTED) {
        assert!(!warning.message().is_empty());
    }
    assert!(TidyBlock::Empty.message().contains("Write something"));
}

fn side(changes: &[Change], skip: ChangeKind) -> String {
    changes
        .iter()
        .filter(|change| change.kind != skip)
        .map(|change| change.text.as_str())
        .collect()
}

#[test]
fn word_diff_replaces_whole_words_and_rebuilds_both_texts() {
    let changes = diff_words("send plan by fri", "Send plan by Friday.");
    let kinds = changes
        .iter()
        .map(|c| (c.kind, c.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            (ChangeKind::Removed, "send"),
            (ChangeKind::Added, "Send"),
            (ChangeKind::Same, " plan by "),
            (ChangeKind::Removed, "fri"),
            (ChangeKind::Added, "Friday."),
        ]
    );
    for (before, after) in [(ROUGH, CONCISE), (ROUGH, DRIFTED), (BUDGET, BUDGET_CONCISE)] {
        let changes = diff_words(before, after);
        assert_eq!(side(&changes, ChangeKind::Added), before);
        assert_eq!(side(&changes, ChangeKind::Removed), after);
    }
}

#[test]
fn the_watermark_is_ignored_when_comparing() {
    let marked = format!("Send{AI_WATERMARK} plan.");
    assert_eq!(fact_warnings("send plan", &marked), Vec::new());
    assert_eq!(
        side(&diff_words("Send plan.", &marked), ChangeKind::Removed),
        "Send plan."
    );
}

#[test]
fn the_diff_renders_as_escaped_markdown_line_by_line() {
    let changes = diff_words("- a *b*\n  - c", "- a *d*\n  - c");
    assert_eq!(
        diff_markdown(&changes),
        "\\- a \\*<font color=\"#b91c1c\">~~b~~</font> <font color=\"#15803d\"><u>d</u></font>\\*\n\u{a0}\u{a0}\\- c"
    );
    // Spaces stay outside the strike-through so the Markdown markers still apply.
    let changes = vec![Change {
        kind: ChangeKind::Removed,
        text: " gone \nnext".into(),
    }];
    assert_eq!(
        diff_markdown(&changes),
        "\u{a0}<font color=\"#b91c1c\">~~gone ¶next~~</font>"
    );
}

#[test]
fn changed_whitespace_is_marked_and_unchanged_whitespace_shown_once() {
    // Joining two list items removes only a line break, which must stay visible.
    let joined = diff_markdown(&diff_words("- a\n- b", "- a - b"));
    assert_eq!(
        joined,
        "\\- a<font color=\"#b91c1c\">~~¶~~</font><font color=\"#15803d\"><u>·</u></font>\\- b"
    );
    let indented = diff_markdown(&diff_words("a\nb", "a\n\tb"));
    assert!(indented.contains("<u>→</u>"), "{indented}");
    let blank_line = vec![Change {
        kind: ChangeKind::Same,
        text: "a\n   \nb".into(),
    }];
    assert_eq!(diff_markdown(&blank_line), "a\n\u{a0}\u{a0}\u{a0}\nb");
}

#[test]
fn decimals_and_units_are_compared_as_written() {
    assert_eq!(
        fact_warnings("budget 1.5m, dose 5mg", "Budget 2m, dose 5mg."),
        vec![
            FactWarning::NumberRemoved("1.5m".into()),
            FactWarning::NumberAdded("2m".into())
        ]
    );
    assert_eq!(
        fact_warnings("timeout 20ms", "Timeout 200ms."),
        vec![
            FactWarning::NumberRemoved("20ms".into()),
            FactWarning::NumberAdded("200ms".into())
        ]
    );
}

#[test]
fn removed_names_are_flagged_but_shortened_ones_are_not() {
    assert_eq!(
        fact_warnings(
            "call with Alice and Bob on Friday",
            "Call with Alice on Fri."
        ),
        vec![FactWarning::NameRemoved("Bob".into())]
    );
}

#[test]
fn model_output_is_normalized_for_the_editor() {
    assert_eq!(clean_output("\r\n- a\r\n- b\r\n\r\n", "x"), "- a\n- b");
    assert_eq!(clean_output("- a\n", "x\n"), "- a\n");
}
