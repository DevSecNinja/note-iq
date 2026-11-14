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

/// Merges runs into changes. Within a run of edits, a space shared by both sides stays with
/// the edits so a replaced phrase reads as one removal followed by one addition.
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
    let mut index = 0;
    while index < steps.len() {
        let (kind, text) = steps[index];
        let pending = !removed.is_empty() || !added.is_empty();
        let bridges = kind == ChangeKind::Same
            && pending
            && text.trim().is_empty()
            && !text.contains('\n')
            && steps
                .get(index + 1)
                .is_some_and(|(next, _)| *next != ChangeKind::Same);
        match kind {
            ChangeKind::Removed => removed.push_str(text),
            ChangeKind::Added => added.push_str(text),
            ChangeKind::Same if bridges => {
                removed.push_str(text);
                added.push_str(text);
            }
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
        index += 1;
    }
    flush(&mut changes, &mut removed, &mut added);
    changes
}

const REMOVED_COLOR: &str = "#b91c1c";
const ADDED_COLOR: &str = "#15803d";
const LINE_BREAK_MARK: &str = "¶";

/// Renders `changes` as the Markdown subset Slint's `StyledText` accepts: removals struck
/// through in red, additions underlined in green. Every line of the note stays a line, and
/// the note's own Markdown is escaped so it shows literally. Changed whitespace that would
/// otherwise look unchanged shows as marks: `·` for a space, `→` for a tab, `¶` for a line
/// break.
pub fn diff_markdown(changes: &[Change]) -> String {
    let mut output = String::new();
    let mut at_line_start = true;
    let mut previous = ChangeKind::Same;
    for change in changes {
        // A replacement reads as "old new" rather than one run-together word.
        if previous == ChangeKind::Removed
            && change.kind == ChangeKind::Added
            && !output.ends_with([' ', '\u{00A0}', '\n'])
            && !change.text.starts_with([' ', '\n'])
        {
            output.push(' ');
        }
        previous = change.kind;
        let whitespace_only = change.kind != ChangeKind::Same && change.text.trim().is_empty();
        let text = match change.kind {
            // A removed line break stays on one line, so joined lines are visible.
            ChangeKind::Removed => change.text.replace('\n', LINE_BREAK_MARK),
            ChangeKind::Added if whitespace_only => {
                change.text.replace('\n', &format!("{LINE_BREAK_MARK}\n"))
            }
            _ => change.text.clone(),
        };
        for (index, line) in text.split('\n').enumerate() {
            if index > 0 {
                output.push('\n');
                at_line_start = true;
            }
            if line.is_empty() {
                continue;
            }
            if whitespace_only {
                let marks: String = line
                    .chars()
                    .map(|c| match c {
                        '\t' => '→',
                        c if c.is_whitespace() => '·',
                        c => c,
                    })
                    .collect();
                push_styled(&mut output, change.kind, &marks);
                at_line_start = false;
                continue;
            }
            let core = line.trim();
            if core.is_empty() {
                push_spaces(&mut output, line, at_line_start);
                continue;
            }
            let leading = &line[..line.len() - line.trim_start().len()];
            let trailing = &line[line.trim_end().len()..];
            push_spaces(&mut output, leading, at_line_start);
            push_styled(&mut output, change.kind, core);
            at_line_start = false;
            push_spaces(&mut output, trailing, at_line_start);
        }
    }
    output
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
/// words elsewhere count as names.
fn names(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut sentence_start = true;
    for token in tokens(text) {
        let first = token.chars().next().unwrap_or(' ');
        if first.is_alphanumeric() {
            if !sentence_start && first.is_uppercase() && token.chars().count() > 1 {
                found.push(token.to_owned());
            }
            sentence_start = false;
        } else if matches!(token, "\n" | "." | ":" | "?" | "!" | ";") {
            sentence_start = true;
        } else if !(token.trim().is_empty() || matches!(token, "-" | "*" | "+" | "#" | ">")) {
            sentence_start = false;
        }
    }
    found
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
    for name in names(before) {
        let lower = name.to_lowercase();
        // "Fri" becoming "Friday", or the reverse, still names the same thing.
        let shortened_or_expanded = kept.iter().any(|word| {
            word.chars().count() >= 3 && (lower.starts_with(word) || word.starts_with(&lower))
        });
        if !kept.contains(lower.as_str()) && !shortened_or_expanded && named.insert(lower) {
            warnings.push(FactWarning::NameRemoved(name));
        }
    }
    for name in names(&after) {
        let lower = name.to_lowercase();
        // "fri" becoming "Friday" or "q1" becoming "Q1" is an expansion, not a new name.
        let expanded = known.iter().any(|word| {
            word.chars().count() >= 3 && (lower.starts_with(word) || word.starts_with(&lower))
        });
        if !known.contains(lower.as_str()) && !expanded && named.insert(lower) {
            warnings.push(FactWarning::NameAdded(name));
        }
    }

    if after_words.len() * 2 < before_words.len() {
        warnings.push(FactWarning::MuchShorter);
    }
    warnings
}
