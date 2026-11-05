use chrono::{DateTime, FixedOffset};
use noteiq_core::note_filename::{
    DATE_TITLE_NOTE_FILENAME, DEFAULT_NOTE_FILENAME, MAX_STEM_LENGTH, NoteFilename,
    NoteFilenameError,
};

fn at() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2026-11-04T14:15:00+01:00").unwrap()
}

fn parse(template: &str) -> NoteFilename {
    NoteFilename::parse(template).unwrap_or_else(|error| panic!("{template:?}: {error}"))
}

#[test]
fn the_default_template_keeps_the_original_naming() {
    let default = NoteFilename::default();

    assert_eq!(default.template(), DEFAULT_NOTE_FILENAME);
    assert_eq!(parse(DEFAULT_NOTE_FILENAME), default);
    assert!(default.uses_title());
    assert_eq!(
        default.render(at(), "project-review", 1),
        "2026-11-04_1415_project-review.md"
    );
    assert_eq!(
        default.render(at(), "project-review", 2),
        "2026-11-04_1415_project-review-2.md"
    );
    assert_eq!(default.example(at()), "2026-11-04_1415_project-review.md");
}

#[test]
fn the_date_title_style_leaves_out_the_time() {
    let style = parse(DATE_TITLE_NOTE_FILENAME);

    assert_eq!(style.template(), "{date}-{title}");
    assert_eq!(style.example(at()), "2026-11-04-project-review.md");
    assert_eq!(
        style.render(at(), "project-review", 3),
        "2026-11-04-project-review-3.md"
    );
}

#[test]
fn templates_are_saved_in_canonical_form() {
    for (typed, canonical) in [
        ("{DATE}-{Title}", "{date}-{title}"),
        ("  { date }_{ TIME }  ", "{date}_{time}"),
        ("{date}-{title}.md", "{date}-{title}"),
        ("{date}-{title}.MD ", "{date}-{title}"),
        ("Meeting {title}", "Meeting {title}"),
    ] {
        assert_eq!(parse(typed).template(), canonical, "{typed:?}");
    }
}

#[test]
fn templates_without_a_unique_part_rely_on_collision_suffixes() {
    let inbox = parse("inbox");
    assert!(!inbox.uses_title());
    assert_eq!(inbox.render(at(), "ignored", 1), "inbox.md");
    assert_eq!(inbox.render(at(), "ignored", 2), "inbox-2.md");

    let timestamp = parse("{date}_{time}");
    assert!(!timestamp.uses_title());
    assert_eq!(timestamp.render(at(), "ignored", 1), "2026-11-04_1415.md");
}

#[test]
fn reserved_windows_names_from_titles_get_a_prefix() {
    let title_only = parse("{title}");
    assert_eq!(title_only.render(at(), "con", 1), "note-con.md");
    assert_eq!(title_only.render(at(), "lpt1", 2), "note-lpt1-2.md");
    assert_eq!(title_only.render(at(), "com10", 1), "com10.md");
}

#[test]
fn invalid_templates_explain_what_to_change() {
    let cases = [
        (
            "",
            NoteFilenameError::Empty,
            "empty",
            "Enter a filename, such as {date}_{time}_{title}.",
        ),
        (
            "  .md ",
            NoteFilenameError::Empty,
            "empty",
            "Enter a filename, such as {date}_{time}_{title}.",
        ),
        (
            "{date}_{name}",
            NoteFilenameError::UnknownPlaceholder("name".to_owned()),
            "unknown_placeholder",
            "Unknown placeholder: {name}. Use {date}, {time}, or {title}.",
        ),
        (
            "{date",
            NoteFilenameError::UnbalancedBraces,
            "unbalanced_braces",
            "Close each placeholder with a matching brace, such as {title}.",
        ),
        (
            "date}",
            NoteFilenameError::UnbalancedBraces,
            "unbalanced_braces",
            "Close each placeholder with a matching brace, such as {title}.",
        ),
        (
            "{da{te}",
            NoteFilenameError::UnbalancedBraces,
            "unbalanced_braces",
            "Close each placeholder with a matching brace, such as {title}.",
        ),
        (
            "notes/{title}",
            NoteFilenameError::PathSeparator,
            "path_separator",
            "Enter a filename, not a folder path.",
        ),
        (
            r"notes\{title}",
            NoteFilenameError::PathSeparator,
            "path_separator",
            "Enter a filename, not a folder path.",
        ),
        (
            "{date}: {title}",
            NoteFilenameError::InvalidCharacter(':'),
            "invalid_character",
            "Remove the : character. Windows and Obsidian can't use it in filenames.",
        ),
        (
            "{date}\t{title}",
            NoteFilenameError::ControlCharacter,
            "control_character",
            "Remove tabs and other control characters from the filename.",
        ),
        (
            ".{title}",
            NoteFilenameError::EdgeDot,
            "edge_dot",
            "Filenames can't start or end with a dot.",
        ),
        (
            "{title}.",
            NoteFilenameError::EdgeDot,
            "edge_dot",
            "Filenames can't start or end with a dot.",
        ),
        (
            "CON",
            NoteFilenameError::ReservedName("CON".to_owned()),
            "reserved_name",
            "\"CON\" is reserved by Windows. Add more text to the filename.",
        ),
        (
            "nul.{date}",
            NoteFilenameError::ReservedName("nul".to_owned()),
            "reserved_name",
            "\"nul\" is reserved by Windows. Add more text to the filename.",
        ),
    ];

    for (template, expected, kind, message) in cases {
        let error = NoteFilename::parse(template).unwrap_err();
        assert_eq!(error, expected, "{template:?}");
        assert_eq!(error.kind(), kind, "{template:?}");
        assert_eq!(error.to_string(), message, "{template:?}");
    }
}

#[test]
fn every_character_windows_or_obsidian_links_cannot_use_is_rejected() {
    for character in ['<', '>', ':', '"', '|', '?', '*', '#', '^', '[', ']'] {
        assert_eq!(
            NoteFilename::parse(&format!("{{date}}{character}{{title}}")),
            Err(NoteFilenameError::InvalidCharacter(character)),
        );
    }
}

#[test]
fn names_are_limited_to_leave_room_for_the_vault_path() {
    assert!(NoteFilename::parse(&"a".repeat(MAX_STEM_LENGTH)).is_ok());

    let error = NoteFilename::parse(&"a".repeat(MAX_STEM_LENGTH + 1)).unwrap_err();
    assert_eq!(error, NoteFilenameError::TooLong);
    assert_eq!(
        error.to_string(),
        format!("Keep the filename to {MAX_STEM_LENGTH} characters or fewer, including the title.")
    );
    assert_eq!(
        NoteFilename::parse(&format!("{}{{title}}", "a".repeat(MAX_STEM_LENGTH - 10))),
        Err(NoteFilenameError::TooLong),
        "the longest generated title must also fit"
    );
}

#[test]
fn diagnostic_kinds_never_include_the_typed_template() {
    let error = NoteFilename::parse("{customer-name}").unwrap_err();

    assert_eq!(error.kind(), "unknown_placeholder");
    assert!(!error.kind().contains("customer"));
}
