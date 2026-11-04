use crate::ui_generated::MainWindow;
use crate::{
    error_ui::meeting_failure,
    foreground,
    hotkey::{self, HotkeyController},
    instance::InstanceRole,
    meeting_ui::select_and_apply,
    open_note::open_note,
    settings_ui::choose_destination,
    tray::{TrayAction, TrayController},
    workiq_windows::discover_executable,
};
use chrono::Local;
use noteiq_core::{
    capture::CaptureController,
    destination::{Destination, DestinationError},
    diagnostics::{DiagnosticEvent, DiagnosticOperation, FailureClass},
    meeting::MeetingSelection,
    model::{Capture, CaptureSource, Meeting, MeetingDecision},
    settings::Settings,
    workiq::{WorkiqClient, WorkiqError},
};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

enum AppEvent {
    Meetings(u64, bool, Result<Vec<Meeting>, WorkiqError>, Instant),
}

struct AppState {
    capture: CaptureController,
    /// False once the capture has been closed and its window hidden; the next launch request
    /// then starts a fresh capture instead of reshowing the finished one.
    capture_open: bool,
    settings: Settings,
    settings_path: PathBuf,
    status: String,
    meeting_label: String,
    meeting_options: Vec<String>,
    meetings: Vec<Meeting>,
    meeting_picker_visible: bool,
    lookup_id: u64,
    destination_error: bool,
}

impl AppState {
    fn new(settings: Settings, settings_path: PathBuf, source: CaptureSource) -> Self {
        let now = Local::now().fixed_offset();
        let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "local".to_owned());
        Self {
            capture: CaptureController::new(Capture::new(source, now, timezone)),
            capture_open: true,
            settings,
            settings_path,
            status: "Ready".to_owned(),
            meeting_label: "Checking for a meeting…".to_owned(),
            meeting_options: Vec::new(),
            meetings: Vec::new(),
            meeting_picker_visible: false,
            lookup_id: 0,
            destination_error: false,
        }
    }

    fn save(&mut self) -> Result<Option<PathBuf>, DestinationError> {
        let destination = Destination::new(self.settings.destination_root.clone());
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
    let window = MainWindow::new()?;
    let state = std::rc::Rc::new(std::cell::RefCell::new(AppState::new(
        settings,
        settings_path,
        source,
    )));
    if let Some(message) = startup_message {
        state.borrow_mut().status = message;
    }

    let (sender, events) = mpsc::channel();
    let ipc = match role {
        InstanceRole::Primary(receiver) => Some(receiver),
        InstanceRole::Forwarded => None,
    };
    let tray = match TrayController::new() {
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

    configure_view(&window, &state);
    let _timer = bind_callbacks(&window, &state, sender.clone(), hotkeys, tray, ipc, events);
    if let Err(message) = crate::uri::register_protocol() {
        tracing::warn!(failure_class = "uri_registration_failed");
        state.borrow_mut().status = message.to_owned();
    }

    window.show()?;
    foreground::bring_to_front(window.window());
    start_meeting_lookup(&state, &sender, false);
    if has_tray {
        // Closing the capture only hides the window; the tray keeps NoteIQ ready for the next
        // shortcut, link or command until the user chooses Quit.
        slint::run_event_loop_until_quit()
    } else {
        slint::run_event_loop()
    }
}

fn configure_view(window: &MainWindow, state: &std::rc::Rc<std::cell::RefCell<AppState>>) {
    let state = state.borrow();
    window.set_note_text(state.capture.capture.body.clone().into());
    window.set_status_text(state.status.clone().into());
    window.set_meeting_label(state.meeting_label.clone().into());
    window.set_destination_label(state.settings.destination_root.display().to_string().into());
    window.set_auto_connect(state.settings.auto_connect_meeting);
    window.set_hotkey_text(state.settings.hotkey.clone().into());
    window.set_hotkey_enabled(state.settings.hotkey_enabled);
    window.set_has_note(state.capture.capture.path.is_some());
    window.set_saved(!state.capture.is_dirty());
    window.set_meeting_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
}

fn bind_callbacks(
    window: &MainWindow,
    state: &std::rc::Rc<std::cell::RefCell<AppState>>,
    sender: Sender<AppEvent>,
    hotkeys: Option<HotkeyController>,
    tray: Option<TrayController>,
    ipc: Option<Receiver<CaptureSource>>,
    events: Receiver<AppEvent>,
) -> Timer {
    let hotkeys = std::rc::Rc::new(std::cell::RefCell::new(hotkeys));
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
        window.on_close_requested(move || {
            if let Some(ui) = ui_weak.upgrade()
                && close_capture(&ui, &state)
            {
                ui.hide().ok();
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
            start_meeting_lookup(&state, &sender, true);
            if let Some(ui) = ui_weak.upgrade() {
                refresh_view(&ui, &state.borrow());
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_connect_requested(move |index| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            let Some(meeting) = usize::try_from(index)
                .ok()
                .and_then(|index| state.meetings.get(index))
                .cloned()
            else {
                state.status = "Choose a meeting before connecting.".to_owned();
                refresh_view(&ui, &state);
                return;
            };
            state.capture.connect_meeting(meeting.clone());
            state.meeting_label = meeting
                .title
                .unwrap_or_else(|| "Connected to current meeting".to_owned());
            state.meeting_picker_visible = false;
            state.status = "Meeting connected.".to_owned();
            persist_meeting_change(&mut state);
            refresh_view(&ui, &state);
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_disconnect_requested(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            state.capture.disconnect_meeting();
            state.meeting_label = "No meeting".to_owned();
            state.meeting_picker_visible = false;
            state.status = "Meeting disconnected.".to_owned();
            persist_meeting_change(&mut state);
            refresh_view(&ui, &state);
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
            refresh_view(&ui, &state);
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
            let value = value.trim().to_owned();
            let mut state = state.borrow_mut();
            if value == state.settings.hotkey {
                ui.set_hotkey_text(value.into());
                return;
            }
            let enabled = state.settings.hotkey_enabled;
            let result = hotkey::validate(&value).and_then(|()| {
                hotkeys
                    .borrow_mut()
                    .as_mut()
                    .map_or(Ok(()), |controller| controller.update(&value, enabled))
            });
            match result {
                Ok(()) => {
                    state.settings.hotkey = value.clone();
                    if persist_settings(&mut state) {
                        state.status = if enabled {
                            format!("Shortcut set to {value}.")
                        } else {
                            format!("Shortcut saved as {value}. Turn it on to use it.")
                        };
                    }
                }
                Err(message) => {
                    tracing::warn!(failure_class = "hotkey_rejected");
                    state.status = message;
                }
            }
            ui.set_hotkey_text(state.settings.hotkey.clone().into());
            refresh_view(&ui, &state);
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
                AppEvent::Meetings(id, manual, result, started) if id == state.lookup_id => {
                    process_meeting_result(&ui, &mut state, manual, result, started);
                }
                AppEvent::Meetings(_, _, _, _) => {}
            }
        }
        if let Some(ipc) = ipc.as_ref() {
            for source in ipc.try_iter() {
                ensure_capture(&ui, &mut state, &sender_for_timer, source);
                present = true;
            }
        }
        if let Some(tray) = tray.as_ref() {
            match tray.poll() {
                Some(TrayAction::Show) => {
                    ensure_capture(&ui, &mut state, &sender_for_timer, CaptureSource::Tray);
                    present = true;
                }
                Some(TrayAction::New) => {
                    start_new_capture_state(&ui, &mut state, &sender_for_timer);
                    present = true;
                }
                Some(TrayAction::Quit) => {
                    if close_capture_state(&ui, &mut state) {
                        slint::quit_event_loop().ok();
                    } else {
                        present = true;
                    }
                }
                None => {}
            }
        }
        if hotkeys
            .borrow()
            .as_ref()
            .is_some_and(|hotkeys| hotkeys.was_pressed())
        {
            ensure_capture(&ui, &mut state, &sender_for_timer, CaptureSource::Shortcut);
            present = true;
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

fn persist_meeting_change(state: &mut AppState) {
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

fn save_and_refresh(window: &MainWindow, state: &std::rc::Rc<std::cell::RefCell<AppState>>) {
    let mut state = state.borrow_mut();
    save_state(&mut state);
    refresh_view(window, &state);
}

fn close_capture(window: &MainWindow, state: &std::rc::Rc<std::cell::RefCell<AppState>>) -> bool {
    let mut state = state.borrow_mut();
    close_capture_state(window, &mut state)
}

fn close_capture_state(window: &MainWindow, state: &mut AppState) -> bool {
    if !state.capture_open {
        return true;
    }
    if !state.capture.capture.is_empty() {
        save_state(state);
        if state.capture.capture.path.is_none() || state.capture.is_dirty() {
            refresh_view(window, state);
            return false;
        }
    }
    if let Err(error) = state.capture.finish() {
        state.status = format!("Could not finish the note: {error}");
        refresh_view(window, state);
        return false;
    }
    state.capture_open = false;
    state.status = "Capture closed.".to_owned();
    refresh_view(window, state);
    true
}

fn start_new_capture(
    window: &MainWindow,
    state: &std::rc::Rc<std::cell::RefCell<AppState>>,
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

fn begin_capture(
    window: &MainWindow,
    state: &mut AppState,
    sender: &Sender<AppEvent>,
    source: CaptureSource,
) {
    let now = Local::now().fixed_offset();
    let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "local".to_owned());
    state.capture = CaptureController::new(Capture::new(source, now, timezone));
    state.capture_open = true;
    state.status = "Ready".to_owned();
    state.meeting_label = "Checking for a meeting…".to_owned();
    state.meetings.clear();
    state.meeting_options.clear();
    state.meeting_picker_visible = false;
    window.set_note_text("".into());
    window.set_settings_visible(false);
    window.set_meeting_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    refresh_view(window, state);
    start_meeting_lookup_state(state, sender, false);
}

fn start_meeting_lookup(
    state: &std::rc::Rc<std::cell::RefCell<AppState>>,
    sender: &Sender<AppEvent>,
    manual: bool,
) {
    start_meeting_lookup_state(&mut state.borrow_mut(), sender, manual);
}

fn start_meeting_lookup_state(state: &mut AppState, sender: &Sender<AppEvent>, manual: bool) {
    state.lookup_id += 1;
    state.meeting_label = "Checking for a meeting…".to_owned();
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
        let _ = sender.send(AppEvent::Meetings(id, manual, result, started));
    });
}

fn process_meeting_result(
    window: &MainWindow,
    state: &mut AppState,
    manual: bool,
    result: Result<Vec<Meeting>, WorkiqError>,
    started: Instant,
) {
    match result {
        Ok(meetings) => {
            state.meetings = meetings;
            let selection = select_and_apply(
                &mut state.capture.capture,
                &state.meetings,
                Local::now().fixed_offset(),
                state.settings.auto_connect_meeting,
            );
            if matches!(
                state.capture.capture.meeting_decision,
                MeetingDecision::Selected | MeetingDecision::Disconnected
            ) {
                state.meeting_label = state
                    .capture
                    .capture
                    .meeting
                    .as_ref()
                    .and_then(|meeting| meeting.title.clone())
                    .unwrap_or_else(|| "No meeting".to_owned());
                if manual {
                    let choices = match &selection {
                        MeetingSelection::Connected(meeting) => vec![meeting.clone()],
                        MeetingSelection::NeedsChoice(meetings) => meetings.clone(),
                        MeetingSelection::NoMeeting => Vec::new(),
                    };
                    if !choices.is_empty() {
                        show_meeting_choices(window, state, choices);
                    }
                }
            } else {
                match selection {
                    MeetingSelection::Connected(meeting) => {
                        state.meeting_label = meeting
                            .title
                            .unwrap_or_else(|| "Connected to current meeting".to_owned());
                        persist_meeting_change(state);
                    }
                    MeetingSelection::NeedsChoice(meetings) => {
                        show_meeting_choices(window, state, meetings);
                    }
                    MeetingSelection::NoMeeting => {
                        state.meeting_label = "No current meeting".to_owned();
                        state.meeting_picker_visible = false;
                        state.meeting_options.clear();
                    }
                }
            }
        }
        Err(error) => {
            let (message, event) = meeting_failure(&error, started);
            tracing::warn!(
                correlation_id = %event.correlation_id,
                operation = ?event.operation,
                failure_class = ?event.failure_class,
                "Meeting lookup failed"
            );
            state.meeting_label = message;
            state.meeting_picker_visible = false;
        }
    }
    refresh_view(window, state);
}

fn show_meeting_choices(window: &MainWindow, state: &mut AppState, meetings: Vec<Meeting>) {
    state.meetings = meetings;
    state.meeting_options = state
        .meetings
        .iter()
        .enumerate()
        .map(|(index, meeting)| {
            format!(
                "{}. {}",
                index + 1,
                meeting.title.as_deref().unwrap_or("Untitled meeting")
            )
        })
        .collect();
    state.meeting_label = "Choose the meeting to connect".to_owned();
    state.meeting_picker_visible = true;
    let options = state
        .meeting_options
        .iter()
        .map(|value| SharedString::from(value.as_str()))
        .collect::<Vec<_>>();
    window.set_meeting_options(ModelRc::new(VecModel::from(options)));
    window.set_meeting_index(0);
}

fn refresh_view(window: &MainWindow, state: &AppState) {
    window.set_status_text(state.status.clone().into());
    window.set_meeting_label(state.meeting_label.clone().into());
    window.set_destination_label(state.settings.destination_root.display().to_string().into());
    window.set_auto_connect(state.settings.auto_connect_meeting);
    window.set_hotkey_enabled(state.settings.hotkey_enabled);
    window.set_meeting_picker_visible(state.meeting_picker_visible);
    window.set_has_meeting(state.capture.capture.meeting.is_some());
    window.set_has_note(state.capture.capture.path.is_some());
    window.set_saved(!state.capture.is_dirty());
    window.set_destination_error(state.destination_error);
}
