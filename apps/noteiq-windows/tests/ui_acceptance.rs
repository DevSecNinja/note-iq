//! Headless acceptance checks for the capture window. They drive the real Slint component
//! through Slint's testing backend, using the accessibility labels screen readers rely on.
#![cfg(windows)]

#[allow(unsafe_code)]
mod generated {
    slint::include_modules!();
}

use generated::{MainWindow, MeetingRow, Palette};
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{
    ComponentHandle, LogicalSize, ModelRc, SharedString, VecModel,
    platform::{Key, PointerEventButton, WindowEvent},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn open_window() -> MainWindow {
    i_slint_backend_testing::init_no_event_loop();
    let window = MainWindow::new().expect("create the capture window");
    window.window().set_size(LogicalSize::new(760.0, 640.0));
    window.show().expect("show the capture window");
    window
}

fn counter() -> Rc<Cell<u32>> {
    Rc::new(Cell::new(0))
}

fn increment(count: &Rc<Cell<u32>>) -> impl Fn() + 'static {
    let count = count.clone();
    move || count.set(count.get() + 1)
}

fn key(window: &MainWindow, text: SharedString) {
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn type_text(window: &MainWindow, text: &str) {
    for character in text.chars() {
        key(window, character.to_string().into());
    }
}

fn press_ctrl(window: &MainWindow, letter: &str) {
    let control: SharedString = Key::Control.into();
    window.window().dispatch_event(WindowEvent::KeyPressed {
        text: control.clone(),
    });
    key(window, letter.into());
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text: control });
}

/// Hidden elements are skipped, so this only finds controls the user can see.
fn find(window: &MainWindow, label: &str, role: AccessibleRole) -> Option<ElementHandle> {
    let mut found = ElementHandle::find_by_accessible_label(window, label)
        .filter(|element| element.accessible_role() == Some(role));
    let element = found.next();
    assert!(
        found.next().is_none(),
        "several {role:?}s labelled {label:?}"
    );
    element
}

fn element(window: &MainWindow, label: &str, role: AccessibleRole) -> ElementHandle {
    find(window, label, role).unwrap_or_else(|| panic!("no visible {role:?} labelled {label:?}"))
}

fn offers_button(window: &MainWindow, label: &str) -> bool {
    find(window, label, AccessibleRole::Button).is_some()
}

fn click_button(window: &MainWindow, label: &str) {
    element(window, label, AccessibleRole::Button).mock_single_click(PointerEventButton::Left);
}

fn shows_text(window: &MainWindow, text: &str) -> bool {
    ElementHandle::find_by_accessible_label(window, text)
        .any(|element| element.accessible_role() == Some(AccessibleRole::Text))
}

fn record_edits(window: &MainWindow) -> Rc<RefCell<Vec<String>>> {
    let edits = Rc::new(RefCell::new(Vec::new()));
    let sink = edits.clone();
    window.on_body_changed(move |text| sink.borrow_mut().push(text.into()));
    edits
}

const NO_MEETING: &str = "No meeting";
const PLANNING: &str = "Sprint planning · 04 Nov · 08:00–08:45";
const STANDUP: &str = "Standup · 04 Nov · 09:00–09:15";
const REVIEW: &str = "Design review · 04 Nov · 09:30–10:00";

/// A list row for a label such as `Standup · 04 Nov · 09:00–09:15`.
fn row(label: &str) -> MeetingRow {
    let mut parts = label.split(" · ");
    let title = parts.next().unwrap_or_default();
    let time = parts.nth(1).and_then(|span| span.split('–').next());
    MeetingRow {
        label: label.into(),
        title: title.into(),
        time: time.unwrap_or_default().into(),
        details: format!("{title} details").into(),
        ..MeetingRow::default()
    }
}

/// Mirrors the controller: row 0 is always "No meeting", followed by the meetings.
fn show_rows(window: &MainWindow, rows: Vec<MeetingRow>, index: i32) {
    let rows: Vec<MeetingRow> = std::iter::once(MeetingRow {
        label: NO_MEETING.into(),
        title: NO_MEETING.into(),
        ..MeetingRow::default()
    })
    .chain(rows)
    .collect();
    window.set_meeting_rows(ModelRc::new(VecModel::from(rows)));
    window.set_meeting_index(index);
    window.set_meeting_checking(false);
}

fn show_meetings(window: &MainWindow, meetings: &[&str], index: i32) {
    show_rows(
        window,
        meetings.iter().map(|label| row(label)).collect(),
        index,
    );
}

/// Runs the change handlers Slint defers to the next frame.
fn settle() {
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(16));
}

fn picker(window: &MainWindow) -> ElementHandle {
    element(window, "Meeting", AccessibleRole::Combobox)
}

fn record_meeting_choices(window: &MainWindow) -> Rc<RefCell<Vec<i32>>> {
    let choices = Rc::new(RefCell::new(Vec::new()));
    let sink = choices.clone();
    window.on_meeting_chosen(move |index| sink.borrow_mut().push(index));
    choices
}

/// Opens the picker like a mouse user and clicks one of its options.
fn choose(window: &MainWindow, option: &str) {
    picker(window).mock_single_click(PointerEventButton::Left);
    element(window, option, AccessibleRole::ListItem).mock_single_click(PointerEventButton::Left);
    settle();
}

fn recorder() -> (Rc<RefCell<Vec<String>>>, impl Fn(SharedString) + 'static) {
    let values = Rc::new(RefCell::new(Vec::new()));
    let sink = values.clone();
    (values, move |text: SharedString| {
        sink.borrow_mut().push(text.into())
    })
}

fn assert_inside_window(window: &MainWindow, element: &ElementHandle) {
    let position = element.absolute_position();
    let size = element.size();
    let bounds = window
        .window()
        .size()
        .to_logical(window.window().scale_factor());
    assert!(
        position.x >= 0.0
            && position.y >= 0.0
            && position.x + size.width <= bounds.width + 1.0
            && position.y + size.height <= bounds.height + 1.0,
        "control at {position:?}, size {size:?} exceeds window {bounds:?}"
    );
}

#[test]
fn app_icon_is_embedded_and_can_supply_transparent_tray_pixels() {
    let window = open_window();
    let image = window.get_app_icon();
    assert_eq!((image.size().width, image.size().height), (256, 256));
    let pixels = image.to_rgba8().expect("the tray can decode the app icon");
    let pixel = |x: usize, y: usize| &pixels.as_slice()[y * 256 + x];
    assert_eq!(pixel(0, 0).a, 0, "icon corners should be transparent");
    assert_eq!(pixel(40, 128), &slint::Rgba8Pixel::new(79, 70, 229, 255));
    assert_eq!(pixel(84, 128), &slint::Rgba8Pixel::new(255, 255, 255, 255));
}

#[test]
fn executable_icon_contains_decodable_frames_for_windows_display_scales() {
    let _window = open_window();
    let icon = include_bytes!(concat!(env!("OUT_DIR"), "/noteiq.ico"));
    let sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
    assert_eq!(&icon[..6], &[0, 0, 1, 0, 9, 0]);
    let mut expected_offset = 6 + 16 * sizes.len();
    for (index, size) in sizes.into_iter().enumerate() {
        let entry = &icon[6 + 16 * index..6 + 16 * (index + 1)];
        let dimension = if size == 256 { 0 } else { size as u8 };
        assert_eq!(&entry[..8], &[dimension, dimension, 0, 0, 1, 0, 32, 0]);
        let length = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        assert_eq!(offset, expected_offset);
        let image = slint::Image::load_from_data(&icon[offset..offset + length], Some("png"))
            .expect("each ICO frame is a valid PNG");
        assert_eq!((image.size().width, image.size().height), (size, size));
        assert_eq!(image.to_rgba8().unwrap().as_slice()[0].a, 0);
        expected_offset += length;
    }
    assert_eq!(expected_offset, icon.len(), "every frame is accounted for");
}

#[test]
fn widget_palette_is_explicitly_light_to_match_the_capture_surfaces() {
    let window = open_window();
    assert_eq!(
        window.global::<Palette>().get_color_scheme(),
        slint::language::ColorScheme::Light,
        "system dark mode must not put white widget text on light capture surfaces"
    );
}

#[test]
fn hidden_settings_leave_room_for_the_note_and_footer() {
    let window = open_window();
    let editor = element(&window, "Note contents", AccessibleRole::TextInput);
    assert!(
        editor.size().height >= 280.0,
        "note editor is only {:?} at the default window size",
        editor.size()
    );
    assert_inside_window(&window, &editor);
    for label in ["New note", "Settings"] {
        assert_inside_window(&window, &element(&window, label, AccessibleRole::Button));
    }
}

#[test]
fn the_capture_window_has_no_header_or_save_button() {
    let window = open_window();
    window.set_has_note(true);
    for label in ["Save", "Refresh", "Refresh meetings"] {
        assert!(
            !offers_button(&window, label),
            "unexpected {label:?} button"
        );
    }
    for text in ["NoteIQ", "Quick capture", "Draft", "Unsaved"] {
        assert!(!shows_text(&window, text), "unexpected {text:?} text");
    }
    let editor = element(&window, "Note contents", AccessibleRole::TextInput);
    assert_eq!(
        editor.accessible_placeholder_text().as_deref(),
        Some("Start typing… notes save automatically.")
    );
}

#[test]
fn meeting_picker_and_note_fit_the_minimum_window_size() {
    let window = open_window();
    window.window().set_size(LogicalSize::new(640.0, 540.0));
    show_meetings(&window, &[STANDUP, REVIEW], 2);
    let hint = "Checking your calendar...";
    window.set_meeting_hint(hint.into());
    settle();

    let picker = picker(&window);
    assert_eq!(picker.accessible_value().as_deref(), Some(REVIEW));
    assert_inside_window(&window, &picker);
    assert_inside_window(&window, &element(&window, hint, AccessibleRole::Text));
    let editor = element(&window, "Note contents", AccessibleRole::TextInput);
    assert!(
        editor.size().height >= 240.0,
        "the meeting card squeezed the editor to {:?}",
        editor.size()
    );
    assert_inside_window(&window, &editor);
    for label in ["New note", "Settings"] {
        assert_inside_window(&window, &element(&window, label, AccessibleRole::Button));
    }
}

#[test]
fn clearing_the_meeting_caption_returns_space_to_the_editor() {
    let window = open_window();
    let message = "Couldn't refresh meetings. Showing earlier results.";
    window.set_meeting_hint(message.into());
    settle();
    assert!(shows_text(&window, message));
    let with_caption = element(&window, "Note contents", AccessibleRole::TextInput)
        .size()
        .height;
    window.set_meeting_hint("".into());
    settle();
    assert!(!shows_text(&window, message));
    let without_caption = element(&window, "Note contents", AccessibleRole::TextInput)
        .size()
        .height;
    assert!(
        without_caption > with_caption,
        "empty captions must not consume an editor row"
    );
}

#[test]
fn settings_fit_without_discarding_the_note_or_selected_meeting() {
    let window = open_window();
    window.window().set_size(LogicalSize::new(640.0, 540.0));
    let choices = record_meeting_choices(&window);
    type_text(&window, "Keep this draft");
    show_meetings(&window, &[REVIEW], 1);

    click_button(&window, "Settings");
    assert_eq!(picker(&window).accessible_enabled(), Some(false));
    picker(&window).mock_single_click(PointerEventButton::Left);
    assert!(
        find(&window, REVIEW, AccessibleRole::ListItem).is_none(),
        "the meeting list stays closed while Settings is open"
    );
    let done = element(&window, "Done", AccessibleRole::Button);
    assert_inside_window(&window, &done);
    assert_inside_window(
        &window,
        &element(&window, "Capture shortcut", AccessibleRole::TextInput),
    );
    assert_inside_window(
        &window,
        &element(&window, "Note filename template", AccessibleRole::TextInput),
    );
    click_button(&window, "Done");
    type_text(&window, "!");
    settle();

    assert_eq!(window.get_note_text(), "Keep this draft!");
    assert_eq!(picker(&window).accessible_value().as_deref(), Some(REVIEW));
    assert!(
        choices.borrow().is_empty(),
        "Settings must not change the meeting"
    );
}

#[test]
fn note_editor_preserves_apostrophes_quotes_and_composed_text() {
    let window = open_window();
    let edits = record_edits(&window);
    let text = "I'm writing 'notes', \"quotes\", café and naïve.";

    type_text(&window, text);
    // A non-composing dead-key sequence can commit multiple characters at once.
    key(&window, "'x".into());

    let expected = format!("{text}'x");
    assert_eq!(window.get_note_text().as_str(), expected);
    assert_eq!(edits.borrow().last(), Some(&expected));
}

#[test]
fn note_editor_has_keyboard_focus_when_the_window_opens() {
    let window = open_window();
    let edits = record_edits(&window);

    type_text(&window, "Hi");

    assert_eq!(edits.borrow().last().map(String::as_str), Some("Hi"));
    assert_eq!(window.get_note_text(), "Hi");
}

#[test]
fn ctrl_s_saves_without_typing_into_the_note() {
    let window = open_window();
    let saves = counter();
    window.on_save_requested(increment(&saves));

    type_text(&window, "Plan");
    press_ctrl(&window, "s");

    assert_eq!(saves.get(), 1);
    assert_eq!(window.get_note_text(), "Plan");
}

#[test]
fn ctrl_n_requests_a_new_note() {
    let window = open_window();
    let new_notes = counter();
    window.on_new_note_requested(increment(&new_notes));

    press_ctrl(&window, "n");

    assert_eq!(new_notes.get(), 1);
}

#[test]
fn new_note_button_uses_the_same_action_as_ctrl_n_without_directly_erasing_text() {
    let window = open_window();
    let new_notes = counter();
    window.on_new_note_requested(increment(&new_notes));
    type_text(&window, "Unsaved note");
    click_button(&window, "New note");
    assert_eq!(new_notes.get(), 1);
    assert_eq!(window.get_note_text(), "Unsaved note");
    window.window().set_size(LogicalSize::new(640.0, 540.0));
    assert_inside_window(
        &window,
        &element(&window, "New note", AccessibleRole::Button),
    );
}

#[test]
fn meeting_change_confirmation_offers_change_new_and_cancel_actions() {
    let window = open_window();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let action_sink = actions.clone();
    window.on_meeting_change_confirmed(move |action| {
        action_sink.borrow_mut().push(action);
    });
    let cancellations = counter();
    window.on_meeting_change_cancelled(increment(&cancellations));
    let prompt = "You are about to change the meeting for this note. Do you want to change it or create a new note?";

    assert!(!shows_text(&window, prompt));
    window.set_meeting_change_confirmation_visible(true);
    settle();
    assert!(shows_text(&window, prompt));

    for label in ["Change meeting", "Create new note", "Cancel"] {
        let button = element(&window, label, AccessibleRole::Button);
        assert_inside_window(&window, &button);
    }

    click_button(&window, "Change meeting");
    assert_eq!(*actions.borrow(), vec![0]);

    click_button(&window, "Create new note");
    assert_eq!(*actions.borrow(), vec![0, 1]);

    click_button(&window, "Cancel");
    assert_eq!(cancellations.get(), 1);
}

#[test]
fn meeting_change_confirmation_blocks_pointer_input_behind_the_dialog() {
    let window = open_window();
    show_meetings(&window, &[STANDUP, REVIEW], 1);
    let choices = record_meeting_choices(&window);
    window.set_meeting_change_confirmation_visible(true);
    settle();

    picker(&window).mock_single_click(PointerEventButton::Left);
    settle();

    assert!(choices.borrow().is_empty());
    assert!(
        find(&window, REVIEW, AccessibleRole::ListItem).is_none(),
        "the meeting picker must not open behind the confirmation"
    );
    assert!(shows_text(
        &window,
        "You are about to change the meeting for this note. Do you want to change it or create a new note?"
    ));
}

#[test]
fn meeting_change_confirmation_supports_escape_and_traps_tab_focus() {
    let window = open_window();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let action_sink = actions.clone();
    window.on_meeting_change_confirmed(move |action| {
        action_sink.borrow_mut().push(action);
    });
    let cancellations = counter();
    let weak_window = window.as_weak();
    let cancellation_sink = cancellations.clone();
    window.on_meeting_change_cancelled(move || {
        cancellation_sink.set(cancellation_sink.get() + 1);
        if let Some(window) = weak_window.upgrade() {
            window.set_meeting_change_confirmation_visible(false);
        }
    });

    window.set_meeting_change_confirmation_visible(true);
    settle();
    key(&window, Key::Return.into());
    settle();
    assert_eq!(
        cancellations.get(),
        1,
        "opening the dialog should focus Cancel"
    );

    window.set_meeting_change_confirmation_visible(true);
    settle();
    key(&window, Key::Tab.into());
    key(&window, Key::Return.into());
    settle();
    assert_eq!(
        *actions.borrow(),
        vec![0],
        "Tab from Cancel should focus Change meeting"
    );

    window.set_meeting_change_confirmation_visible(false);
    settle();
    window.set_meeting_change_confirmation_visible(true);
    settle();
    key(&window, Key::Tab.into());
    key(&window, Key::Tab.into());
    key(&window, Key::Tab.into());
    key(&window, Key::Return.into());
    settle();
    assert_eq!(
        cancellations.get(),
        2,
        "Tab from the last action should wrap focus to Cancel"
    );
    assert!(!window.get_meeting_change_confirmation_visible());

    window.set_meeting_change_confirmation_visible(true);
    settle();
    key(&window, Key::Escape.into());
    settle();

    assert_eq!(cancellations.get(), 3);
    assert!(!window.get_meeting_change_confirmation_visible());
}

#[test]
fn starting_a_new_note_clears_text_typed_into_the_editor() {
    let window = open_window();
    let edits = record_edits(&window);
    type_text(&window, "Old");

    window.set_note_text(SharedString::new());
    type_text(&window, "New");

    assert_eq!(edits.borrow().last().map(String::as_str), Some("New"));
}

#[test]
fn the_footer_shows_a_single_status_line() {
    let window = open_window();
    let status = "Saved · 14:41";
    window.set_has_note(true);
    window.set_saved(true);
    window.set_status_text(status.into());
    assert!(shows_text(&window, status));

    let problem = "Could not save the note: disk full";
    window.set_status_text(problem.into());
    window.set_status_error(true);
    assert!(shows_text(&window, problem));
    assert!(!shows_text(&window, status));
}

#[test]
fn open_note_button_appears_once_the_note_is_saved() {
    let window = open_window();
    let opens = counter();
    window.on_open_requested(increment(&opens));

    assert!(
        !offers_button(&window, "Open note"),
        "Open note must be hidden before the first save"
    );

    window.set_has_note(true);
    window.set_saved(true);
    click_button(&window, "Open note");
    assert_eq!(opens.get(), 1);
}

#[test]
fn picker_starts_on_no_meeting_without_separate_meeting_buttons() {
    let window = open_window();
    let picker = picker(&window);

    assert_eq!(picker.accessible_value().as_deref(), Some(NO_MEETING));
    assert_eq!(
        picker.accessible_enabled(),
        Some(true),
        "the list always offers Refresh meetings"
    );
    assert!(shows_text(&window, "Checking your calendar…"));
    window.set_meeting_checking(false);
    assert!(!shows_text(&window, "Checking your calendar…"));
    for label in ["No meeting", "Connect", "Disconnect", "Find meeting"] {
        assert!(
            !offers_button(&window, label),
            "unexpected {label:?} button"
        );
    }
}

#[test]
fn choosing_from_the_picker_reports_the_option_index() {
    let window = open_window();
    let choices = record_meeting_choices(&window);
    show_meetings(&window, &[STANDUP, REVIEW], 0);

    choose(&window, REVIEW);
    assert_eq!(*choices.borrow(), vec![2]);
    assert_eq!(window.get_meeting_index(), 2);

    choose(&window, NO_MEETING);
    assert_eq!(*choices.borrow(), vec![2, 0]);
    assert_eq!(
        picker(&window).accessible_value().as_deref(),
        Some(NO_MEETING)
    );
}

#[test]
fn meeting_rows_show_time_title_and_details_and_mark_the_current_meeting() {
    let window = open_window();
    window.window().set_size(LogicalSize::new(640.0, 540.0));
    let mut current = row(REVIEW);
    current.details = "ends 10:00 · Alex Morgan · 12 people".into();
    current.now = true;
    let mut ended = row(STANDUP);
    ended.past = true;
    show_rows(&window, vec![current, ended], 1);

    picker(&window).mock_single_click(PointerEventButton::Left);
    assert_eq!(picker(&window).accessible_expanded(), Some(true));
    let item = element(&window, REVIEW, AccessibleRole::ListItem);
    assert_eq!(
        item.accessible_description().as_deref(),
        Some("ends 10:00 · Alex Morgan · 12 people")
    );
    assert_eq!(item.accessible_item_selected(), Some(true));
    assert_eq!(item.accessible_item_index(), Some(1));
    assert_eq!(item.accessible_item_count(), Some(3));
    assert_inside_window(&window, &item);
    assert_inside_window(
        &window,
        &element(&window, "Refresh meetings", AccessibleRole::Button),
    );
}

#[test]
fn the_keyboard_opens_the_list_and_chooses_a_row() {
    let window = open_window();
    let choices = record_meeting_choices(&window);
    show_meetings(&window, &[STANDUP, REVIEW], 1);

    picker(&window).mock_single_click(PointerEventButton::Left);
    settle();
    let standup = element(&window, STANDUP, AccessibleRole::ListItem);
    assert_eq!(
        standup.accessible_item_selected(),
        Some(true),
        "the list opens on the attached meeting"
    );
    key(&window, Key::DownArrow.into());
    settle();
    assert_eq!(
        element(&window, REVIEW, AccessibleRole::ListItem).accessible_item_selected(),
        Some(true),
        "assistive technology follows the arrow keys"
    );
    assert_eq!(standup.accessible_item_selected(), Some(false));
    assert!(
        choices.borrow().is_empty(),
        "arrows only move the highlight"
    );
    key(&window, Key::Return.into());
    settle();

    assert_eq!(*choices.borrow(), vec![2]);
    assert_eq!(window.get_meeting_index(), 2);
    assert!(find(&window, REVIEW, AccessibleRole::ListItem).is_none());
}

#[test]
fn a_reordered_list_clears_the_highlight_so_enter_cannot_choose_the_wrong_meeting() {
    let window = open_window();
    let choices = record_meeting_choices(&window);
    let refreshes = counter();
    window.on_meeting_refresh_requested(increment(&refreshes));
    show_meetings(&window, &[STANDUP, REVIEW], 0);

    picker(&window).mock_single_click(PointerEventButton::Left);
    settle();
    key(&window, Key::DownArrow.into());
    settle();
    show_meetings(&window, &[REVIEW, STANDUP], 0);
    window.set_meeting_list_version(window.get_meeting_list_version() + 1);
    settle();

    key(&window, Key::Return.into());
    settle();
    assert!(
        choices.borrow().is_empty(),
        "nothing is highlighted any more"
    );
    assert_eq!(refreshes.get(), 0);
    assert!(
        find(&window, REVIEW, AccessibleRole::ListItem).is_some(),
        "the list stays open"
    );

    key(&window, Key::DownArrow.into());
    key(&window, Key::Return.into());
    settle();
    assert_eq!(
        *choices.borrow(),
        vec![0],
        "the arrows start again from the attached meeting"
    );
}

#[test]
fn the_list_scrolls_to_the_attached_meeting_when_it_opens() {
    let window = open_window();
    window.window().set_size(LogicalSize::new(640.0, 540.0));
    let labels: Vec<String> = (0..14)
        .map(|hour| {
            format!(
                "Meeting {hour} · 04 Nov · {:02}:00–{:02}:30",
                7 + hour,
                7 + hour
            )
        })
        .collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    show_meetings(&window, &refs, 14);

    picker(&window).mock_single_click(PointerEventButton::Left);
    settle();
    assert_inside_window(
        &window,
        &element(&window, &labels[13], AccessibleRole::ListItem),
    );
    assert_inside_window(
        &window,
        &element(&window, "Refresh meetings", AccessibleRole::Button),
    );
}

#[test]
fn escape_closes_the_picker_without_changing_the_meeting() {
    let window = open_window();
    let choices = record_meeting_choices(&window);
    show_meetings(&window, &[STANDUP, REVIEW], 1);

    picker(&window).mock_single_click(PointerEventButton::Left);
    assert!(
        find(&window, REVIEW, AccessibleRole::ListItem).is_some(),
        "clicking the picker lists the meetings"
    );
    key(&window, Key::Escape.into());
    settle();

    assert!(
        find(&window, REVIEW, AccessibleRole::ListItem).is_none(),
        "Escape closes the list"
    );
    assert!(choices.borrow().is_empty());
    assert_eq!(picker(&window).accessible_value().as_deref(), Some(STANDUP));
}

#[test]
fn updating_the_picker_from_code_keeps_the_selection_and_is_not_a_choice() {
    let window = open_window();
    let choices = record_meeting_choices(&window);

    show_meetings(&window, &[STANDUP, REVIEW], 2);
    settle();
    assert_eq!(picker(&window).accessible_value().as_deref(), Some(REVIEW));

    // A refresh lists an earlier meeting first, so the attached one moves down a row.
    show_meetings(&window, &[PLANNING, STANDUP, REVIEW], 3);
    settle();
    assert_eq!(window.get_meeting_index(), 3);
    assert_eq!(picker(&window).accessible_value().as_deref(), Some(REVIEW));

    // A shorter list must not leave the selection on a stale row.
    show_meetings(&window, &[REVIEW], 1);
    settle();
    assert_eq!(window.get_meeting_index(), 1);
    assert_eq!(picker(&window).accessible_value().as_deref(), Some(REVIEW));

    assert!(
        choices.borrow().is_empty(),
        "updates from code must never look like the user's choice"
    );
}

#[test]
fn refresh_explains_lookup_problems_and_waits_for_a_running_lookup() {
    let window = open_window();
    let refreshes = counter();
    window.on_meeting_refresh_requested(increment(&refreshes));
    let message = "Couldn't refresh meetings. Showing earlier results.";

    window.set_meeting_hint(message.into());
    assert!(shows_text(&window, message));
    assert!(
        !offers_button(&window, "Refresh meetings"),
        "Refresh lives in the meeting list"
    );
    picker(&window).mock_single_click(PointerEventButton::Left);
    click_button(&window, "Refresh meetings");
    assert_eq!(refreshes.get(), 1);

    window.set_meeting_refresh_enabled(false);
    let refresh = element(&window, "Refresh meetings", AccessibleRole::Button);
    assert_eq!(refresh.accessible_enabled(), Some(false));
    refresh.mock_single_click(PointerEventButton::Left);
    assert_eq!(refreshes.get(), 1, "a lookup is already running");
}

#[test]
fn unwritable_folder_offers_a_folder_choice_in_the_footer() {
    let window = open_window();
    let choices = counter();
    window.on_destination_requested(increment(&choices));
    let footer_fix = |window: &MainWindow| find(window, "Choose folder", AccessibleRole::Button);
    assert!(
        footer_fix(&window).is_none(),
        "no folder prompt while saving works"
    );

    let message = "Can't save to this folder. Choose another folder.";
    window.set_destination_error(true);
    window.set_status_text(message.into());

    assert!(shows_text(&window, message));
    footer_fix(&window)
        .expect("footer folder button")
        .mock_single_click(PointerEventButton::Left);
    assert_eq!(choices.get(), 1);
}

#[test]
fn settings_panel_opens_and_returns_to_the_note() {
    let window = open_window();
    let requests = Rc::new(RefCell::new(Vec::new()));
    let sink = requests.clone();
    window.on_settings_requested(move |visible| sink.borrow_mut().push(visible));

    click_button(&window, "Settings");
    assert!(window.get_settings_visible());
    click_button(&window, "Done");

    assert!(!window.get_settings_visible());
    assert_eq!(*requests.borrow(), vec![true, false]);

    type_text(&window, "x");
    assert_eq!(
        window.get_note_text(),
        "x",
        "typing resumes in the note after Done"
    );
}

#[test]
fn the_window_relies_on_the_native_close_button() {
    let window = open_window();
    element(&window, "Note contents", AccessibleRole::TextInput);

    for saved in [false, true] {
        window.set_has_note(saved);
        window.set_saved(saved);
        for label in ["Close", "Close window"] {
            assert!(
                !offers_button(&window, label),
                "unexpected {label:?} button"
            );
        }
    }
}

fn record_shortcut_changes(window: &MainWindow) -> Rc<RefCell<Vec<String>>> {
    let (changes, record) = recorder();
    window.on_hotkey_changed(record);
    changes
}

fn replace_setting_text(window: &MainWindow, label: &str, text: &str) {
    if !window.get_settings_visible() {
        click_button(window, "Settings");
    }
    element(window, label, AccessibleRole::TextInput).mock_single_click(PointerEventButton::Left);
    press_ctrl(window, "a");
    type_text(window, text);
}

fn replace_shortcut_text(window: &MainWindow, text: &str) {
    replace_setting_text(window, "Capture shortcut", text);
}

#[test]
fn shortcut_is_applied_on_enter_not_on_every_keystroke() {
    let window = open_window();
    let changes = record_shortcut_changes(&window);

    replace_shortcut_text(&window, "Ctrl+Alt+N");
    assert!(
        changes.borrow().is_empty(),
        "half-typed shortcuts must not be registered"
    );
    assert_eq!(window.get_hotkey_text(), "Ctrl+Alt+N");

    key(&window, Key::Return.into());
    assert_eq!(*changes.borrow(), vec!["Ctrl+Alt+N".to_owned()]);
}

#[test]
fn done_applies_a_pending_shortcut() {
    let window = open_window();
    let changes = record_shortcut_changes(&window);

    replace_shortcut_text(&window, "Ctrl+Alt+N");
    click_button(&window, "Done");

    assert_eq!(*changes.borrow(), vec!["Ctrl+Alt+N".to_owned()]);
    assert!(!window.get_settings_visible());
}

#[test]
fn note_filename_is_previewed_while_typing_and_applied_on_enter() {
    let window = open_window();
    let (edits, record_edit) = recorder();
    window.on_note_filename_edited(record_edit);
    let (changes, record_change) = recorder();
    window.on_note_filename_changed(record_change);

    replace_setting_text(&window, "Note filename template", "{date}-{title}");
    assert_eq!(
        edits.borrow().last().map(String::as_str),
        Some("{date}-{title}"),
        "every edit refreshes the example"
    );
    assert!(
        changes.borrow().is_empty(),
        "half-typed templates must not be saved"
    );

    key(&window, Key::Return.into());
    assert_eq!(*changes.borrow(), vec!["{date}-{title}".to_owned()]);
    assert!(
        window.get_settings_visible(),
        "Enter applies the template without closing Settings"
    );
}

#[test]
fn done_applies_a_pending_note_filename() {
    let window = open_window();
    let (changes, record_change) = recorder();
    window.on_note_filename_changed(record_change);

    replace_setting_text(&window, "Note filename template", "{date}_{title}");
    click_button(&window, "Done");

    assert_eq!(*changes.borrow(), vec!["{date}_{title}".to_owned()]);
    assert!(!window.get_settings_visible());
}

#[test]
fn rejected_settings_keep_the_panel_open_and_explain_why() {
    let window = open_window();
    click_button(&window, "Settings");

    let shortcut_problem =
        "Add Win, Ctrl, Alt or Shift so the shortcut doesn't block normal typing.";
    window.set_hotkey_error(shortcut_problem.into());
    click_button(&window, "Done");
    settle();
    assert!(window.get_settings_visible());
    assert!(shows_text(&window, shortcut_problem));

    window.set_hotkey_error(SharedString::new());
    let example = "Example: 2026-11-04_1415_project-review.md";
    let filename_problem = "Unknown placeholder: {name}. Use {date}, {time}, or {title}.";
    window.set_note_filename_preview(example.into());
    window.set_note_filename_error(filename_problem.into());
    click_button(&window, "Done");
    settle();
    assert!(window.get_settings_visible());
    assert!(shows_text(&window, filename_problem));
    assert!(
        !shows_text(&window, example),
        "the problem replaces the example"
    );

    window.set_note_filename_error(SharedString::new());
    settle();
    assert!(shows_text(&window, example));
    click_button(&window, "Done");
    assert!(!window.get_settings_visible());
}

fn template_names(names: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        names
            .iter()
            .map(|name| SharedString::from(*name))
            .collect::<Vec<_>>(),
    ))
}

#[test]
fn meeting_templates_are_offered_for_the_note_only_once_enabled() {
    let window = open_window();
    assert!(find(&window, "Note template", AccessibleRole::Combobox).is_none());
    assert!(!offers_button(&window, "Insert template"));
    let toggles = Rc::new(RefCell::new(Vec::new()));
    let sink = toggles.clone();
    window.on_templates_enabled_changed(move |enabled| sink.borrow_mut().push(enabled));

    // Tall enough that the whole Settings panel is visible without scrolling.
    window.window().set_size(LogicalSize::new(760.0, 1400.0));
    click_button(&window, "Settings");
    element(
        &window,
        "Default meeting template",
        AccessibleRole::Combobox,
    );
    element(&window, "Template to edit", AccessibleRole::Combobox);
    let enable = element(
        &window,
        "Insert a template into new meeting notes",
        AccessibleRole::Checkbox,
    );
    assert_eq!(
        enable.accessible_checked(),
        Some(false),
        "templates are opt-in"
    );
    enable.invoke_accessible_default_action();
    assert_eq!(*toggles.borrow(), vec![true]);
    assert!(window.get_templates_enabled());
    click_button(&window, "Done");
    settle();
    window.window().set_size(LogicalSize::new(760.0, 640.0));

    let inserts = Rc::new(RefCell::new(Vec::new()));
    let sink = inserts.clone();
    window.on_note_template_insert_requested(move |index| sink.borrow_mut().push(index));
    window.set_template_names(template_names(&["General meeting", "Stand-up"]));
    window.set_note_template_index(1);
    settle();
    let picker = element(&window, "Note template", AccessibleRole::Combobox);
    assert_eq!(picker.accessible_value().as_deref(), Some("Stand-up"));
    assert_inside_window(&window, &picker);
    click_button(&window, "Insert template");
    assert_eq!(*inserts.borrow(), vec![1]);
    assert_eq!(
        window.get_note_text(),
        "",
        "Rust decides how the template is inserted"
    );
}

#[test]
fn built_in_templates_are_read_only_and_new_templates_can_be_saved() {
    let window = open_window();
    window.window().set_size(LogicalSize::new(760.0, 1400.0));
    click_button(&window, "Settings");
    window.set_template_name_text("General meeting".into());
    window.set_template_body_text("### Agenda".into());
    window.set_template_editable(false);
    settle();
    for label in ["Template name", "Template text"] {
        assert_eq!(
            element(&window, label, AccessibleRole::TextInput).accessible_read_only(),
            Some(true),
            "{label} of a built-in template"
        );
    }
    assert!(!offers_button(&window, "Save template"));
    assert!(!offers_button(&window, "Delete template"));

    let (news, saves) = (
        Rc::new(RefCell::new(Vec::new())),
        Rc::new(RefCell::new(Vec::new())),
    );
    let sink = news.clone();
    window.on_template_new_requested(move |copy| sink.borrow_mut().push(copy));
    let sink = saves.clone();
    window.on_template_save_requested(move |name, body| {
        sink.borrow_mut().push((name.to_string(), body.to_string()));
    });
    element(&window, "Duplicate template", AccessibleRole::Button)
        .invoke_accessible_default_action();
    element(&window, "New template", AccessibleRole::Button).invoke_accessible_default_action();
    assert_eq!(*news.borrow(), vec![true, false]);

    window.set_template_editable(true);
    window.set_template_is_new(true);
    window.set_template_name_text("Weekly sync".into());
    window.set_template_body_text("### Wins".into());
    settle();
    assert!(
        !offers_button(&window, "Delete template"),
        "a template that was never saved has nothing to delete"
    );
    element(&window, "Save template", AccessibleRole::Button).invoke_accessible_default_action();
    assert_eq!(
        *saves.borrow(),
        vec![("Weekly sync".to_owned(), "### Wins".to_owned())]
    );

    let problem =
        "Unknown placeholder: {owner}. Use {title}, {date}, {time}, {organizer}, or {attendees}.";
    window.set_template_message(problem.into());
    window.set_template_message_error(true);
    settle();
    assert!(shows_text(&window, problem));
    window.set_template_is_new(false);
    settle();
    assert!(offers_button(&window, "Delete template"));
}
