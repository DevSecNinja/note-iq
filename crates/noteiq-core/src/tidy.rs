//! Reviews an AI rewrite of a note before the user accepts it: which notes may be rewritten,
//! what changed word by word, and which changes could alter facts. Nothing here calls a model.

use std::collections::BTreeSet;

/// Longer notes drifted in testing: facts were merged or reordered incorrectly, so only short
/// captures are offered for rewriting.
pub const MAX_TIDY_CHARS: usize = 700;

/// The invisible sequence Windows AI text APIs may insert to mark generated text.
pub const AI_WATERMARK: &str = "\u{200B}\u{034F}\u{2060}\u{200B}";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TidyBlock {
    Empty,
    TooLong { limit: usize },
}

impl TidyBlock {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "Write something first, then tidy it up.".to_owned(),
            Self::TooLong { limit } => format!(
                "Tidy up works on notes up to {limit} characters. Longer notes are more likely to be changed incorrectly."
            ),
        }
    }
}

pub fn check_eligible(body: &str) -> Result<(), TidyBlock> {
    if body.trim().is_empty() {
        return Err(TidyBlock::Empty);
    }
    if body.chars().count() > MAX_TIDY_CHARS {
        return Err(TidyBlock::TooLong {
            limit: MAX_TIDY_CHARS,
        });
    }
    Ok(())
}

/// Normalizes a model response for the editor: Windows line endings become `\n`, surrounding
/// blank space is trimmed, and a trailing newline is kept only when the original had one.
pub fn clean_output(raw: &str, original: &str) -> String {
    let mut text = raw.replace("\r\n", "\n").trim().to_owned();
    if original.ends_with('\n') && !text.is_empty() {
        text.push('\n');
    }
    text
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    Same,
    Removed,
    Added,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Change {
    pub kind: ChangeKind,
    pub text: String,
}

/// Words, single punctuation marks, runs of spaces, and line breaks.
fn tokens(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut kind = 0_u8;
    for (offset, character) in text.char_indices() {
        let current = if character.is_alphanumeric() {
            1
        } else if character == ' ' || character == '\t' {
            2
        } else {
            3
        };
        if let Some(begin) = start {
            if current == kind && current != 3 {
                continue;
            }
            tokens.push(&text[begin..offset]);
        }
        start = Some(offset);
        kind = current;
    }
    if let Some(begin) = start {
        tokens.push(&text[begin..]);
    }
    tokens
}

/// A word-level diff of `before` and `after`. Adjacent changes of the same kind are merged,
/// and removals are listed before the additions that replace them.
pub fn diff_words(before: &str, after: &str) -> Vec<Change> {
    let (before, after) = (
        before.replace(AI_WATERMARK, ""),
        after.replace(AI_WATERMARK, ""),
    );
    let (before, after) = (tokens(&before), tokens(&after));
    let (rows, columns) = (before.len(), after.len());
    let mut steps = Vec::new();
    if rows.saturating_mul(columns) > 2_000_000 {
        steps.extend(before.iter().map(|token| (ChangeKind::Removed, *token)));
        steps.extend(after.iter().map(|token| (ChangeKind::Added, *token)));
    } else {
        // lengths[i][j] is the longest common subsequence of before[i..] and after[j..].
        let mut lengths = vec![0_u32; (rows + 1) * (columns + 1)];
        let at = |i: usize, j: usize| i * (columns + 1) + j;
        for i in (0..rows).rev() {
            for j in (0..columns).rev() {
                lengths[at(i, j)] = if before[i] == after[j] {
                    lengths[at(i + 1, j + 1)] + 1
                } else {
                    lengths[at(i + 1, j)].max(lengths[at(i, j + 1)])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < rows || j < columns {
            if i < rows && j < columns && before[i] == after[j] {
                steps.push((ChangeKind::Same, before[i]));
                i += 1;
                j += 1;
            } else if i < rows && (j == columns || lengths[at(i + 1, j)] >= lengths[at(i, j + 1)]) {
                steps.push((ChangeKind::Removed, before[i]));
                i += 1;
            } else {
                steps.push((ChangeKind::Added, after[j]));
                j += 1;
            }
        }
    }
    group(steps)
}

/// Merges runs into changes, listing each run's removals before its additions. Spaces shared
/// between edits stay `Same`, so the renderer can tell them from whitespace that changed.
fn group(steps: Vec<(ChangeKind, &str)>) -> Vec<Change> {
    let mut changes: Vec<Change> = Vec::new();
    let mut removed = String::new();
    let mut added = String::new();
    let flush = |changes: &mut Vec<Change>, removed: &mut String, added: &mut String| {
        for (kind, text) in [
            (ChangeKind::Removed, std::mem::take(removed)),
            (ChangeKind::Added, std::mem::take(added)),
        ] {
            if !text.is_empty() {
                changes.push(Change { kind, text });
            }
        }
    };
    for (kind, text) in steps {
        match kind {
            ChangeKind::Removed => removed.push_str(text),
            ChangeKind::Added => added.push_str(text),
            ChangeKind::Same => {
                flush(&mut changes, &mut removed, &mut added);
                match changes.last_mut() {
                    Some(last) if last.kind == ChangeKind::Same => last.text.push_str(text),
                    _ => changes.push(Change {
                        kind,
                        text: text.to_owned(),
                    }),
                }
            }
        }
    }
    flush(&mut changes, &mut removed, &mut added);
    changes
}

const REMOVED_COLOR: &str = "#b91c1c";
const ADDED_COLOR: &str = "#15803d";
const LINE_BREAK_MARK: char = '¶';

/// Text on one side of an edit, with whether it changed or is a space shared between edits.
type Side = Vec<(String, bool)>;

/// Renders `changes` as the Markdown subset Slint's `StyledText` accepts: removals struck
/// through in red, additions underlined in green. Every line of the note stays a line, and
/// the note's own Markdown is escaped so it shows literally. A replaced phrase reads as one
/// removal followed by one addition. Changed whitespace at the edge of a change, where it
/// would otherwise look unchanged, shows as marks: `·` for a space, `→` for a tab, `¶` for a
/// line break.
pub fn diff_markdown(changes: &[Change]) -> String {
    let mut output = String::new();
    let mut at_line_start = true;
    let (mut removed, mut added): (Side, Side) = (Vec::new(), Vec::new());
    for (index, change) in changes.iter().enumerate() {
        match change.kind {
            ChangeKind::Removed => removed.push((change.text.clone(), true)),
            ChangeKind::Added => added.push((change.text.clone(), true)),
            ChangeKind::Same => {
                let bridges = (!removed.is_empty() || !added.is_empty())
                    && change.text.trim().is_empty()
                    && !change.text.contains('\n')
                    && changes
                        .get(index + 1)
                        .is_some_and(|next| next.kind != ChangeKind::Same);
                if bridges {
                    removed.push((change.text.clone(), false));
                    added.push((change.text.clone(), false));
                    continue;
                }
                flush_edit(&mut output, &mut at_line_start, &mut removed, &mut added);
                push_same(&mut output, &mut at_line_start, &change.text);
            }
        }
    }
    flush_edit(&mut output, &mut at_line_start, &mut removed, &mut added);
    output
}

fn push_same(output: &mut String, at_line_start: &mut bool, text: &str) {
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            output.push('\n');
            *at_line_start = true;
        }
        let core = line.trim();
        if core.is_empty() {
            push_spaces(output, line, *at_line_start);
            continue;
        }
        push_spaces(
            output,
            &line[..line.len() - line.trim_start().len()],
            *at_line_start,
        );
        output.push_str(&escape_markdown(core));
        *at_line_start = false;
        push_spaces(output, &line[line.trim_end().len()..], false);
    }
}

fn flush_edit(output: &mut String, at_line_start: &mut bool, removed: &mut Side, added: &mut Side) {
    // A space shared after the last edit on one side belongs to neither side alone.
    for side in [&mut *removed, &mut *added] {
        while side.last().is_some_and(|(_, changed)| !changed) {
            side.pop();
        }
    }
    if removed.iter().any(|(_, changed)| *changed) {
        push_side(output, at_line_start, ChangeKind::Removed, removed);
    }
    if added.iter().any(|(_, changed)| *changed) {
        // A replacement reads as "old new" rather than one run-together word.
        if !removed.is_empty()
            && !output.ends_with([' ', '\u{00A0}', '\n'])
            && !added[0].0.starts_with(char::is_whitespace)
        {
            output.push(' ');
        }
        push_side(output, at_line_start, ChangeKind::Added, added);
    }
    removed.clear();
    added.clear();
}

/// Renders one side of an edit line by line. Whitespace that changed is marked where it
/// starts or ends a line of the edit; between words it stays a plain space, and so does a
/// single space separating the edit's words from unchanged text.
fn push_side(output: &mut String, at_line_start: &mut bool, kind: ChangeKind, side: &Side) {
    // Characters with whether they changed; changed line breaks become marks.
    let mut characters: Vec<(char, bool)> = Vec::new();
    for (text, changed) in side {
        for character in text.chars() {
            match character {
                '\n' if kind == ChangeKind::Removed => characters.push((LINE_BREAK_MARK, true)),
                '\n' => {
                    characters.push((LINE_BREAK_MARK, true));
                    characters.push(('\n', true));
                }
                _ => characters.push((character, *changed)),
            }
        }
    }
    let has_words = characters
        .iter()
        .any(|(c, _)| !c.is_whitespace() && *c != LINE_BREAK_MARK);
    for (index, line) in characters.split(|(c, _)| *c == '\n').enumerate() {
        if index > 0 {
            output.push('\n');
            *at_line_start = true;
        }
        let first = line.iter().position(|(c, _)| !c.is_whitespace());
        let last = line.iter().rposition(|(c, _)| !c.is_whitespace());
        let separator = |edge: &[(char, bool)]| has_words && matches!(edge, [(' ', _)]);
        let (mark_leading, mark_trailing) = match (first, last) {
            (Some(first), Some(last)) => {
                (!separator(&line[..first]), !separator(&line[last + 1..]))
            }
            _ => (true, true),
        };
        let shown: String = line
            .iter()
            .enumerate()
            .map(|(at, (c, changed))| {
                let edge = if first.is_none_or(|first| at < first) {
                    mark_leading
                } else {
                    last.is_some_and(|last| at > last) && mark_trailing
                };
                match (c, *changed && edge) {
                    ('\t', true) => '→',
                    (c, true) if c.is_whitespace() => '·',
                    (c, _) => *c,
                }
            })
            .collect();
        let core = shown.trim();
        if core.is_empty() {
            push_spaces(output, &shown, *at_line_start);
            continue;
        }
        push_spaces(
            output,
            &shown[..shown.len() - shown.trim_start().len()],
            *at_line_start,
        );
        push_styled(output, kind, core);
        *at_line_start = false;
        push_spaces(output, &shown[shown.trim_end().len()..], false);
    }
}

fn push_styled(output: &mut String, kind: ChangeKind, text: &str) {
    let escaped = escape_markdown(text);
    match kind {
        ChangeKind::Same => output.push_str(&escaped),
        ChangeKind::Removed => output.push_str(&format!(
            "<font color=\"{REMOVED_COLOR}\">~~{escaped}~~</font>"
        )),
        ChangeKind::Added => output.push_str(&format!(
            "<font color=\"{ADDED_COLOR}\"><u>{escaped}</u></font>"
        )),
    }
}

/// Spaces at the start of a line would otherwise turn into Markdown indentation.
fn push_spaces(output: &mut String, spaces: &str, at_line_start: bool) {
    if at_line_start {
        output.extend(spaces.chars().map(|_| '\u{00A0}'));
    } else {
        output.push_str(spaces);
    }
}

fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii_punctuation() {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// A change in a rewrite that could alter what the note says, worth checking before applying.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactWarning {
    NumberRemoved(String),
    NumberAdded(String),
    NegationRemoved,
    NegationAdded,
    UncertaintyRemoved,
    FirstPersonRemoved,
    SymbolAdded(char),
    NameRemoved(String),
    NameAdded(String),
    MuchShorter,
}

impl FactWarning {
    pub fn message(&self) -> String {
        match self {
            Self::NumberRemoved(number) => format!("\"{number}\" is no longer in the note."),
            Self::NumberAdded(number) => format!("\"{number}\" was added."),
            Self::NegationRemoved => {
                "A negation such as \"not\" or \"no\" may have been removed.".to_owned()
            }
            Self::NegationAdded => "A negation such as \"not\" or \"no\" was added.".to_owned(),
            Self::UncertaintyRemoved => {
                "Something marked as uncertain (such as \"??\" or \"maybe\") may now read as certain."
                    .to_owned()
            }
            Self::FirstPersonRemoved => {
                "The note may no longer say what you will do yourself (\"me\", \"I\").".to_owned()
            }
            Self::SymbolAdded(symbol) => format!("\"{symbol}\" was added."),
            Self::NameRemoved(name) => format!("\"{name}\" is no longer in the note."),
            Self::NameAdded(name) => format!("\"{name}\" was added."),
            Self::MuchShorter => {
                "The suggestion is less than half as long. Check that nothing important was dropped."
                    .to_owned()
            }
        }
    }
}

const NEGATIONS: [&str; 10] = [
    "not", "no", "never", "none", "nobody", "nothing", "neither", "nor", "cannot", "without",
];
const UNCERTAIN_WORDS: [&str; 12] = [
    "maybe",
    "might",
    "perhaps",
    "possibly",
    "possible",
    "tentative",
    "tentatively",
    "unclear",
    "unsure",
    "tbd",
    "probably",
    "likely",
];
const FIRST_PERSON: [&str; 5] = ["i", "me", "my", "mine", "myself"];
const SYMBOLS: [char; 6] = ['$', '€', '£', '¥', '%', '@'];

fn words(text: &str) -> Vec<String> {
    tokens(text)
        .into_iter()
        .filter(|token| token.chars().next().is_some_and(char::is_alphanumeric))
        .map(str::to_lowercase)
        .collect()
}

/// Numbers keyed by value, so "2,000" matches "2000" and "40k" matches "40000". Decimals and
/// unit suffixes stay in the key, so "1.5m" differs from "2m" and "5mg" from "50mg". The value
/// maps to the form shown in the note.
fn numbers(text: &str) -> Vec<(String, String)> {
    let characters: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let starts_word = index == 0 || !characters[index - 1].is_alphanumeric();
        if !(characters[index].is_ascii_digit() && starts_word) {
            index += 1;
            continue;
        }
        let start = index;
        while index < characters.len()
            && (characters[index].is_ascii_digit()
                || (matches!(characters[index], ',' | '.')
                    && characters.get(index + 1).is_some_and(char::is_ascii_digit)))
        {
            index += 1;
        }
        let digits: String = characters[start..index]
            .iter()
            .filter(|c| **c != ',')
            .collect();
        let suffix_start = index;
        while index < characters.len() && characters[index].is_alphabetic() {
            index += 1;
        }
        let suffix: String = characters[suffix_start..index]
            .iter()
            .collect::<String>()
            .to_lowercase();
        let value = match suffix.as_str() {
            "" | "st" | "nd" | "rd" | "th" => Some(digits),
            "k" if !digits.contains('.') => Some(format!("{digits}000")),
            // Units and scales such as "1.5m", "5mg", "20ms", or "3pm".
            _ if suffix.chars().count() <= 3 => Some(format!("{digits}{suffix}")),
            // Digits glued to a longer word are not a quantity.
            _ => None,
        };
        if let Some(value) = value {
            found.push((value, characters[start..index].iter().collect()));
        }
    }
    found
}

/// Negation words plus contractions such as "didn't", which tokenize as separate pieces.
fn negations(text: &str, words: &[String]) -> usize {
    let lower = text.to_lowercase();
    words
        .iter()
        .filter(|word| NEGATIONS.contains(&word.as_str()))
        .count()
        + lower.matches("n't").count()
        + lower.matches("n\u{2019}t").count()
}

/// Each run of question marks counts once, as do hedging words.
fn uncertainty(text: &str, words: &[String]) -> usize {
    let mut previous = "";
    let mut questions = 0;
    for token in tokens(text) {
        if token == "?" && previous != "?" {
            questions += 1;
        }
        previous = token;
    }
    questions
        + words
            .iter()
            .filter(|word| UNCERTAIN_WORDS.contains(&word.as_str()))
            .count()
}

/// Words that start a sentence or a list item are capitalized anyway, so only capitalized
/// words elsewhere count as names. With `owner_labels`, a capitalized label before a colon,
/// such as "Alice" in "- Alice: ship beta", counts too, so a dropped owner is noticed.
fn names(text: &str, owner_labels: bool) -> Vec<String> {
    let tokens = tokens(text);
    let mut found = Vec::new();
    let mut sentence_start = true;
    for (index, token) in tokens.iter().enumerate() {
        let first = token.chars().next().unwrap_or(' ');
        if first.is_alphanumeric() {
            let label = owner_labels && tokens.get(index + 1) == Some(&":");
            if (!sentence_start || label) && first.is_uppercase() && token.chars().count() > 1 {
                found.push((*token).to_owned());
            }
            sentence_start = false;
        } else if matches!(*token, "\n" | "." | ":" | "?" | "!" | ";") {
            sentence_start = true;
        } else if !(token.trim().is_empty() || matches!(*token, "-" | "*" | "+" | "#" | ">")) {
            sentence_start = false;
        }
    }
    found
}

/// Day and month names with their usual abbreviations; other words must match exactly, so
/// "Caroline" becoming "Carol" is still a change.
const ABBREVIATIONS: [(&str, &[&str]); 19] = [
    ("monday", &["mon"]),
    ("tuesday", &["tue", "tues"]),
    ("wednesday", &["wed"]),
    ("thursday", &["thu", "thur", "thurs"]),
    ("friday", &["fri"]),
    ("saturday", &["sat"]),
    ("sunday", &["sun"]),
    ("january", &["jan"]),
    ("february", &["feb"]),
    ("march", &["mar"]),
    ("april", &["apr"]),
    ("june", &["jun"]),
    ("july", &["jul"]),
    ("august", &["aug"]),
    ("september", &["sep", "sept"]),
    ("october", &["oct"]),
    ("november", &["nov"]),
    ("december", &["dec"]),
    ("tomorrow", &["tmrw", "tmr"]),
];

fn canonical(word: &str) -> &str {
    ABBREVIATIONS
        .iter()
        .find(|(full, short)| *full == word || short.contains(&word))
        .map_or(word, |(full, _)| full)
}

/// Whether `name` (lowercase) appears among `words`, allowing known abbreviations.
fn mentioned(name: &str, words: &BTreeSet<&str>) -> bool {
    let name = canonical(name);
    words.iter().any(|word| canonical(word) == name)
}

/// The saved note finds the editable text after its only `## Notes` heading, so a suggestion
/// must not add another one outside a code fence: the note would no longer reopen.
pub fn adds_notes_heading(original: &str, suggestion: &str) -> bool {
    // The leading line keeps a "---" block in the text from being read as front matter.
    let count = |text: &str| {
        crate::saved_note::headings(&format!("\n{text}"))
            .iter()
            .filter(|(_, _, heading)| *heading == "## Notes")
            .count()
    };
    count(suggestion) > count(original)
}

/// Deterministic checks for facts a rewrite may have changed. They catch numbers, negations,
/// hedges, owners, symbols, and new names, not every change in meaning; the diff shows the rest.
pub fn fact_warnings(before: &str, after: &str) -> Vec<FactWarning> {
    let after = after.replace(AI_WATERMARK, "");
    let mut warnings = Vec::new();
    let before_numbers = numbers(before);
    let after_numbers = numbers(&after);
    let after_values: BTreeSet<&str> = after_numbers.iter().map(|(v, _)| v.as_str()).collect();
    let before_values: BTreeSet<&str> = before_numbers.iter().map(|(v, _)| v.as_str()).collect();
    let mut seen = BTreeSet::new();
    for (value, shown) in &before_numbers {
        if !after_values.contains(value.as_str()) && seen.insert(value) {
            warnings.push(FactWarning::NumberRemoved(shown.clone()));
        }
    }
    for (value, shown) in &after_numbers {
        if !before_values.contains(value.as_str()) && seen.insert(value) {
            warnings.push(FactWarning::NumberAdded(shown.clone()));
        }
    }

    let before_words = words(before);
    let after_words = words(&after);
    let negation_counts = (
        negations(before, &before_words),
        negations(&after, &after_words),
    );
    if negation_counts.1 < negation_counts.0 {
        warnings.push(FactWarning::NegationRemoved);
    } else if negation_counts.1 > negation_counts.0 {
        warnings.push(FactWarning::NegationAdded);
    }

    if uncertainty(&after, &after_words) < uncertainty(before, &before_words) {
        warnings.push(FactWarning::UncertaintyRemoved);
    }

    let first_person = |words: &[String]| {
        words
            .iter()
            .filter(|word| FIRST_PERSON.contains(&word.as_str()))
            .count()
    };
    if first_person(&after_words) < first_person(&before_words) {
        warnings.push(FactWarning::FirstPersonRemoved);
    }

    for symbol in SYMBOLS {
        if after.contains(symbol) && !before.contains(symbol) {
            warnings.push(FactWarning::SymbolAdded(symbol));
        }
    }

    let known: BTreeSet<&str> = before_words.iter().map(String::as_str).collect();
    let kept: BTreeSet<&str> = after_words.iter().map(String::as_str).collect();
    let mut named = BTreeSet::new();
    for name in names(before, true) {
        let lower = name.to_lowercase();
        if !mentioned(&lower, &kept) && named.insert(lower) {
            warnings.push(FactWarning::NameRemoved(name));
        }
    }
    // Labels the rewrite introduces, such as "Decision:", are not names.
    for name in names(&after, false) {
        let lower = name.to_lowercase();
        if !mentioned(&lower, &known) && named.insert(lower) {
            warnings.push(FactWarning::NameAdded(name));
        }
    }

    if after_words.len() * 2 < before_words.len() {
        warnings.push(FactWarning::MuchShorter);
    }
    warnings
}
