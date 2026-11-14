//! On-device AI Tidy up. The user asks for a suggestion, reviews a word-level diff with
//! warnings about changed facts, then applies or discards it. The note never changes before
//! Apply, and an applied tidy-up can be undone while the note stays open.

use super::{AppEvent, AppState, persist_settings, refresh_view};
use crate::local_ai::{self, Availability, RewriteFailure};
use crate::ui_generated::MainWindow;
use noteiq_core::tidy::{
    TidyBlock, check_eligible, clean_output, diff_markdown, diff_words, fact_warnings,
};
use slint::{ComponentHandle, ModelRc, SharedString, StyledText, VecModel};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::Instant,
};

pub(super) struct TidyState {
    pub(super) availability: Availability,
    generation: u64,
    running: Option<Running>,
    preview: Option<Preview>,
    undo: Option<Undo>,
}

impl Default for TidyState {
    fn default() -> Self {
        Self {
            availability: Availability::Checking,
            generation: 0,
            running: None,
            preview: None,
            undo: None,
        }
    }
}

/// Each entry belongs to one note, so a suggestion can never land in another note.
struct Running {
    generation: u64,
    capture_id: String,
    original: String,
    cancel: Arc<AtomicBool>,
}

struct Preview {
    capture_id: String,
    original: String,
    suggestion: String,
}

struct Undo {
    capture_id: String,
    original: String,
    applied: String,
    was_tidied: bool,
}

/// A rewrite for the background worker.
pub(super) struct Job {
    generation: u64,
    text: String,
    cancel: Arc<AtomicBool>,
}

fn capture_id(state: &AppState) -> String {
    state.capture.capture.id.to_string()
}

pub(super) fn check_availability(sender: Sender<AppEvent>) {
    std::thread::spawn(move || {
        let _ = sender.send(AppEvent::TidyAvailability(local_ai::availability()));
    });
}

fn spawn(job: Job, sender: Sender<AppEvent>) {
    std::thread::spawn(move || {
        let result = local_ai::rewrite(&job.text, &job.cancel);
        let _ = sender.send(AppEvent::Tidy(job.generation, result));
    });
}

pub(super) fn bind(window: &MainWindow, state: &Rc<RefCell<AppState>>, sender: &Sender<AppEvent>) {
    let ui_weak = window.as_weak();
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        let sender = sender.clone();
        window.on_tidy_requested(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            if let Some(job) = start(&ui, &mut state.borrow_mut()) {
                spawn(job, sender.clone());
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_tidy_cancel_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                cancel(&ui, &mut state.borrow_mut());
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_tidy_apply_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                apply(&ui, &mut state.borrow_mut());
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_tidy_discard_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                discard(&ui, &mut state.borrow_mut());
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_tidy_undo_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                undo(&ui, &mut state.borrow_mut());
            }
        });
    }
    {
        let state = state.clone();
        window.on_tidy_setting_changed(move |enabled| {
            if let Some(ui) = ui_weak.upgrade() {
                set_enabled(&ui, &mut state.borrow_mut(), enabled);
            }
        });
    }
}

fn set_enabled(window: &MainWindow, state: &mut AppState, enabled: bool) {
    state.settings.tidy_up_enabled = enabled;
    if !enabled {
        if let Some(running) = state.tidy.running.take() {
            running.cancel.store(true, Ordering::Relaxed);
        }
        state.tidy.preview = None;
    }
    persist_settings(state);
    refresh_view(window, state);
}

/// Forgets suggestions and undo for the previous note, stopping any rewrite in progress.
pub(super) fn reset(tidy: &mut TidyState) {
    if let Some(running) = tidy.running.take() {
        running.cancel.store(true, Ordering::Relaxed);
    }
    tidy.preview = None;
    tidy.undo = None;
}

/// Starts a rewrite of the open note, or explains why it can't. The caller runs the job.
pub(super) fn start(window: &MainWindow, state: &mut AppState) -> Option<Job> {
    if !state.settings.tidy_up_enabled
        || state.tidy.availability != Availability::Ready
        || !state.capture_open
    {
        return None;
    }
    let id = capture_id(state);
    if state
        .tidy
        .preview
        .as_ref()
        .is_some_and(|p| p.capture_id == id)
    {
        return None;
    }
    if let Some(running) = state.tidy.running.take() {
        if running.capture_id == id {
            state.tidy.running = Some(running);
            return None;
        }
        running.cancel.store(true, Ordering::Relaxed);
    }
    let body = state.capture.capture.body.clone();
    let eligible = if state.capture.capture.is_empty() {
        Err(TidyBlock::Empty)
    } else {
        check_eligible(&body)
    };
    if let Err(block) = eligible {
        state.status = block.message();
        refresh_view(window, state);
        return None;
    }
    state.tidy.generation += 1;
    let cancel = Arc::new(AtomicBool::new(false));
    state.tidy.running = Some(Running {
        generation: state.tidy.generation,
        capture_id: id,
        original: body.clone(),
        cancel: cancel.clone(),
    });
    refresh_view(window, state);
    Some(Job {
        generation: state.tidy.generation,
        text: body,
        cancel,
    })
}

fn cancel(window: &MainWindow, state: &mut AppState) {
    if let Some(running) = state.tidy.running.take() {
        running.cancel.store(true, Ordering::Relaxed);
        state.status = "Tidy up cancelled. Your note is unchanged.".to_owned();
    }
    refresh_view(window, state);
}

/// Shows a finished rewrite for review. Results for another note, a cancelled run, or a note
/// that changed in the meantime are dropped.
pub(super) fn finish(
    window: &MainWindow,
    state: &mut AppState,
    generation: u64,
    result: Result<String, RewriteFailure>,
) {
    let Some(running) = state
        .tidy
        .running
        .take_if(|running| running.generation == generation)
    else {
        return;
    };
    if running.capture_id != capture_id(state) {
        return;
    }
    match result {
        Err(RewriteFailure::Cancelled) => {}
        Err(RewriteFailure::Failed(message)) => {
            state.status = format!("Could not tidy up: {message}");
        }
        Ok(_) if state.capture.capture.body != running.original => {
            state.status = "The note changed while it was being tidied up. Try again.".to_owned();
        }
        Ok(raw) => {
            let suggestion = clean_output(&raw, &running.original);
            if suggestion.trim().is_empty() {
                state.status =
                    "Could not tidy up: the on-device AI model returned nothing.".to_owned();
            } else if suggestion == running.original {
                state.status = "Tidy up found nothing to change.".to_owned();
            } else {
                show_preview(window, &running.original, &suggestion);
                state.tidy.preview = Some(Preview {
                    capture_id: running.capture_id,
                    original: running.original,
                    suggestion,
                });
            }
        }
    }
    refresh_view(window, state);
}

fn show_preview(window: &MainWindow, original: &str, suggestion: &str) {
    let markdown = diff_markdown(&diff_words(original, suggestion));
    let diff = StyledText::from_markdown(&markdown).unwrap_or_else(|error| {
        tracing::warn!(%error, "Could not render the tidy-up changes");
        StyledText::from_plain_text(suggestion)
    });
    window.set_tidy_diff(diff);
    let warnings = fact_warnings(original, suggestion)
        .iter()
        .map(|warning| SharedString::from(warning.message()))
        .collect::<Vec<_>>();
    window.set_tidy_warnings(ModelRc::new(VecModel::from(warnings)));
}

fn apply(window: &MainWindow, state: &mut AppState) {
    let Some(preview) = state.tidy.preview.take() else {
        return;
    };
    let id = capture_id(state);
    if preview.capture_id != id || state.capture.capture.body != preview.original {
        state.status = "The note changed. Run Tidy up again.".to_owned();
        refresh_view(window, state);
        return;
    }
    state.tidy.undo = Some(Undo {
        capture_id: id,
        original: preview.original,
        applied: preview.suggestion.clone(),
        was_tidied: state.capture.capture.ai_tidied,
    });
    state.capture.capture.ai_tidied = true;
    state
        .capture
        .edit(preview.suggestion.clone(), Instant::now());
    window.set_note_text(preview.suggestion.into());
    window.set_saved(false);
    state.status =
        "Tidied up with on-device AI. Press Ctrl+Z or select Undo to restore your note.".to_owned();
    refresh_view(window, state);
}

fn discard(window: &MainWindow, state: &mut AppState) {
    if state.tidy.preview.take().is_some() {
        state.status = "Kept your note as it was.".to_owned();
    }
    refresh_view(window, state);
}

/// Restores the note from before the tidy-up, discarding any edits made since.
fn undo(window: &MainWindow, state: &mut AppState) {
    let Some(undo) = state.tidy.undo.take() else {
        return;
    };
    if undo.capture_id != capture_id(state) {
        refresh_view(window, state);
        return;
    }
    state.capture.capture.ai_tidied = undo.was_tidied;
    state.capture.edit(undo.original.clone(), Instant::now());
    window.set_note_text(undo.original.into());
    window.set_saved(false);
    state.status = "Restored your note from before the tidy-up.".to_owned();
    refresh_view(window, state);
}

pub(super) fn refresh(window: &MainWindow, state: &AppState) {
    let id = capture_id(state);
    let tidy = &state.tidy;
    window.set_tidy_setting(state.settings.tidy_up_enabled);
    window.set_tidy_available(
        state.settings.tidy_up_enabled
            && tidy.availability == Availability::Ready
            && state.capture_open,
    );
    window.set_tidy_availability_text(tidy.availability.description().into());
    window.set_tidy_running(tidy.running.as_ref().is_some_and(|r| r.capture_id == id));
    window.set_tidy_preview_visible(tidy.preview.as_ref().is_some_and(|p| p.capture_id == id));
    let undo = tidy.undo.as_ref().filter(|undo| undo.capture_id == id);
    let armed = undo.is_some_and(|undo| state.capture.capture.body == undo.applied);
    window.set_tidy_undo_available(undo.is_some());
    window.set_tidy_undo_armed(armed);
    window.set_tidy_undo_label(
        if armed {
            "Undo tidy-up"
        } else {
            "Restore note from before tidy-up"
        }
        .into(),
    );
}

#[cfg(test)]
mod tests {
    use super::super::tests::{init_backend, test_settings};
    use super::*;
    use noteiq_core::model::CaptureSource;
    use slint::Model;
    use std::path::Path;

    const ROUGH: &str = "q4 roadmap review w/ alex, priya\n- alex: ship sync beta by dec 5?? depends on api freeze\n- todo me: send benchmark plan to priya by fri";
    const TIDIED: &str = "- Review roadmap with Alex and Priya.\n- Alex: Ship sync beta by Dec 5, depending on API freeze.\n- Task: Send benchmark plan to Priya by Friday.";

    /// Tidy up turned on and the model ready, with settings saved under `folder`.
    fn ready_window(folder: &Path) -> (MainWindow, AppState) {
        init_backend();
        let window = MainWindow::new().unwrap();
        let mut settings = test_settings();
        settings.auto_connect_meeting = false;
        settings.tidy_up_enabled = true;
        let mut state = AppState::new(settings, folder.join("settings.toml"), CaptureSource::Cli);
        state.tidy.availability = Availability::Ready;
        refresh_view(&window, &state);
        (window, state)
    }

    fn type_text(window: &MainWindow, state: &mut AppState, text: &str) {
        state.capture.edit(text.to_owned(), Instant::now());
        window.set_note_text(text.into());
        refresh_view(window, state);
    }

    fn warnings(window: &MainWindow) -> Vec<String> {
        window
            .get_tidy_warnings()
            .iter()
            .map(|warning| warning.to_string())
            .collect()
    }

    #[test]
    fn tidy_up_is_offered_only_when_turned_on_and_windows_can_run_the_model() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = ready_window(folder.path());
        assert!(window.get_tidy_available());

        state.tidy.availability = Availability::Unavailable("Not on this PC.".into());
        refresh_view(&window, &state);
        assert!(!window.get_tidy_available());
        assert_eq!(window.get_tidy_availability_text(), "Not on this PC.");

        state.tidy.availability = Availability::Ready;
        set_enabled(&window, &mut state, false);
        assert!(!window.get_tidy_available());
        assert!(!window.get_tidy_setting());
        assert!(start(&window, &mut state).is_none());
        let saved = std::fs::read_to_string(folder.path().join("settings.toml")).unwrap();
        assert!(saved.contains("tidy_up_enabled = false"));
    }

    #[test]
    fn a_suggestion_is_reviewed_before_it_changes_the_note_and_can_be_undone() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = ready_window(folder.path());
        type_text(&window, &mut state, ROUGH);

        let job = start(&window, &mut state).expect("a rewrite starts");
        assert_eq!(job.text, ROUGH);
        assert!(window.get_tidy_running());
        assert!(
            start(&window, &mut state).is_none(),
            "one rewrite at a time"
        );

        finish(
            &window,
            &mut state,
            job.generation,
            Ok(format!("{TIDIED}\r\n")),
        );
        assert!(!window.get_tidy_running());
        assert!(window.get_tidy_preview_visible());
        assert_eq!(
            window.get_note_text(),
            ROUGH,
            "nothing changes before Apply"
        );
        assert_eq!(state.capture.capture.body, ROUGH);
        assert!(!state.capture.capture.ai_tidied);
        let shown = warnings(&window);
        assert!(shown.iter().any(|w| w.contains("uncertain")), "{shown:?}");
        assert!(shown.iter().any(|w| w.contains("\"me\"")), "{shown:?}");

        apply(&window, &mut state);
        assert!(!window.get_tidy_preview_visible());
        assert_eq!(window.get_note_text(), TIDIED);
        assert_eq!(state.capture.capture.body, TIDIED);
        assert!(state.capture.capture.ai_tidied);
        assert!(state.capture.is_dirty());
        assert!(window.get_tidy_undo_available());
        assert!(window.get_tidy_undo_armed());
        assert_eq!(window.get_tidy_undo_label(), "Undo tidy-up");

        undo(&window, &mut state);
        assert_eq!(window.get_note_text(), ROUGH);
        assert_eq!(state.capture.capture.body, ROUGH);
        assert!(!state.capture.capture.ai_tidied);
        assert!(!window.get_tidy_undo_available());
        assert!(!window.get_tidy_undo_armed());
    }

    #[test]
    fn discarding_keeps_the_note_and_its_provenance() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = ready_window(folder.path());
        type_text(&window, &mut state, ROUGH);
        let job = start(&window, &mut state).unwrap();
        finish(&window, &mut state, job.generation, Ok(TIDIED.into()));
        discard(&window, &mut state);
        assert!(!window.get_tidy_preview_visible());
        assert_eq!(state.capture.capture.body, ROUGH);
        assert!(!state.capture.capture.ai_tidied);
        assert!(!window.get_tidy_undo_available());
        assert_eq!(window.get_status_text(), "Kept your note as it was.");
    }

    #[test]
    fn edits_after_applying_turn_undo_into_an_explicit_restore() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = ready_window(folder.path());
        type_text(&window, &mut state, ROUGH);
        let job = start(&window, &mut state).unwrap();
        finish(&window, &mut state, job.generation, Ok(TIDIED.into()));
        apply(&window, &mut state);

        type_text(&window, &mut state, &format!("{TIDIED}\n- more"));
        assert!(window.get_tidy_undo_available());
        assert!(
            !window.get_tidy_undo_armed(),
            "Ctrl+Z goes back to the editor"
        );
        assert_eq!(
            window.get_tidy_undo_label(),
            "Restore note from before tidy-up"
        );
        undo(&window, &mut state);
        assert_eq!(state.capture.capture.body, ROUGH);
    }

    #[test]
    fn results_for_a_changed_or_replaced_note_are_dropped() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = ready_window(folder.path());
        type_text(&window, &mut state, ROUGH);
        let job = start(&window, &mut state).unwrap();
        // A template or reopened note can change the text while the model runs.
        state.capture.edit(format!("{ROUGH}\nmore"), Instant::now());
        finish(&window, &mut state, job.generation, Ok(TIDIED.into()));
        assert!(!window.get_tidy_preview_visible());
        assert!(window.get_status_text().contains("changed while"));

        type_text(&window, &mut state, ROUGH);
        let job = start(&window, &mut state).unwrap();
        state.capture =
            noteiq_core::capture::CaptureController::new(noteiq_core::model::Capture::new(
                CaptureSource::NewNote,
                chrono::Local::now().fixed_offset(),
                "UTC".into(),
            ));
        reset(&mut state.tidy);
        assert!(
            job.cancel.load(Ordering::Relaxed),
            "the old rewrite is stopped"
        );
        finish(&window, &mut state, job.generation, Ok(TIDIED.into()));
        assert!(!window.get_tidy_preview_visible());
        assert_eq!(state.capture.capture.body, "");
    }

    #[test]
    fn applying_after_the_note_changed_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = ready_window(folder.path());
        type_text(&window, &mut state, ROUGH);
        let job = start(&window, &mut state).unwrap();
        finish(&window, &mut state, job.generation, Ok(TIDIED.into()));
        state
            .capture
            .edit(format!("{ROUGH}\ninserted"), Instant::now());
        apply(&window, &mut state);
        assert_eq!(state.capture.capture.body, format!("{ROUGH}\ninserted"));
        assert!(!state.capture.capture.ai_tidied);
        assert_eq!(
            window.get_status_text(),
            "The note changed. Run Tidy up again."
        );
    }

    #[test]
    fn cancelling_stops_the_model_and_ignores_its_late_result() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = ready_window(folder.path());
        type_text(&window, &mut state, ROUGH);
        let job = start(&window, &mut state).unwrap();
        cancel(&window, &mut state);
        assert!(job.cancel.load(Ordering::Relaxed));
        assert!(!window.get_tidy_running());
        finish(&window, &mut state, job.generation, Ok(TIDIED.into()));
        assert!(!window.get_tidy_preview_visible());
        assert_eq!(
            window.get_status_text(),
            "Tidy up cancelled. Your note is unchanged."
        );
    }

    #[test]
    fn empty_long_or_failed_notes_are_explained() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = ready_window(folder.path());
        assert!(start(&window, &mut state).is_none());
        assert!(window.get_status_text().contains("Write something first"));

        type_text(&window, &mut state, &"word ".repeat(200));
        assert!(start(&window, &mut state).is_none());
        assert!(window.get_status_text().contains("up to 700 characters"));

        type_text(&window, &mut state, ROUGH);
        let job = start(&window, &mut state).unwrap();
        finish(
            &window,
            &mut state,
            job.generation,
            Err(RewriteFailure::Failed("Blocked.".into())),
        );
        assert_eq!(window.get_status_text(), "Could not tidy up: Blocked.");
        assert!(window.get_status_error());

        let job = start(&window, &mut state).unwrap();
        finish(
            &window,
            &mut state,
            job.generation,
            Ok(format!("{ROUGH}\n")),
        );
        assert_eq!(window.get_status_text(), "Tidy up found nothing to change.");
    }

    #[test]
    fn diffs_of_real_notes_render_as_styled_text() {
        let samples = [
            (ROUGH, TIDIED),
            (
                "# heading?\n> quote | pipe\n1. one\n    code-ish <b>tag</b> [link](x) `tick` *star* _under_ ~tilde~",
                "Heading:\n- quote, pipe\n1) one <i>tag</i> ~~strike~~ **bold**",
            ),
        ];
        for (before, after) in samples {
            let markdown = diff_markdown(&diff_words(before, after));
            let styled = StyledText::from_markdown(&markdown);
            assert!(styled.is_ok(), "{markdown}: {:?}", styled.err());
        }
    }
}
