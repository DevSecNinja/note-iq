//! Headless acceptance checks for the capture window. They drive the real Slint component
//! through Slint's testing backend, using the accessibility labels screen readers rely on.
#![cfg(windows)]

#[allow(unsafe_code)]
mod generated {
    slint::include_modules!();
}

use generated::MainWindow;
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
fn starting_a_new_note_clears_text_typed_into_the_editor() {
    let window = open_window();
    let edits = record_edits(&window);
    type_text(&window, "Old");

    window.set_note_text(SharedString::new());
    type_text(&window, "New");

    assert_eq!(edits.borrow().last().map(String::as_str), Some("New"));
}

#[test]
fn save_state_moves_from_draft_to_unsaved_to_saved() {
    let window = open_window();
    assert!(shows_text(&window, "Draft"));

    window.set_has_note(true);
    window.set_saved(false);
    assert!(shows_text(&window, "Unsaved"));

    window.set_saved(true);
    assert!(shows_text(&window, "Saved"));
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
fn no_meeting_is_offered_only_while_connected() {
    let window = open_window();
    let disconnects = counter();
    window.on_disconnect_requested(increment(&disconnects));

    assert!(!offers_button(&window, "No meeting"));

    window.set_meeting_label("Design review".into());
    window.set_has_meeting(true);
    assert!(shows_text(&window, "Design review"));
    click_button(&window, "No meeting");
    assert_eq!(disconnects.get(), 1);
}

#[test]
fn meeting_picker_connects_the_selected_meeting() {
    let window = open_window();
    let connected = Rc::new(Cell::new(None));
    let sink = connected.clone();
    window.on_connect_requested(move |index| sink.set(Some(index)));
    window.set_meeting_options(ModelRc::new(VecModel::from(vec![
        SharedString::from("09:00 Standup"),
        SharedString::from("09:00 Review"),
    ])));
    window.set_meeting_index(1);

    assert!(
        !offers_button(&window, "Connect"),
        "Connect must be hidden until the picker opens"
    );

    window.set_meeting_picker_visible(true);
    click_button(&window, "Connect");
    assert_eq!(connected.get(), Some(1));
}

#[test]
fn failed_meeting_lookup_shows_the_reason_and_keeps_retry_available() {
    let window = open_window();
    let retries = counter();
    window.on_meeting_refresh_requested(increment(&retries));
    let message = "Meeting details are unavailable. Your note is still saved.";

    window.set_meeting_label(message.into());
    window.set_has_meeting(false);

    assert!(shows_text(&window, message));
    click_button(&window, "Find meeting");
    assert_eq!(retries.get(), 1);
}

#[test]
fn unwritable_folder_offers_a_folder_choice_in_the_footer() {
    let window = open_window();
    let choices = counter();
    window.on_destination_requested(increment(&choices));
    let footer_fix = |window: &MainWindow| {
        ElementHandle::find_by_element_id(window, "MainWindow::fix-folder").next()
    };
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
fn close_controls_and_editor_have_accessible_names() {
    let window = open_window();
    let closes = counter();
    window.on_close_requested(increment(&closes));

    element(&window, "Note contents", AccessibleRole::TextInput);
    click_button(&window, "Close window");
    click_button(&window, "Close");

    assert_eq!(closes.get(), 2);
}

fn record_shortcut_changes(window: &MainWindow) -> Rc<RefCell<Vec<String>>> {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let sink = changes.clone();
    window.on_hotkey_changed(move |text| sink.borrow_mut().push(text.into()));
    changes
}

fn replace_shortcut_text(window: &MainWindow, text: &str) {
    click_button(window, "Settings");
    element(window, "Capture shortcut", AccessibleRole::TextInput)
        .mock_single_click(PointerEventButton::Left);
    press_ctrl(window, "a");
    type_text(window, text);
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
