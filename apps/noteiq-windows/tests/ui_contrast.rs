//! WCAG 2.2 AA contrast checks for every text colour in the capture window.

const MAIN_WINDOW: &str = include_str!("../ui/main.slint");
const BADGE_BACKGROUND: &str = "#4f46e5";
const AA_NORMAL_TEXT: f64 = 4.5;

fn hex_tokens(line: &str) -> Vec<&str> {
    line.split(|c: char| c.is_whitespace() || c == ';' || c == ':')
        .filter(|token| {
            token.len() == 7
                && token.starts_with('#')
                && token[1..].chars().all(|c| c.is_ascii_hexdigit())
        })
        .collect()
}

fn declarations(property: &str) -> Vec<String> {
    let prefix = format!("{property}:");
    MAIN_WINDOW
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with(&prefix))
        .map(str::to_owned)
        .collect()
}

fn colours(property: &str) -> Vec<String> {
    let mut colours: Vec<String> = declarations(property)
        .iter()
        .flat_map(|line| hex_tokens(line).into_iter().map(str::to_lowercase))
        .collect();
    colours.sort();
    colours.dedup();
    colours
}

fn channel(value: u8) -> f64 {
    let srgb = f64::from(value) / 255.0;
    if srgb <= 0.040_45 {
        srgb / 12.92
    } else {
        ((srgb + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(hex: &str) -> f64 {
    let rgb = |range| u8::from_str_radix(&hex[range], 16).unwrap();
    0.2126 * channel(rgb(1..3)) + 0.7152 * channel(rgb(3..5)) + 0.0722 * channel(rgb(5..7))
}

fn contrast(foreground: &str, background: &str) -> f64 {
    let (a, b) = (luminance(foreground), luminance(background));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn contrast_formula_matches_wcag_reference_values() {
    assert!((contrast("#000000", "#ffffff") - 21.0).abs() < 0.01);
    assert!((contrast("#ffffff", "#ffffff") - 1.0).abs() < 0.01);
    assert!((contrast("#767676", "#ffffff") - 4.54).abs() < 0.01);
}

#[test]
fn every_text_colour_meets_aa_on_every_surface() {
    let text = colours("color");
    let surfaces: Vec<String> = colours("background")
        .into_iter()
        .filter(|colour| colour != BADGE_BACKGROUND)
        .collect();
    assert!(
        text.len() >= 5,
        "expected the window's text colours: {text:?}"
    );
    assert!(
        surfaces.len() >= 2,
        "expected the window surfaces: {surfaces:?}"
    );

    let failures: Vec<String> = text
        .iter()
        .flat_map(|foreground| {
            surfaces.iter().filter_map(move |background| {
                let ratio = contrast(foreground, background);
                (ratio < AA_NORMAL_TEXT)
                    .then(|| format!("{foreground} on {background} is {ratio:.2}:1"))
            })
        })
        .collect();
    assert!(failures.is_empty(), "contrast below 4.5:1: {failures:?}");
}

#[test]
fn badge_initial_meets_aa_on_the_brand_colour() {
    assert!(
        colours("background").contains(&BADGE_BACKGROUND.to_owned()),
        "badge colour changed; update this test"
    );
    assert!(
        declarations("color")
            .iter()
            .any(|line| line == "color: white;")
    );
    assert!(contrast("#ffffff", BADGE_BACKGROUND) >= AA_NORMAL_TEXT);
}
