use crate::ui_generated::MainWindow;
use crate::{
    error_ui::meeting_failure,
    foreground,
    hotkey::{self, HotkeyController},
    instance::InstanceRole,
    meeting_ui::select_and_apply,
    open_note::open_note,
    settings_ui::choose_destination,
    tray::TrayController,
    workiq_windows::discover_executable,
};
use chrono::{DateTime, FixedOffset, Local};
use noteiq_core::{
    capture::CaptureController,
    destination::{Destination, DestinationError},
    diagnostics::{DiagnosticEvent, DiagnosticOperation, FailureClass},
    meeting::{browse_meetings, choice_label},
    model::{Capture, CaptureSource, Meeting, MeetingDecision},
    note_filename::NoteFilename,
    saved_note::{SavedNoteError, find_meeting_note, same_occurrence},
    settings::Settings,
    workiq::{WorkiqClient, WorkiqError},
};
use slint::winit_030::{WinitWindowAccessor, winit::platform::windows::WindowExtWindows};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

enum AppEvent {
    Meetings(u64, Result<Vec<Meeting>, WorkiqError>, Instant),
    Author(Option<String>),
    MeetingNote(NoteRequest, Result<Option<Box<Capture>>, SavedNoteError>),
}

#[derive(Clone)]
struct NoteRequest {
    generation: u64,
    capture_id: String,
    root: PathBuf,
    meeting: Meeting,
}

/// Calendar results are reused for this long; Refresh always looks again.
const MEETING_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
/// How often an open note re-checks the cached meetings against the clock, so a meeting that
/// starts while the note is open is picked up without another calendar lookup.
const REEVALUATION_INTERVAL: Duration = Duration::from_secs(60);
const CHECKING_CALENDAR: &str = "Checking your calendar…";
const NO_MEETING: &str = "No meeting";

struct AppState {
    capture: CaptureController,
    /// False once the capture has been closed and its window hidden; the next launch request
    /// then starts a fresh capture instead of reshowing the finished one.
    capture_open: bool,
    settings: Settings,
    settings_path: PathBuf,
    /// The filename template for notes saved from now on; saved notes keep their own.
    note_filename: NoteFilename,
    status: String,
    meeting_hint: String,
    /// The picker text on screen. Option 0 is always "No meeting".
    meeting_options: Vec<String>,
    meetings: Vec<Meeting>,
    /// The meetings behind picker options 1 and up, in the same order.
    picker_meetings: Vec<Meeting>,
    last_meeting_refresh: Option<Instant>,
    /// Set while the latest lookup failed; cached meetings stay selectable, but nothing is
    /// attached automatically until a lookup succeeds again.
    last_lookup_error: Option<WorkiqError>,
    last_reevaluation: Option<Instant>,
    lookup_pending: bool,
    lookup_id: u64,
    destination_error: bool,
    /// The Windows display name, found in the background at startup.
    author: Option<String>,
    note_lookup_generation: u64,
    pending_note: Option<NoteRequest>,
}

impl AppState {
    fn new(settings: Settings, settings_path: PathBuf, source: CaptureSource) -> Self {
        let now = Local::now().fixed_offset();
        let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "local".to_owned());
        let note_filename = settings.note_filename_template();
        let mut state = Self {
            capture: CaptureController::new(Capture::new(source, now, timezone)),
            capture_open: true,
            settings,
            settings_path,
            note_filename,
            status: "Ready".to_owned(),
            meeting_hint: CHECKING_CALENDAR.to_owned(),
            meeting_options: vec![NO_MEETING.to_owned()],
            meetings: Vec::new(),
            picker_meetings: Vec::new(),
            last_meeting_refresh: None,
            last_lookup_error: None,
            last_reevaluation: None,
            lookup_pending: false,
            lookup_id: 0,
            destination_error: false,
            author: None,
            note_lookup_generation: 0,
            pending_note: None,
        };
        state.capture.capture.author = state.effective_author();
        state
    }

    /// The `author_name` setting wins over the Windows display name.
    fn effective_author(&self) -> Option<String> {
        self.settings
            .author_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| self.author.clone())
    }

    fn save(&mut self) -> Result<Option<PathBuf>, DestinationError> {
        let destination = Destination::new(self.settings.destination_root.clone())
            .with_note_filename(self.note_filename.clone());
        self.capture.save(&destination, Local::now().fixed_offset())
    }

    fn save_settings(&self) -> Result<(), noteiq_core::settings::SettingsError> {
        self.settings.save(&self.settings_path)
    }
}

pub fn run(
    settings: Settings,
    settings_path: PathBuf,
    source: CaptureSource,
    role: InstanceRole,
    startup_message: Option<String>,
) -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_custom_application_handler(crate::keyboard::ComposedTextInput::default())
        .select()?;
    let window = MainWindow::new()?;
    let state = Rc::new(RefCell::new(AppState::new(settings, settings_path, source)));
    if let Some(message) = startup_message {
        state.borrow_mut().status = message;
    }

    let (sender, events) = mpsc::channel();
    let author_sender = sender.clone();
    std::thread::spawn(move || {
        let _ = author_sender.send(AppEvent::Author(crate::author::display_name()));
    });
    let ipc = match role {
        InstanceRole::Primary(receiver) => Some(receiver),
        InstanceRole::Forwarded => None,
    };
    let tray = match TrayController::new(&window.get_app_icon()) {
        Ok(tray) => Some(tray),
        Err(message) => {
            tracing::warn!(failure_class = "tray_unavailable");
            state.borrow_mut().status = message.to_owned();
            None
        }
    };
    let has_tray = tray.is_some();
    let mut hotkeys = match HotkeyController::new() {
        Ok(controller) => Some(controller),
        Err(message) => {
            tracing::warn!(failure_class = "hotkey_unavailable");
            state.borrow_mut().status = message;
            None
        }
    };
    if let Some(controller) = hotkeys.as_mut() {
        let result = {
            let state = state.borrow();
            controller.update(&state.settings.hotkey, state.settings.hotkey_enabled)
        };
        if let Err(message) = result {
            tracing::warn!(failure_class = "hotkey_unavailable");
            state.borrow_mut().status = message;
        }
    }
    if let Err(message) = crate::uri::register_protocol() {
        tracing::warn!(failure_class = "uri_registration_failed");
        state.borrow_mut().status = message.to_owned();
    }

    start_meeting_lookup(&state, &sender, false);
    configure_view(&window, &state);
    let _timer = bind_callbacks(&window, &state, sender, hotkeys, tray, ipc, events);

    window.show()?;
    let icon_window = window.as_weak();
    let icon_state = state.clone();
    slint::spawn_local(async move {
        let Some(window) = icon_window.upgrade() else {
            return;
        };
        let result = async {
            let pixels = window
                .get_app_icon()
                .to_rgba8()
                .ok_or("icon_decode_failed")?;
            let icon = slint::winit_030::winit::window::Icon::from_rgba(
                pixels.as_bytes().to_vec(),
                pixels.width(),
                pixels.height(),
            )
            .map_err(|_| "icon_dimensions_invalid")?;
            // Await native window creation rather than relying on startup timing.
            let native = window
                .window()
                .winit_window()
                .await
                .map_err(|_| "native_window_unavailable")?;
            native.set_taskbar_icon(Some(icon));
            Ok::<_, &'static str>(())
        }
        .await;
        if let Err(failure_class) = result {
            tracing::warn!(failure_class, "Taskbar icon setup failed");
            let message = "The taskbar icon could not be loaded. Notes still work normally.";
            icon_state.borrow_mut().status = message.to_owned();
            window.set_status_text(message.into());
        }
    })
    .map_err(|error| slint::PlatformError::Other(error.to_string()))?;
    foreground::bring_to_front(window.window());
    if has_tray {
        // Closing the capture only hides the window; the tray keeps NoteIQ ready for the next
        // shortcut, link or command until the user chooses Quit.
        slint::run_event_loop_until_quit()
    } else {
        slint::run_event_loop()
    }
}

fn configure_view(window: &MainWindow, state: &Rc<RefCell<AppState>>) {
    let mut state = state.borrow_mut();
    let now = Local::now().fixed_offset();
    window.set_note_text(state.capture.capture.body.clone().into());
    reset_settings_view(window, &state, now);
    update_meeting_view(window, &mut state, now);
}

fn bind_callbacks(
    window: &MainWindow,
    state: &Rc<RefCell<AppState>>,
    sender: Sender<AppEvent>,
    hotkeys: Option<HotkeyController>,
    tray: Option<TrayController>,
    ipc: Option<Receiver<CaptureSource>>,
    events: Receiver<AppEvent>,
) -> Timer {
    let hotkeys = Rc::new(RefCell::new(hotkeys));
    let ui_weak = window.as_weak();

    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_body_changed(move |text| {
            state
                .borrow_mut()
                .capture
                .edit(String::from(text), Instant::now());
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_saved(false);
                ui.set_status_text("Unsaved changes".into());
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_save_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                save_and_refresh(&ui, &state);
            }
        });
    }
    {
        // The native title-bar Close is the only way to close a capture.
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.window().on_close_requested(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return slint::CloseRequestResponse::KeepWindowShown;
            };
            if close_capture(&ui, &state) {
                slint::CloseRequestResponse::HideWindow
            } else {
                slint::CloseRequestResponse::KeepWindowShown
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_open_requested(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let path = state.borrow().capture.capture.path.clone();
            match path {
                Some(path) => match open_note(&path) {
                    Ok(()) => ui.set_status_text("Opened the saved note.".into()),
                    Err(message) => ui.set_status_text(message.into()),
                },
                None => ui.set_status_text("Save the note before opening it.".into()),
            }
        });
    }
    {
        let state = state.clone();
        let sender = sender.clone();
        let ui_weak = ui_weak.clone();
        window.on_meeting_refresh_requested(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            start_meeting_lookup_state(&mut state, &sender, true);
            update_meeting_view(&ui, &mut state, Local::now().fixed_offset());
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        let sender = sender.clone();
        window.on_meeting_chosen(move |index| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            request_meeting_note(
                &ui,
                &mut state.borrow_mut(),
                index,
                Local::now().fixed_offset(),
                &sender,
            );
        });
    }
    {
        let state = state.clone();
        let sender = sender.clone();
        let ui_weak = ui_weak.clone();
        window.on_auto_connect_changed(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            state.settings.auto_connect_meeting = enabled;
            if let Err(error) = state.save_settings() {
                tracing::warn!(failure_class = "settings_save_failed");
                state.status = format!("Could not save settings: {error}");
            }
            start_meeting_lookup_state(&mut state, &sender, false);
            apply_available_meetings_at(&ui, &mut state, Local::now().fixed_offset());
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_settings_requested(move |visible| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let state = state.borrow();
            if visible {
                // Opening Settings shows the saved values, not text a previous visit rejected.
                reset_settings_view(&ui, &state, Local::now().fixed_offset());
            }
            refresh_view(&ui, &state);
        });
    }
    {
        let ui_weak = ui_weak.clone();
        window.on_note_filename_edited(move |value| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            ui.set_note_filename_error(SharedString::new());
            ui.set_note_filename_preview(
                note_filename_preview(&value, Local::now().fixed_offset()).into(),
            );
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_note_filename_changed(move |value| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            apply_note_filename(
                &ui,
                &mut state.borrow_mut(),
                &value,
                Local::now().fixed_offset(),
            );
        });
    }
    {
        let state = state.clone();
        let hotkeys = hotkeys.clone();
        let ui_weak = ui_weak.clone();
        window.on_hotkey_changed(move |value| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            apply_hotkey(
                &ui,
                &mut state.borrow_mut(),
                hotkeys.borrow_mut().as_mut(),
                &value,
            );
        });
    }
    {
        let state = state.clone();
        let hotkeys = hotkeys.clone();
        let ui_weak = ui_weak.clone();
        window.on_hotkey_enabled_changed(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            state.settings.hotkey_enabled = enabled;
            persist_settings(&mut state);
            if let Some(controller) = hotkeys.borrow_mut().as_mut()
                && let Err(message) = controller.update(&state.settings.hotkey, enabled)
            {
                tracing::warn!(failure_class = "hotkey_unavailable");
                state.status = message;
            }
            refresh_view(&ui, &state);
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_destination_requested(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let current = state.borrow().settings.destination_root.clone();
            let Some(folder) = choose_destination(&current) else {
                ui.set_status_text("Destination unchanged.".into());
                return;
            };
            let mut state = state.borrow_mut();
            state.settings.destination_root = folder;
            state.destination_error = false;
            persist_settings(&mut state);
            if !state.capture.capture.is_empty() || state.capture.capture.path.is_some() {
                save_state(&mut state);
            }
            refresh_view(&ui, &state);
        });
    }
    {
        let state = state.clone();
        let sender = sender.clone();
        let ui_weak = ui_weak.clone();
        window.on_new_note_requested(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            start_new_capture(&ui, &state, &sender);
        });
    }

    if let Some(tray) = tray.as_ref() {
        let state = state.clone();
        let sender = sender.clone();
        let ui_weak = ui_weak.clone();
        window.on_tray_action_requested(move |action| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            let present = match action {
                0 => {
                    ensure_capture(&ui, &mut state, &sender, CaptureSource::Tray);
                    true
                }
                1 => {
                    start_new_capture_state(&ui, &mut state, &sender);
                    true
                }
                2 => {
                    if close_capture_state(&ui, &mut state) {
                        if let Err(error) = slint::quit_event_loop() {
                            tracing::error!(%error, "Could not exit NoteIQ");
                            state.status =
                                "NoteIQ could not exit. Please try Quit again.".to_owned();
                            refresh_view(&ui, &state);
                            true
                        } else {
                            false
                        }
                    } else {
                        true
                    }
                }
                _ => return,
            };
            drop(state);
            if present {
                if let Err(error) = ui.show() {
                    tracing::error!(%error, "Could not show NoteIQ after a tray action");
                }
                foreground::bring_to_front(ui.window());
            }
        });
        tray.bind(window.as_weak());
    }

    let timer = Timer::default();
    let ui_weak = ui_weak.clone();
    let state_for_timer = state.clone();
    let sender_for_timer = sender;
    timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let mut present = false;
        let mut state = state_for_timer.borrow_mut();
        for event in events.try_iter() {
            match event {
                AppEvent::Meetings(id, result, started) if id == state.lookup_id => {
                    process_meeting_result(&ui, &mut state, result, started);
                }
                AppEvent::Meetings(..) => {}
                AppEvent::Author(author) => apply_author(&ui, &mut state, author),
                AppEvent::MeetingNote(request, result) => {
                    apply_meeting_note(
                        &ui,
                        &mut state,
                        request,
                        result,
                        Local::now().fixed_offset(),
                    );
                }
            }
        }
        if let Some(ipc) = ipc.as_ref() {
            for source in ipc.try_iter() {
                ensure_capture(&ui, &mut state, &sender_for_timer, source);
                present = true;
            }
        }
        let _keep_tray_alive = &tray;
        if hotkeys
            .borrow()
            .as_ref()
            .is_some_and(|hotkeys| hotkeys.was_pressed())
        {
            ensure_capture(&ui, &mut state, &sender_for_timer, CaptureSource::Shortcut);
            present = true;
        }
        if state.capture_open
            && state
                .last_reevaluation
                .is_none_or(|at| at.elapsed() >= REEVALUATION_INTERVAL)
        {
            apply_available_meetings_at(&ui, &mut state, Local::now().fixed_offset());
        }
        if state.capture.autosave_due(Instant::now()) {
            save_state(&mut state);
            refresh_view(&ui, &state);
        }
        drop(state);
        if present {
            // Activation can synchronously re-enter Slint, so it runs after the state borrow ends.
            ui.show().ok();
            foreground::bring_to_front(ui.window());
        }
    });
    timer
}

fn reset_settings_view(window: &MainWindow, state: &AppState, now: DateTime<FixedOffset>) {
    window.set_hotkey_text(state.settings.hotkey.clone().into());
    window.set_hotkey_error(SharedString::new());
    window.set_note_filename_text(state.note_filename.template().into());
    window.set_note_filename_error(SharedString::new());
    window.set_note_filename_preview(example_text(&state.note_filename, now).into());
}

fn example_text(note_filename: &NoteFilename, now: DateTime<FixedOffset>) -> String {
    format!("Example: {}", note_filename.example(now))
}

/// The line under the filename box while typing: an example name, or what to fix.
fn note_filename_preview(value: &str, now: DateTime<FixedOffset>) -> String {
    match NoteFilename::parse(value) {
        Ok(note_filename) => example_text(&note_filename, now),
        Err(error) => error.to_string(),
    }
}

/// Saves a valid template for notes saved from now on. A rejected template stays in the box
/// with an explanation, and the saved setting is kept.
fn apply_note_filename(
    window: &MainWindow,
    state: &mut AppState,
    value: &str,
    now: DateTime<FixedOffset>,
) {
    match NoteFilename::parse(value) {
        Ok(note_filename) => {
            let template = note_filename.template();
            if template != state.settings.note_filename {
                state.settings.note_filename = template.clone();
                state.note_filename = note_filename.clone();
                if persist_settings(state) {
                    state.status = "Note filename saved. Applies to new notes.".to_owned();
                }
            }
            window.set_note_filename_text(template.into());
            window.set_note_filename_error(SharedString::new());
            window.set_note_filename_preview(example_text(&note_filename, now).into());
        }
        Err(error) => {
            tracing::warn!(
                failure_class = "note_filename_rejected",
                error_kind = error.kind()
            );
            window.set_note_filename_error(error.to_string().into());
        }
    }
    refresh_view(window, state);
}

/// Registers and saves a valid shortcut. A rejected one stays in the box with an explanation,
/// and the current shortcut keeps working.
fn apply_hotkey(
    window: &MainWindow,
    state: &mut AppState,
    controller: Option<&mut HotkeyController>,
    value: &str,
) {
    let value = value.trim().to_owned();
    if value == state.settings.hotkey {
        window.set_hotkey_error(SharedString::new());
        window.set_hotkey_text(value.into());
        return;
    }
    let enabled = state.settings.hotkey_enabled;
    let result = hotkey::validate(&value)
        .and_then(|()| controller.map_or(Ok(()), |controller| controller.update(&value, enabled)));
    match result {
        Ok(()) => {
            state.settings.hotkey = value.clone();
            if persist_settings(state) {
                state.status = if enabled {
                    format!("Shortcut set to {value}.")
                } else {
                    format!("Shortcut saved as {value}. Turn it on to use it.")
                };
            }
            window.set_hotkey_error(SharedString::new());
            window.set_hotkey_text(value.into());
        }
        Err(message) => {
            tracing::warn!(failure_class = "hotkey_rejected");
            window.set_hotkey_error(message.into());
        }
    }
    refresh_view(window, state);
}

fn persist_settings(state: &mut AppState) -> bool {
    if let Err(error) = state.save_settings() {
        tracing::warn!(failure_class = "settings_save_failed");
        state.status = format!("Could not save settings: {error}");
        false
    } else {
        state.status = "Settings saved.".to_owned();
        true
    }
}

/// Writes an author change into a note that already has a file at once. Otherwise the change
/// waits for the usual autosave, and an empty draft that was never saved stays unsaved.
fn persist_metadata_change(state: &mut AppState) {
    state.capture.metadata_changed(Instant::now());
    if state.capture.capture.path.is_some() {
        save_state(state);
    }
}

fn save_state(state: &mut AppState) {
    match state.save() {
        Ok(Some(path)) => {
            state.destination_error = false;
            state.status = format!(
                "Saved {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
        }
        Ok(None) => {
            state.destination_error = false;
            state.status = "Nothing to save yet.".to_owned();
        }
        Err(error) => {
            let class = error.failure_class();
            let event = DiagnosticEvent::new(DiagnosticOperation::NoteSave, class, 0);
            tracing::error!(
                correlation_id = %event.correlation_id,
                operation = ?event.operation,
                failure_class = ?event.failure_class,
                "Note save failed"
            );
            state.destination_error = matches!(
                class,
                FailureClass::DestinationUnavailable | FailureClass::PermissionDenied
            );
            state.status = if state.destination_error {
                "Could not save the note to this folder. Choose a different folder to retry."
                    .to_owned()
            } else {
                format!("Could not save the note: {error}")
            };
        }
    }
}

fn save_and_refresh(window: &MainWindow, state: &Rc<RefCell<AppState>>) {
    let mut state = state.borrow_mut();
    save_state(&mut state);
    refresh_view(window, &state);
}

fn close_capture(window: &MainWindow, state: &Rc<RefCell<AppState>>) -> bool {
    let mut state = state.borrow_mut();
    close_capture_state(window, &mut state)
}

fn close_capture_state(window: &MainWindow, state: &mut AppState) -> bool {
    if !state.capture_open {
        return true;
    }
    if !state.capture.capture.is_empty()
        || (state.capture.capture.path.is_some() && state.capture.is_dirty())
    {
        save_state(state);
        if state.capture.capture.path.is_none() || state.capture.is_dirty() {
            refresh_view(window, state);
            return false;
        }
    }
    if let Err(error) = state.capture.finish() {
        tracing::warn!(
            failure_class = "capture_finish_failed",
            "Could not finish capture"
        );
        state.status = format!("Could not finish the note: {error}");
        refresh_view(window, state);
        return false;
    }
    state.capture_open = false;
    state.pending_note = None;
    state.status = "Capture closed.".to_owned();
    refresh_view(window, state);
    true
}

fn start_new_capture(
    window: &MainWindow,
    state: &Rc<RefCell<AppState>>,
    sender: &Sender<AppEvent>,
) {
    let mut state = state.borrow_mut();
    start_new_capture_state(window, &mut state, sender);
}

fn start_new_capture_state(window: &MainWindow, state: &mut AppState, sender: &Sender<AppEvent>) {
    if close_capture_state(window, state) {
        begin_capture(window, state, sender, CaptureSource::NewNote);
    }
}

/// Keeps an open capture (and its text) as is, or starts a fresh one after the last was closed.
fn ensure_capture(
    window: &MainWindow,
    state: &mut AppState,
    sender: &Sender<AppEvent>,
    source: CaptureSource,
) {
    if !state.capture_open {
        begin_capture(window, state, sender, source);
    }
}

/// Starts a fresh note. Meeting choices made for the previous note do not carry over.
fn begin_capture(
    window: &MainWindow,
    state: &mut AppState,
    sender: &Sender<AppEvent>,
    source: CaptureSource,
) {
    let now = Local::now().fixed_offset();
    let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "local".to_owned());
    state.capture = CaptureController::new(Capture::new(source, now, timezone));
    state.pending_note = None;
    state.capture.capture.author = state.effective_author();
    state.capture_open = true;
    state.status = "Ready".to_owned();
    window.set_note_text(SharedString::new());
    window.set_settings_visible(false);
    start_meeting_lookup_state(state, sender, false);
    apply_available_meetings_at(window, state, now);
}

/// Stores the background author lookup and fills it into the open note when that note started
/// before the lookup ended, saving it again if it already has a file. Closed notes are left
/// alone: rewriting them in the background could overwrite edits made since in the vault.
fn apply_author(window: &MainWindow, state: &mut AppState, author: Option<String>) {
    tracing::info!(
        author_available = author.is_some(),
        "author lookup finished"
    );
    state.author = author;
    if !state.capture_open || state.capture.capture.author.is_some() {
        return;
    }
    state.capture.capture.author = state.effective_author();
    if state.capture.capture.author.is_some() {
        persist_metadata_change(state);
        refresh_view(window, state);
    }
}

fn start_meeting_lookup(state: &Rc<RefCell<AppState>>, sender: &Sender<AppEvent>, manual: bool) {
    start_meeting_lookup_state(&mut state.borrow_mut(), sender, manual);
}

/// Looks up meetings in the background unless a lookup is already running or, for automatic
/// lookups, the cache is still fresh. A failed lookup is retried at the next opportunity.
fn start_meeting_lookup_state(state: &mut AppState, sender: &Sender<AppEvent>, manual: bool) {
    let cache_fresh = state.last_lookup_error.is_none()
        && state
            .last_meeting_refresh
            .is_some_and(|at| at.elapsed() < MEETING_CACHE_TTL);
    if state.lookup_pending || (!manual && cache_fresh) {
        return;
    }
    state.lookup_pending = true;
    state.lookup_id += 1;
    let id = state.lookup_id;
    let executable = discover_executable(&state.settings);
    let sender = sender.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let result = match executable {
            Some(executable) => {
                WorkiqClient::new(executable).discover_meetings(Local::now().fixed_offset())
            }
            None => Err(WorkiqError::ExecutableMissing),
        };
        let _ = sender.send(AppEvent::Meetings(id, result, started));
    });
}

fn process_meeting_result(
    window: &MainWindow,
    state: &mut AppState,
    result: Result<Vec<Meeting>, WorkiqError>,
    started: Instant,
) {
    state.lookup_pending = false;
    match result {
        Ok(meetings) => {
            for meeting in meetings {
                if let Some(cached) = state
                    .meetings
                    .iter_mut()
                    .find(|cached| same_occurrence(cached, &meeting))
                {
                    *cached = meeting;
                } else {
                    state.meetings.push(meeting);
                }
            }
            state.last_meeting_refresh = Some(Instant::now());
            state.last_lookup_error = None;
        }
        Err(error) => {
            let (_, event) = meeting_failure(&error, started);
            tracing::warn!(
                correlation_id = %event.correlation_id,
                operation = ?event.operation,
                failure_class = ?event.failure_class,
                "Meeting lookup failed"
            );
            state.last_lookup_error = Some(error);
        }
    }
    apply_available_meetings_at(window, state, Local::now().fixed_offset());
}

/// Re-checks the cached meetings against `now`: a single meeting in progress is attached
/// automatically when the note's meeting rules allow it. Nothing is attached automatically
/// while the latest lookup failed.
fn apply_available_meetings_at(
    window: &MainWindow,
    state: &mut AppState,
    now: DateTime<FixedOffset>,
) {
    state.last_reevaluation = Some(Instant::now());
    if state.capture_open && state.last_lookup_error.is_none() && state.pending_note.is_none() {
        let before = state.capture.capture.meeting.clone();
        select_and_apply(
            &mut state.capture.capture,
            &state.meetings,
            now,
            state.settings.auto_connect_meeting,
        );
        if state.capture.capture.meeting != before {
            state.capture.metadata_changed(Instant::now());
        }
    }
    update_meeting_view(window, state, now);
}

/// Applies a choice the user committed in the picker. Option 0 is "No meeting".
fn request_meeting_note(
    window: &MainWindow,
    state: &mut AppState,
    index: i32,
    now: DateTime<FixedOffset>,
    sender: &Sender<AppEvent>,
) {
    state.note_lookup_generation = state.note_lookup_generation.wrapping_add(1);
    state.pending_note = None;
    let meeting = usize::try_from(index)
        .ok()
        .and_then(|position| position.checked_sub(1))
        .and_then(|position| state.picker_meetings.get(position))
        .cloned();
    let Some(meeting) = meeting else {
        choose_meeting(window, state, index, now);
        return;
    };
    if state.capture.capture.path.is_some()
        && state
            .capture
            .capture
            .meeting
            .as_ref()
            .is_some_and(|current| same_occurrence(current, &meeting))
    {
        choose_meeting(window, state, index, now);
        return;
    }
    let request = NoteRequest {
        generation: state.note_lookup_generation,
        capture_id: state.capture.capture.id.to_string(),
        root: state.settings.destination_root.clone(),
        meeting,
    };
    state.pending_note = Some(request.clone());
    state.status = "Opening meeting note...".to_owned();
    refresh_view(window, state);
    let sender = sender.clone();
    std::thread::spawn(move || {
        let result = find_meeting_note(&Destination::new(request.root.clone()), &request.meeting)
            .map(|capture| capture.map(Box::new));
        if sender.send(AppEvent::MeetingNote(request, result)).is_err() {
            tracing::warn!(
                failure_class = "meeting_note_delivery_failed",
                "Could not deliver meeting note lookup"
            );
        }
    });
}

fn apply_meeting_note(
    window: &MainWindow,
    state: &mut AppState,
    request: NoteRequest,
    result: Result<Option<Box<Capture>>, SavedNoteError>,
    now: DateTime<FixedOffset>,
) {
    if !state.capture_open
        || request.generation != state.note_lookup_generation
        || state
            .pending_note
            .as_ref()
            .is_none_or(|pending| pending.generation != request.generation)
    {
        return;
    }
    state.pending_note = None;
    if request.capture_id != state.capture.capture.id.to_string()
        || request.root != state.settings.destination_root
    {
        state.status = if state.capture.is_dirty() {
            "Unsaved changes"
        } else {
            "Ready"
        }
        .to_owned();
        update_meeting_view(window, state, now);
        return;
    }
    match result {
        Ok(Some(capture)) if capture.id == state.capture.capture.id => {
            state
                .capture
                .connect_meeting(request.meeting, Instant::now());
            state.status = if state.capture.is_dirty() {
                "Unsaved changes"
            } else {
                "Ready"
            }
            .to_owned();
        }
        Ok(Some(capture)) => {
            if close_capture_state(window, state) {
                state.capture = CaptureController::new(*capture);
                state.capture_open = true;
                state.destination_error = false;
                state.status = "Saved".to_owned();
                window.set_note_text(state.capture.capture.body.clone().into());
                window.invoke_focus();
            }
        }
        Ok(None) => {
            state
                .capture
                .connect_meeting(request.meeting, Instant::now());
            state.status = if state.capture.is_dirty() {
                "Unsaved changes"
            } else {
                "Ready"
            }
            .to_owned();
        }
        Err(error) => {
            tracing::warn!(
                failure_class = "meeting_note_open_failed",
                "Could not open meeting note"
            );
            state.status = format!("Could not open the meeting note: {error}");
        }
    }
    update_meeting_view(window, state, now);
}

fn choose_meeting(
    window: &MainWindow,
    state: &mut AppState,
    index: i32,
    now: DateTime<FixedOffset>,
) {
    match usize::try_from(index) {
        Ok(0) => {
            if state.capture.capture.meeting.is_some() {
                state.capture.disconnect_meeting(Instant::now());
            } else {
                // Nothing to remove, but the note must stop snapping to meetings.
                state.capture.capture.disconnect();
            }
        }
        Ok(position) => {
            if let Some(meeting) = state.picker_meetings.get(position - 1).cloned() {
                let already_attached = state
                    .capture
                    .capture
                    .meeting
                    .as_ref()
                    .is_some_and(|attached| same_occurrence(attached, &meeting));
                if already_attached {
                    // Confirms an automatic choice so it is never replaced.
                    state.capture.capture.meeting_decision = MeetingDecision::Selected;
                } else {
                    state.capture.connect_meeting(meeting, Instant::now());
                }
            }
        }
        Err(_) => {}
    }
    if state.capture.is_dirty() {
        state.status = "Unsaved changes".to_owned();
    } else if state.status == "Opening meeting note..." {
        state.status = "Ready".to_owned();
    }
    update_meeting_view(window, state, now);
}

/// Shows "No meeting" and the cached meetings, with the attached meeting selected. An attached
/// meeting stays listed even when a refresh no longer returns it.
fn update_meeting_view(window: &MainWindow, state: &mut AppState, now: DateTime<FixedOffset>) {
    let mut choices = browse_meetings(&state.meetings, now);
    let selected = match state.capture.capture.meeting.as_ref() {
        Some(attached) => match choices
            .iter()
            .position(|meeting| same_occurrence(meeting, attached))
        {
            Some(position) => {
                choices[position] = attached.clone();
                position + 1
            }
            None => {
                choices.insert(0, attached.clone());
                1
            }
        },
        None => 0,
    };
    state.picker_meetings = choices;
    let options = std::iter::once(NO_MEETING.to_owned())
        .chain(state.picker_meetings.iter().map(choice_label))
        .collect::<Vec<_>>();
    if options != state.meeting_options {
        // Replacing the model while a list is open would close it, so it only happens when the
        // options really changed. The model is set before the index that refers to it.
        let model = options
            .iter()
            .map(|option| SharedString::from(option.as_str()))
            .collect::<Vec<_>>();
        window.set_meeting_options(ModelRc::new(VecModel::from(model)));
        state.meeting_options = options;
    }
    window.set_meeting_index(i32::try_from(selected).unwrap_or(0));
    state.meeting_hint = meeting_hint(state);
    refresh_view(window, state);
}

fn meeting_hint(state: &AppState) -> String {
    if state.lookup_pending {
        return CHECKING_CALENDAR.to_owned();
    }
    if let Some(error) = state.last_lookup_error.as_ref() {
        return match error {
            WorkiqError::ExecutableMissing => {
                "Meeting lookup unavailable. WorkIQ comes with Scout.".to_owned()
            }
            WorkiqError::AuthenticationRequired => error.user_message().to_owned(),
            _ if !state.picker_meetings.is_empty() => {
                "Couldn't refresh meetings. Showing earlier results.".to_owned()
            }
            _ => error.user_message().to_owned(),
        };
    }
    if state.last_meeting_refresh.is_none() {
        return CHECKING_CALENDAR.to_owned();
    }
    String::new()
}

fn refresh_view(window: &MainWindow, state: &AppState) {
    window.set_status_text(state.status.clone().into());
    window.set_meeting_hint(state.meeting_hint.clone().into());
    window.set_meeting_picker_enabled(!state.picker_meetings.is_empty());
    window.set_meeting_refresh_enabled(!state.lookup_pending);
    window.set_destination_label(state.settings.destination_root.display().to_string().into());
    window.set_auto_connect(state.settings.auto_connect_meeting);
    window.set_hotkey_enabled(state.settings.hotkey_enabled);
    window.set_has_note(state.capture.capture.path.is_some());
    window.set_saved(!state.capture.is_dirty());
    window.set_destination_error(state.destination_error);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use noteiq_core::note_filename::DEFAULT_NOTE_FILENAME;
    use slint::Model;
    use std::{cell::Cell, path::Path};

    thread_local! {
        static BACKEND_READY: Cell<bool> = const { Cell::new(false) };
    }

    /// The testing backend can only be installed once per thread.
    fn init_backend() {
        BACKEND_READY.with(|ready| {
            if !ready.replace(true) {
                i_slint_backend_testing::init_no_event_loop();
            }
        });
    }

    /// Settings that point at a vault that does not exist, never the real one.
    fn test_settings() -> Settings {
        Settings::default_for(
            &std::env::temp_dir()
                .join("noteiq-ui-tests")
                .join("missing-profile"),
        )
    }

    fn fixed_now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-11-04T14:15:00+01:00").unwrap()
    }

    fn meeting_at(id: &str, now: DateTime<FixedOffset>, start: i64, end: i64) -> Meeting {
        Meeting {
            id: id.into(),
            title: Some(id.into()),
            start: Some(now + TimeDelta::minutes(start)),
            end: Some(now + TimeDelta::minutes(end)),
            organizer: None,
            invitees: Vec::new(),
            your_rsvp: None,
            entity_url: None,
            cancelled: false,
            declined: false,
            all_day: false,
        }
    }

    /// A note created at `now`, with `meetings` cached as if a lookup had just finished. Tests
    /// never start a real lookup: the cache stays fresh unless a test marks a lookup pending.
    fn window_with_settings(
        settings: Settings,
        meetings: Vec<Meeting>,
        now: DateTime<FixedOffset>,
    ) -> (MainWindow, AppState) {
        init_backend();
        let window = MainWindow::new().unwrap();
        let mut state = AppState::new(settings, PathBuf::new(), CaptureSource::Cli);
        state.capture.capture.created_at = now;
        state.meetings = meetings;
        state.last_meeting_refresh = Some(Instant::now());
        state.lookup_id = 4;
        apply_available_meetings_at(&window, &mut state, now);
        (window, state)
    }

    fn window_with(meetings: Vec<Meeting>, now: DateTime<FixedOffset>) -> (MainWindow, AppState) {
        window_with_settings(test_settings(), meetings, now)
    }

    /// An earlier meeting and one in progress, which is attached automatically.
    fn cached_window() -> (MainWindow, AppState, DateTime<FixedOffset>) {
        let now = Local::now().fixed_offset();
        let meetings = vec![
            meeting_at("older", now, -120, -90),
            meeting_at("current", now, -10, 20),
        ];
        let (window, state) = window_with(meetings, now);
        (window, state, now)
    }

    fn options(window: &MainWindow) -> Vec<String> {
        window
            .get_meeting_options()
            .iter()
            .map(|option| option.to_string())
            .collect()
    }

    fn attached_id(state: &AppState) -> Option<&str> {
        state
            .capture
            .capture
            .meeting
            .as_ref()
            .map(|m| m.id.as_str())
    }

    fn file_name(path: &Path) -> String {
        path.file_name().unwrap().to_string_lossy().into_owned()
    }

    fn switching_fixture() -> (tempfile::TempDir, MainWindow, AppState, Capture) {
        let folder = tempfile::tempdir().unwrap();
        let now = fixed_now();
        let selected = meeting_at("older", now, -120, -90);
        let mut saved = Capture::new(CaptureSource::Cli, now, "Europe/Amsterdam".into());
        saved.connect(selected.clone());
        saved.set_body("Previous meeting text");
        noteiq_core::destination::save_capture(
            &mut saved,
            &Destination::new(folder.path().into()),
            now,
        )
        .unwrap();
        let settings = Settings {
            destination_root: folder.path().into(),
            ..test_settings()
        };
        let (window, state) = window_with_settings(
            settings,
            vec![selected, meeting_at("current", now, -10, 20)],
            now,
        );
        (folder, window, state, saved)
    }

    fn lookup_note(window: &MainWindow, state: &mut AppState, selected: &Meeting) -> AppEvent {
        let index = state
            .picker_meetings
            .iter()
            .position(|m| same_occurrence(m, selected))
            .unwrap()
            + 1;
        let (sender, receiver) = mpsc::channel();
        request_meeting_note(window, state, index as i32, fixed_now(), &sender);
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("bounded saved-note lookup")
    }

    fn deliver_note(window: &MainWindow, state: &mut AppState, event: AppEvent) {
        let AppEvent::MeetingNote(request, result) = event else {
            panic!("expected saved-note response")
        };
        apply_meeting_note(window, state, request, result, fixed_now());
    }

    #[test]
    fn picker_callback_and_event_timer_install_the_background_lookup_result() {
        let (_folder, window, state, saved) = switching_fixture();
        let selected = state
            .picker_meetings
            .iter()
            .position(|m| same_occurrence(m, saved.meeting.as_ref().unwrap()))
            .unwrap()
            + 1;
        let state = Rc::new(RefCell::new(state));
        let (lookup_sender, lookup_receiver) = mpsc::channel();
        let (event_sender, events) = mpsc::channel();
        let _timer = bind_callbacks(&window, &state, lookup_sender, None, None, None, events);
        window.invoke_meeting_chosen(selected as i32);
        let event = lookup_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        event_sender
            .send(event)
            .unwrap_or_else(|_| panic!("event channel closed"));
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(201));
        assert_eq!(state.borrow().capture.capture.id, saved.id);
        assert_eq!(window.get_note_text(), "Previous meeting text\n");
    }

    #[test]
    fn selection_reopens_the_note_after_saving_the_latest_draft_in_its_original_context() {
        let (_folder, window, mut state, saved) = switching_fixture();
        let draft_id = state.capture.capture.id;
        state.capture.edit("Draft", Instant::now());
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        state
            .capture
            .edit("Latest typing during lookup", Instant::now());
        deliver_note(&window, &mut state, event);
        assert_eq!(state.capture.capture.id, saved.id);
        assert_eq!(state.capture.capture.path, saved.path);
        assert_eq!(window.get_note_text(), "Previous meeting text\n");
        assert!(!state.capture.is_dirty());
        assert!(state.capture_open);
        assert!(state.pending_note.is_none());
        let current = state.meetings.iter().find(|m| m.id == "current").unwrap();
        let draft = find_meeting_note(
            &Destination::new(state.settings.destination_root.clone()),
            current,
        )
        .unwrap()
        .unwrap();
        assert_eq!(draft.id, draft_id);
        assert_eq!(draft.body, "Latest typing during lookup\n");
        state
            .capture
            .edit("Continued previous note", Instant::now());
        save_state(&mut state);
        assert_eq!(state.capture.capture.path, saved.path);
        assert!(
            std::fs::read_to_string(saved.path.unwrap())
                .unwrap()
                .ends_with("Continued previous note\n")
        );
    }

    #[test]
    fn selecting_a_meeting_without_a_note_attaches_without_replacing_the_draft() {
        let (_folder, window, mut state, _) = switching_fixture();
        let selected = meeting_at("not-yet-saved", fixed_now(), -60, -30);
        state.meetings.push(selected.clone());
        update_meeting_view(&window, &mut state, fixed_now());
        state.capture.edit("Keep the current draft", Instant::now());
        let id = state.capture.capture.id;
        let event = lookup_note(&window, &mut state, &selected);
        deliver_note(&window, &mut state, event);
        assert_eq!(state.capture.capture.id, id);
        assert_eq!(state.capture.capture.body, "Keep the current draft");
        assert_eq!(state.capture.capture.meeting, Some(selected));
        assert!(state.capture.capture.path.is_none());
    }

    #[test]
    fn failed_save_blocks_switching_without_losing_text_or_context() {
        let (_folder, window, mut state, saved) = switching_fixture();
        state.capture.edit("Persisted draft", Instant::now());
        save_state(&mut state);
        let path = state.capture.capture.path.clone().unwrap();
        let id = state.capture.capture.id;
        state.capture.edit("Latest unsaved draft", Instant::now());
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        std::fs::write(&path, "Externally replaced file").unwrap();
        deliver_note(&window, &mut state, event);
        assert_eq!(state.capture.capture.id, id);
        assert_eq!(state.capture.capture.body, "Latest unsaved draft");
        assert_eq!(attached_id(&state), Some("current"));
        assert!(state.capture.is_dirty());
        assert!(state.capture_open);
        assert!(state.status.contains("Could not save"));
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "Externally replaced file"
        );
    }

    #[test]
    fn no_meeting_cancels_an_in_flight_reopen_and_stays_detached() {
        let (_folder, window, mut state, saved) = switching_fixture();
        let id = state.capture.capture.id;
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        let (sender, _) = mpsc::channel();
        request_meeting_note(&window, &mut state, 0, fixed_now(), &sender);
        deliver_note(&window, &mut state, event);
        assert_eq!(state.capture.capture.id, id);
        assert_eq!(state.capture.capture.meeting, None);
        assert_eq!(
            state.capture.capture.meeting_decision,
            MeetingDecision::Disconnected
        );
        assert!(state.pending_note.is_none());
    }

    #[test]
    fn no_meeting_on_a_reopened_note_removes_its_persisted_context() {
        let (_folder, window, mut state, saved) = switching_fixture();
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        deliver_note(&window, &mut state, event);
        let (sender, _) = mpsc::channel();
        request_meeting_note(&window, &mut state, 0, fixed_now(), &sender);
        save_state(&mut state);
        assert!(!state.capture.is_dirty());
        assert_eq!(state.capture.capture.path, saved.path);
        let text = std::fs::read_to_string(saved.path.unwrap()).unwrap();
        assert!(!text.contains("calendar_event_id:"));
        assert!(!text.contains("## Meeting details"));
        assert!(text.ends_with("Previous meeting text\n"));
    }

    #[test]
    fn a_destination_change_rejects_the_response_and_clears_pending_selection() {
        let (_folder, window, mut state, saved) = switching_fixture();
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        let id = state.capture.capture.id;
        let next = tempfile::tempdir().unwrap();
        state.settings.destination_root = next.path().into();
        deliver_note(&window, &mut state, event);
        assert_eq!(state.capture.capture.id, id);
        assert_eq!(attached_id(&state), Some("current"));
        assert!(state.pending_note.is_none());
        assert_ne!(state.status, "Opening meeting note...");
    }

    #[test]
    fn obsolete_responses_cannot_replace_a_new_capture_or_newer_selection() {
        let (_folder, window, mut state, saved) = switching_fixture();
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        let latest = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        deliver_note(&window, &mut state, event);
        assert!(state.pending_note.is_some());
        deliver_note(&window, &mut state, latest);
        assert_eq!(state.capture.capture.id, saved.id);
        let selected = meeting_at("current", fixed_now(), -10, 20);
        let event = lookup_note(&window, &mut state, &selected);
        let (sender, _) = mpsc::channel();
        start_new_capture_state(&window, &mut state, &sender);
        let id = state.capture.capture.id;
        deliver_note(&window, &mut state, event);
        assert_eq!(state.capture.capture.id, id);
        assert_ne!(id, saved.id);
        assert!(state.capture.capture.body.is_empty());
        assert!(state.pending_note.is_none());
    }

    #[test]
    fn choosing_the_active_saved_occurrence_does_not_reload_over_unsaved_text() {
        let (_folder, window, mut state, saved) = switching_fixture();
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        deliver_note(&window, &mut state, event);
        state.capture.edit("Not saved yet", Instant::now());
        let index = state
            .picker_meetings
            .iter()
            .position(|m| same_occurrence(m, saved.meeting.as_ref().unwrap()))
            .unwrap()
            + 1;
        let (sender, receiver) = mpsc::channel();
        request_meeting_note(&window, &mut state, index as i32, fixed_now(), &sender);
        assert!(state.pending_note.is_none());
        assert!(receiver.try_recv().is_err());
        assert_eq!(state.capture.capture.body, "Not saved yet");
        assert!(state.capture.is_dirty());
    }

    #[test]
    fn reattaching_a_detached_unsaved_note_never_reloads_its_older_disk_text() {
        let (_folder, window, mut state, saved) = switching_fixture();
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        deliver_note(&window, &mut state, event);
        choose_meeting(&window, &mut state, 0, fixed_now());
        state.capture.edit("Unsaved continuation", Instant::now());
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        deliver_note(&window, &mut state, event);
        assert_eq!(state.capture.capture.id, saved.id);
        assert_eq!(state.capture.capture.body, "Unsaved continuation");
        assert_eq!(state.capture.capture.meeting, saved.meeting);
    }

    #[test]
    fn malformed_saved_note_reports_an_error_and_retains_the_current_draft() {
        let (_folder, window, mut state, saved) = switching_fixture();
        let path = saved.path.unwrap();
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("noteiq_schema: 4", "noteiq_schema: 3");
        std::fs::write(path, text).unwrap();
        state.capture.edit("Keep this draft", Instant::now());
        let id = state.capture.capture.id;
        let event = lookup_note(&window, &mut state, saved.meeting.as_ref().unwrap());
        deliver_note(&window, &mut state, event);
        assert_eq!(state.capture.capture.id, id);
        assert_eq!(state.capture.capture.body, "Keep this draft");
        assert!(state.capture.is_dirty());
        assert!(state.status.contains("Could not open the meeting note"));
        assert!(state.pending_note.is_none());
    }

    #[test]
    fn picker_keeps_shared_series_ids_as_distinct_occurrences() {
        let now = fixed_now();
        let earlier = meeting_at("series", now, -120, -90);
        let current = meeting_at("series", now, -10, 20);
        let (window, mut state) = window_with(vec![earlier.clone(), current.clone()], now);
        assert_eq!(state.picker_meetings.len(), 2);
        assert!(same_occurrence(
            &state.picker_meetings[window.get_meeting_index() as usize - 1],
            &current
        ));
        let index = state
            .picker_meetings
            .iter()
            .position(|m| same_occurrence(m, &earlier))
            .unwrap()
            + 1;
        choose_meeting(&window, &mut state, index as i32, now);
        assert_eq!(state.capture.capture.meeting, Some(earlier));
    }

    #[test]
    fn quit_finishes_and_saves_the_latest_capture_text() {
        let (window, mut state, _) = cached_window();
        let folder = tempfile::tempdir().unwrap();
        state.settings.destination_root = folder.path().to_path_buf();
        state.capture.edit("Saved before quitting", Instant::now());
        save_state(&mut state);
        state
            .capture
            .edit("Latest text before quitting", Instant::now());

        assert!(close_capture_state(&window, &mut state));
        assert!(!state.capture_open);
        assert!(!state.capture.is_dirty());
        let path = state.capture.capture.path.as_ref().unwrap();
        let saved = std::fs::read_to_string(path).unwrap();
        assert!(saved.contains("Latest text before quitting"));
        assert!(!saved.contains("Saved before quitting"));
        assert!(close_capture_state(&window, &mut state));
    }

    #[test]
    fn quit_keeps_the_capture_open_and_reports_a_failed_save() {
        let (window, mut state, _) = cached_window();
        state.settings.destination_root =
            std::env::temp_dir().join(format!("noteiq-missing-{}", state.capture.capture.id));
        assert!(!state.settings.destination_root.exists());
        state
            .capture
            .edit("Keep this unsaved draft", Instant::now());

        assert!(!close_capture_state(&window, &mut state));
        assert!(state.capture_open);
        assert!(state.capture.is_dirty());
        assert_eq!(state.capture.capture.body, "Keep this unsaved draft");
        assert!(state.capture.capture.path.is_none());
        assert_ne!(window.get_status_text(), "Ready");
        assert!(!window.get_status_text().is_empty());
    }

    #[test]
    fn new_note_reuses_the_cache_without_starting_another_lookup() {
        let (window, mut state, _) = cached_window();
        let old_id = state.capture.capture.id;
        let (sender, receiver) = mpsc::channel();
        start_new_capture_state(&window, &mut state, &sender);
        assert_ne!(state.capture.capture.id, old_id);
        assert_eq!(state.meetings.len(), 2);
        assert_eq!(state.picker_meetings.len(), 2);
        assert_eq!(attached_id(&state), Some("current"));
        assert_eq!(window.get_meeting_index(), 1);
        assert!(
            window.get_meeting_picker_enabled(),
            "older meetings remain selectable while connected"
        );
        assert_eq!(state.lookup_id, 4);
        assert!(!state.lookup_pending);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn new_note_reuses_an_in_flight_lookup() {
        let (window, mut state, _) = cached_window();
        state.last_meeting_refresh = None;
        state.lookup_pending = true;
        let (sender, _receiver) = mpsc::channel();
        start_new_capture_state(&window, &mut state, &sender);
        assert_eq!(state.lookup_id, 4);
        assert!(state.lookup_pending);
        assert_eq!(window.get_meeting_hint(), CHECKING_CALENDAR);
        assert!(!window.get_meeting_refresh_enabled());
    }

    #[test]
    fn failed_refresh_keeps_the_cached_picker_and_the_save_status() {
        let (window, mut state, _) = cached_window();
        state.lookup_pending = true;
        process_meeting_result(
            &window,
            &mut state,
            Err(WorkiqError::TimedOut),
            Instant::now(),
        );
        assert!(!state.lookup_pending);
        assert_eq!(state.picker_meetings.len(), 2);
        assert_eq!(window.get_meeting_index(), 1);
        assert!(window.get_meeting_picker_enabled());
        assert!(window.get_meeting_refresh_enabled());
        assert_eq!(
            window.get_meeting_hint(),
            "Couldn't refresh meetings. Showing earlier results."
        );
        assert_eq!(state.status, "Ready");
    }

    #[test]
    fn partial_calendar_results_do_not_discard_older_cached_meetings() {
        let (window, mut state, _) = cached_window();
        let current = state.meetings[1].clone();
        state.lookup_pending = true;
        process_meeting_result(&window, &mut state, Ok(vec![current]), Instant::now());
        assert_eq!(state.meetings.len(), 2);
        assert_eq!(state.picker_meetings.len(), 2);
    }

    #[test]
    fn new_note_keeps_the_existing_draft_when_saving_fails() {
        let (window, mut state, _) = cached_window();
        let old_id = state.capture.capture.id;
        state.settings.destination_root =
            std::env::temp_dir().join(format!("noteiq-missing-{old_id}"));
        assert!(!state.settings.destination_root.exists());
        state.capture.edit("Keep this draft", Instant::now());
        let (sender, _receiver) = mpsc::channel();
        start_new_capture_state(&window, &mut state, &sender);
        assert_eq!(state.capture.capture.id, old_id);
        assert_eq!(state.capture.capture.body, "Keep this draft");
        assert!(state.capture_open);
        assert_eq!(state.meetings.len(), 2);
        assert_eq!(state.lookup_id, 4);
    }

    #[test]
    fn new_note_keeps_a_clean_capture_when_its_retry_save_conflicts() {
        let (window, mut state, _) = cached_window();
        let folder = tempfile::tempdir().unwrap();
        state.settings.destination_root = folder.path().to_path_buf();
        state
            .capture
            .edit("Preserve this saved text", Instant::now());
        save_state(&mut state);
        let path = state.capture.capture.path.clone().expect("note saved");
        let original_id = state.capture.capture.id;
        let replacement = std::fs::read_to_string(&path)
            .unwrap()
            .replace(&original_id.to_string(), "some-other-note");
        std::fs::write(&path, replacement).unwrap();
        let (sender, _receiver) = mpsc::channel();

        start_new_capture_state(&window, &mut state, &sender);

        assert_eq!(state.capture.capture.id, original_id);
        assert_eq!(state.capture.capture.body, "Preserve this saved text");
        assert!(state.capture_open);
        assert!(state.capture.is_dirty());
        assert_eq!(state.capture.capture.path.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn late_author_lookup_fills_the_open_note_and_the_setting_wins() {
        let (window, mut state, _) = cached_window();
        assert_eq!(state.capture.capture.author, None);
        apply_author(&window, &mut state, Some("Windows Name".to_owned()));
        assert_eq!(
            state.capture.capture.author.as_deref(),
            Some("Windows Name")
        );
        assert_eq!(
            state.capture.capture.path, None,
            "an unsaved note stays unsaved"
        );

        let (sender, _receiver) = mpsc::channel();
        state.settings.author_name = Some("  Preferred Name ".to_owned());
        start_new_capture_state(&window, &mut state, &sender);
        assert_eq!(
            state.capture.capture.author.as_deref(),
            Some("Preferred Name")
        );

        state.settings.author_name = Some("   ".to_owned());
        start_new_capture_state(&window, &mut state, &sender);
        assert_eq!(
            state.capture.capture.author.as_deref(),
            Some("Windows Name")
        );
    }

    #[test]
    fn late_author_lookup_rewrites_the_saved_open_note() {
        let (window, mut state, _) = cached_window();
        let folder = tempfile::tempdir().unwrap();
        state.settings.destination_root = folder.path().to_path_buf();
        state
            .capture
            .edit("Saved before the author lookup ended", Instant::now());
        save_state(&mut state);
        let path = state.capture.capture.path.clone().expect("note saved");
        assert!(!std::fs::read_to_string(&path).unwrap().contains("from:"));

        apply_author(&window, &mut state, Some("Windows Name".to_owned()));

        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("\nfrom: \"[[Windows Name]]\"\n"));
        assert!(saved.contains("**From:** [[Windows Name]]"));
        assert!(!state.capture.is_dirty());
        assert!(state.status.starts_with("Saved "));
    }

    #[test]
    fn late_author_lookup_leaves_a_closed_note_untouched() {
        let (window, mut state, _) = cached_window();
        let folder = tempfile::tempdir().unwrap();
        state.settings.destination_root = folder.path().to_path_buf();
        state
            .capture
            .edit("Closed before the author lookup ended", Instant::now());
        assert!(close_capture_state(&window, &mut state));
        let path = state.capture.capture.path.clone().expect("note saved");
        let closed = std::fs::read_to_string(&path).unwrap();

        apply_author(&window, &mut state, Some("Windows Name".to_owned()));

        assert_eq!(std::fs::read_to_string(&path).unwrap(), closed);
        assert_eq!(state.capture.capture.author, None);
        assert_eq!(state.author.as_deref(), Some("Windows Name"));
    }

    #[test]
    fn picker_lists_no_meeting_first_and_meetings_without_numbers() {
        let (window, state, _) = cached_window();
        let expected = vec![
            NO_MEETING.to_owned(),
            choice_label(&state.meetings[1]),
            choice_label(&state.meetings[0]),
        ];
        assert_eq!(options(&window), expected);
        assert_eq!(state.meeting_options, expected);
        assert!(expected[1].starts_with("current · "), "{}", expected[1]);
        assert!(expected[2].starts_with("older · "), "{}", expected[2]);
        assert_eq!(window.get_meeting_index(), 1);
        assert_eq!(window.get_meeting_hint(), "");
    }

    #[test]
    fn choosing_no_meeting_updates_the_saved_file() {
        let (window, mut state, now) = cached_window();
        let folder = tempfile::tempdir().unwrap();
        state.settings.destination_root = folder.path().to_path_buf();
        state.capture.edit("Discussed the rollout", Instant::now());
        save_state(&mut state);
        let connected = state.capture.capture.path.clone().expect("note saved");
        assert!(file_name(&connected).ends_with("_current.md"));
        assert!(
            std::fs::read_to_string(&connected)
                .unwrap()
                .contains("\nmeeting:")
        );

        choose_meeting(&window, &mut state, 0, now);
        assert_eq!(attached_id(&state), None);
        assert_eq!(
            state.capture.capture.meeting_decision,
            MeetingDecision::Disconnected
        );
        assert!(state.capture.is_dirty());
        assert!(
            state
                .capture
                .autosave_due(Instant::now() + Duration::from_secs(1)),
            "the change is saved after the usual pause"
        );
        assert_eq!(window.get_meeting_index(), 0);
        assert_eq!(window.get_meeting_hint(), "");

        save_state(&mut state);
        let disconnected = state.capture.capture.path.clone().expect("note saved");
        assert!(file_name(&disconnected).ends_with("_discussed-the-rollout.md"));
        assert!(
            !connected.exists(),
            "the renamed note leaves no copy behind"
        );
        let saved = std::fs::read_to_string(&disconnected).unwrap();
        assert!(!saved.contains("\nmeeting:"));
        assert!(saved.contains("Discussed the rollout"));
        assert!(!state.capture.is_dirty());

        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(1));
        assert_eq!(attached_id(&state), None, "No meeting is never overridden");
        assert_eq!(window.get_meeting_index(), 0);
    }

    #[test]
    fn a_chosen_meeting_survives_refreshes_and_reevaluation() {
        let (window, mut state, now) = cached_window();
        choose_meeting(&window, &mut state, 2, now);
        assert_eq!(attached_id(&state), Some("older"));
        assert_eq!(
            state.capture.capture.meeting_decision,
            MeetingDecision::Selected
        );
        assert_eq!(window.get_meeting_index(), 2);
        assert_eq!(window.get_meeting_hint(), "");

        let current = state.meetings[1].clone();
        state.lookup_pending = true;
        process_meeting_result(&window, &mut state, Ok(vec![current]), Instant::now());
        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(1));
        assert_eq!(attached_id(&state), Some("older"));
        assert_eq!(window.get_meeting_index(), 2);
        assert_eq!(window.get_meeting_hint(), "");
    }

    #[test]
    fn choosing_the_automatic_meeting_locks_it_without_saving() {
        let (window, mut state, now) = cached_window();
        assert_eq!(
            state.capture.capture.meeting_decision,
            MeetingDecision::Automatic
        );
        choose_meeting(&window, &mut state, 1, now);
        assert_eq!(attached_id(&state), Some("current"));
        assert_eq!(
            state.capture.capture.meeting_decision,
            MeetingDecision::Selected
        );
        assert!(!state.capture.is_dirty());
        assert_eq!(window.get_meeting_hint(), "");
    }

    #[test]
    fn an_empty_note_snaps_to_a_meeting_once_it_starts() {
        let now = Local::now().fixed_offset();
        let (window, mut state) = window_with(vec![meeting_at("standup", now, 5, 35)], now);
        assert_eq!(attached_id(&state), None);
        assert_eq!(window.get_meeting_index(), 0);
        assert_eq!(window.get_meeting_hint(), "");
        assert!(
            window.get_meeting_picker_enabled(),
            "an upcoming meeting can be chosen early"
        );

        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(6));
        assert_eq!(attached_id(&state), Some("standup"));
        assert_eq!(window.get_meeting_index(), 1);
        assert_eq!(window.get_meeting_hint(), "");
        assert!(!state.capture.is_dirty(), "an empty draft is not saved");
    }

    #[test]
    fn a_note_with_text_snaps_only_to_a_meeting_that_starts_soon_after_it() {
        let now = Local::now().fixed_offset();
        let (window, mut state) = window_with(vec![meeting_at("review", now, 10, 40)], now);
        state.capture.edit("Prep for the review", Instant::now());
        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(11));
        assert_eq!(attached_id(&state), Some("review"));

        let (window, mut state) = window_with(vec![meeting_at("later", now, 20, 50)], now);
        state.capture.edit("Unrelated thoughts", Instant::now());
        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(21));
        assert_eq!(attached_id(&state), None);
        assert_eq!(window.get_meeting_index(), 0);
        assert_eq!(window.get_meeting_hint(), "");
    }

    #[test]
    fn nothing_is_attached_automatically_while_the_last_lookup_failed() {
        let now = Local::now().fixed_offset();
        let (window, mut state) = window_with(vec![meeting_at("standup", now, 5, 35)], now);
        state.lookup_pending = true;
        process_meeting_result(
            &window,
            &mut state,
            Err(WorkiqError::LookupFailed),
            Instant::now(),
        );
        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(6));
        assert_eq!(attached_id(&state), None);
        assert_eq!(
            window.get_meeting_hint(),
            "Couldn't refresh meetings. Showing earlier results."
        );
        assert!(window.get_meeting_picker_enabled());

        state.lookup_pending = true;
        process_meeting_result(&window, &mut state, Ok(Vec::new()), Instant::now());
        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(6));
        assert_eq!(attached_id(&state), Some("standup"));
    }

    #[test]
    fn a_cancelled_update_keeps_the_attached_meeting_listed() {
        let (window, mut state, _) = cached_window();
        let mut cancelled = state.meetings[1].clone();
        cancelled.cancelled = true;
        state.lookup_pending = true;
        process_meeting_result(&window, &mut state, Ok(vec![cancelled]), Instant::now());
        assert_eq!(attached_id(&state), Some("current"));
        assert_eq!(window.get_meeting_index(), 1);
        assert_eq!(options(&window).len(), 3);
    }

    #[test]
    fn the_hint_explains_each_lookup_state() {
        init_backend();
        let window = MainWindow::new().unwrap();
        let mut state = AppState::new(test_settings(), PathBuf::new(), CaptureSource::Cli);
        let now = Local::now().fixed_offset();
        state.capture.capture.created_at = now;
        update_meeting_view(&window, &mut state, now);
        assert_eq!(window.get_meeting_hint(), CHECKING_CALENDAR);
        assert!(!window.get_meeting_picker_enabled());
        assert_eq!(options(&window), [NO_MEETING]);

        state.lookup_pending = true;
        process_meeting_result(
            &window,
            &mut state,
            Err(WorkiqError::ExecutableMissing),
            Instant::now(),
        );
        assert_eq!(
            window.get_meeting_hint(),
            "Meeting lookup unavailable. WorkIQ comes with Scout."
        );
        assert!(window.get_meeting_refresh_enabled());

        state.lookup_pending = true;
        process_meeting_result(
            &window,
            &mut state,
            Err(WorkiqError::AuthenticationRequired),
            Instant::now(),
        );
        assert_eq!(
            window.get_meeting_hint(),
            WorkiqError::AuthenticationRequired.user_message()
        );

        state.lookup_pending = true;
        process_meeting_result(&window, &mut state, Ok(Vec::new()), Instant::now());
        assert_eq!(window.get_meeting_hint(), "");
        assert!(!window.get_meeting_picker_enabled());

        state.meetings = vec![
            meeting_at("first", now, -10, 20),
            meeting_at("second", now, -5, 25),
        ];
        apply_available_meetings_at(&window, &mut state, now);
        assert_eq!(attached_id(&state), None);
        assert_eq!(window.get_meeting_index(), 0);
        assert_eq!(window.get_meeting_hint(), "");

        state.settings.auto_connect_meeting = false;
        state.meetings.truncate(1);
        apply_available_meetings_at(&window, &mut state, now);
        assert_eq!(attached_id(&state), None);
        assert_eq!(window.get_meeting_hint(), "");
    }

    #[test]
    fn an_ended_meeting_does_not_add_a_redundant_caption() {
        let (window, mut state, now) = cached_window();
        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(25));
        assert_eq!(attached_id(&state), Some("current"));
        assert_eq!(window.get_meeting_index(), 1);
        assert_eq!(window.get_meeting_hint(), "");
    }

    #[test]
    fn only_an_empty_unsaved_note_moves_on_to_the_next_meeting() {
        let now = Local::now().fixed_offset();
        let meetings = vec![
            meeting_at("first", now, -20, 10),
            meeting_at("second", now, 10, 40),
        ];
        let (window, mut state) = window_with(meetings.clone(), now);
        assert_eq!(attached_id(&state), Some("first"));
        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(15));
        assert_eq!(attached_id(&state), Some("second"));
        assert_eq!(window.get_meeting_index(), 1);
        assert!(!state.capture.is_dirty());

        let (window, mut state) = window_with(meetings, now);
        state
            .capture
            .edit("Notes from the first meeting", Instant::now());
        apply_available_meetings_at(&window, &mut state, now + TimeDelta::minutes(15));
        assert_eq!(attached_id(&state), Some("first"));
        assert_eq!(window.get_meeting_index(), 2);
        assert_eq!(window.get_meeting_hint(), "");
    }

    #[test]
    fn a_valid_note_filename_is_saved_and_used_for_the_next_save() {
        let (window, mut state, _) = cached_window();
        let folder = tempfile::tempdir().unwrap();
        state.settings_path = folder.path().join("settings.toml");
        apply_note_filename(&window, &mut state, " {DATE}-{Title}.md ", fixed_now());
        assert_eq!(window.get_note_filename_text(), "{date}-{title}");
        assert_eq!(window.get_note_filename_error(), "");
        assert_eq!(
            window.get_note_filename_preview(),
            "Example: 2026-11-04-project-review.md"
        );
        assert_eq!(state.status, "Note filename saved. Applies to new notes.");
        let saved = Settings::load(&state.settings_path, folder.path()).unwrap();
        assert_eq!(saved.note_filename, "{date}-{title}");

        state.settings.destination_root = folder.path().to_path_buf();
        state
            .capture
            .edit("Filed under the new name", Instant::now());
        save_state(&mut state);
        let name = file_name(state.capture.capture.path.as_ref().expect("note saved"));
        assert!(name.ends_with("-current.md"), "{name}");
        assert!(!name.contains('_'), "{name}");
    }

    #[test]
    fn an_invalid_note_filename_keeps_the_typed_text_and_the_saved_setting() {
        let (window, mut state, _) = cached_window();
        window.set_note_filename_text("{name}".into());
        apply_note_filename(&window, &mut state, "{name}", fixed_now());
        assert_eq!(
            window.get_note_filename_error(),
            "Unknown placeholder: {name}. Use {date}, {time}, or {title}."
        );
        assert_eq!(window.get_note_filename_text(), "{name}");
        assert_eq!(state.settings.note_filename, DEFAULT_NOTE_FILENAME);
        assert_eq!(state.note_filename, NoteFilename::default());

        reset_settings_view(&window, &state, fixed_now());
        assert_eq!(window.get_note_filename_text(), DEFAULT_NOTE_FILENAME);
        assert_eq!(window.get_note_filename_error(), "");
        assert_eq!(
            window.get_note_filename_preview(),
            "Example: 2026-11-04_1415_project-review.md"
        );
    }

    #[test]
    fn the_note_filename_preview_explains_problems_while_typing() {
        assert_eq!(
            note_filename_preview("{date}_{title}", fixed_now()),
            "Example: 2026-11-04_project-review.md"
        );
        assert_eq!(
            note_filename_preview("notes/{title}", fixed_now()),
            "Enter a filename, not a folder path."
        );
    }

    #[test]
    fn a_shortcut_without_a_modifier_is_rejected_and_the_old_one_kept() {
        let (window, mut state, _) = cached_window();
        apply_hotkey(&window, &mut state, None, "N");
        assert_eq!(
            window.get_hotkey_error(),
            "Add Win, Ctrl or Alt so the shortcut doesn't block normal typing."
        );
        assert_eq!(state.settings.hotkey, "Win+Shift+N");
        assert_eq!(state.status, "Ready");
    }

    #[test]
    fn a_valid_shortcut_is_saved() {
        let (window, mut state, _) = cached_window();
        let folder = tempfile::tempdir().unwrap();
        state.settings_path = folder.path().join("settings.toml");
        window.set_hotkey_error("Earlier problem".into());
        apply_hotkey(&window, &mut state, None, " Ctrl+Alt+N ");
        assert_eq!(state.settings.hotkey, "Ctrl+Alt+N");
        assert_eq!(state.status, "Shortcut set to Ctrl+Alt+N.");
        assert_eq!(window.get_hotkey_text(), "Ctrl+Alt+N");
        assert_eq!(window.get_hotkey_error(), "");
    }
}
