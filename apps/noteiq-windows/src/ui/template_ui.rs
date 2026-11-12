//! Meeting templates in Settings and the capture window.

use super::{AppState, persist_settings, refresh_view};
use crate::ui_generated::MainWindow;
use chrono::{DateTime, FixedOffset, Local};
use noteiq_core::meeting_template::{
    DEFAULT_TEMPLATE_ID, MeetingTemplate, TemplateInsertion, TemplateStore, default_template,
    insert_template, needs_template_sync, sync_capture_template,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc, time::Instant};

fn store(state: &AppState) -> TemplateStore {
    TemplateStore::beside_settings(&state.settings_path)
}

/// Inserts the default template when the note was connected to a different meeting, or
/// removes an untouched one when it was disconnected.
pub(super) fn sync_meeting_template(
    window: &MainWindow,
    state: &mut AppState,
    now: DateTime<FixedOffset>,
) {
    if !needs_template_sync(&state.capture.capture) {
        return;
    }
    let template = default_template(&state.settings, &store(state));
    if sync_capture_template(&mut state.capture.capture, template.as_ref(), now) {
        state.capture.metadata_changed(Instant::now());
        window.set_note_text(state.capture.capture.body.clone().into());
    }
}

/// Shows the saved template settings and lists, with the default template in the editor.
pub(super) fn reset_view(window: &MainWindow, state: &AppState) {
    window.set_templates_enabled(state.settings.meeting_templates_enabled);
    let templates = load_lists(window, state);
    let index = usize::try_from(window.get_default_template_index()).unwrap_or(0);
    if let Some(template) = templates.get(index).or_else(|| templates.first()) {
        show_in_editor(window, &templates, template);
    }
}

/// Fills the template pickers and selects the default template. A default that no longer
/// exists shows as General meeting, which is what new notes use.
fn load_lists(window: &MainWindow, state: &AppState) -> Vec<MeetingTemplate> {
    let templates = store(state).list();
    let names = templates
        .iter()
        .map(|template| SharedString::from(template.name.as_str()))
        .collect::<Vec<_>>();
    let ids = templates
        .iter()
        .map(|template| SharedString::from(template.id.as_str()))
        .collect::<Vec<_>>();
    window.set_template_names(ModelRc::new(VecModel::from(names)));
    window.set_template_ids(ModelRc::new(VecModel::from(ids)));
    let default = index_of(&templates, &state.settings.default_meeting_template)
        .or_else(|| index_of(&templates, DEFAULT_TEMPLATE_ID))
        .unwrap_or(0);
    window.set_default_template_index(default);
    window.set_note_template_index(default);
    templates
}

fn index_of(templates: &[MeetingTemplate], id: &str) -> Option<i32> {
    templates
        .iter()
        .position(|template| template.id == id)
        .and_then(|position| i32::try_from(position).ok())
}

/// Shows `template` in the editor and remembers its id, so Save and Delete act on it even if
/// the template folder changes and the lists are reordered.
fn show_in_editor(window: &MainWindow, templates: &[MeetingTemplate], template: &MeetingTemplate) {
    window.set_edit_template_index(index_of(templates, &template.id).unwrap_or(0));
    window.set_edit_template_id(template.id.clone().into());
    window.set_template_name_text(template.name.clone().into());
    window.set_template_body_text(template.body.clone().into());
    window.set_template_editable(!template.built_in);
    window.set_template_is_new(false);
    set_message(window, "", false);
}

fn set_message(window: &MainWindow, message: &str, error: bool) {
    window.set_template_message(message.into());
    window.set_template_message_error(error);
}

fn template_id(window: &MainWindow, index: i32) -> Option<String> {
    let index = usize::try_from(index).ok()?;
    window
        .get_template_ids()
        .row_data(index)
        .map(|id| id.to_string())
}

fn set_templates_enabled(
    window: &MainWindow,
    state: &mut AppState,
    enabled: bool,
    now: DateTime<FixedOffset>,
) {
    state.settings.meeting_templates_enabled = enabled;
    persist_settings(state);
    sync_meeting_template(window, state, now);
    refresh_view(window, state);
}

fn choose_default_template(window: &MainWindow, state: &mut AppState, index: i32) {
    let Some(id) = template_id(window, index) else {
        return;
    };
    window.set_note_template_index(index);
    if id != state.settings.default_meeting_template {
        state.settings.default_meeting_template = id;
        persist_settings(state);
    }
    refresh_view(window, state);
}

/// Adds the chosen template to the open note: it replaces an empty note or an untouched
/// template, and is added after any text the user wrote.
fn insert_note_template(
    window: &MainWindow,
    state: &mut AppState,
    index: i32,
    now: DateTime<FixedOffset>,
) {
    let template = template_id(window, index).and_then(|id| match store(state).get(&id) {
        Ok(template) => template,
        Err(error) => {
            tracing::warn!(
                failure_class = "meeting_template_unreadable",
                error_kind = error.kind()
            );
            None
        }
    });
    let Some(template) = template else {
        state.status = "Could not insert the template. It may have been deleted.".to_owned();
        refresh_view(window, state);
        return;
    };
    let insertion = insert_template(&mut state.capture.capture, &template, now);
    let body = state.capture.capture.body.clone();
    match insertion {
        TemplateInsertion::Replaced => {
            state.capture.metadata_changed(Instant::now());
            state.status = format!("Inserted the {} template.", template.name);
        }
        TemplateInsertion::Appended => {
            state.capture.edit(body.clone(), Instant::now());
            state.status = format!("Added the {} template below your notes.", template.name);
        }
    }
    window.set_note_text(body.into());
    refresh_view(window, state);
}

/// Shows the template the user picked, identified by the id listed at `index` when they
/// picked it, and refreshes the lists from the folder.
fn edit_template(window: &MainWindow, state: &AppState, index: i32) {
    let Some(id) = template_id(window, index) else {
        return;
    };
    show_template(window, state, &id);
}

/// Reloads the lists and shows the template `id`, or General meeting if it is gone.
fn show_template(window: &MainWindow, state: &AppState, id: &str) {
    let templates = load_lists(window, state);
    match templates.iter().find(|template| template.id == id) {
        Some(template) => show_in_editor(window, &templates, template),
        None => {
            if let Some(first) = templates.first() {
                show_in_editor(window, &templates, first);
            }
            set_message(window, "That template no longer exists.", true);
        }
    }
}

/// Starts a new template in the editor, blank or as a copy of the one shown.
fn new_template(window: &MainWindow, copy: bool) {
    if copy {
        let name = format!("{} copy", window.get_template_name_text());
        window.set_template_name_text(name.into());
    } else {
        window.set_template_name_text(SharedString::new());
        window.set_template_body_text(SharedString::new());
    }
    window.set_edit_template_id(SharedString::new());
    window.set_template_editable(true);
    window.set_template_is_new(true);
    set_message(window, "", false);
}

/// The id of the saved template in the editor; `None` while it holds a new template.
fn editor_template_id(window: &MainWindow) -> Option<String> {
    let id = window.get_edit_template_id();
    (!window.get_template_is_new() && !id.is_empty()).then(|| id.to_string())
}

fn save_template(window: &MainWindow, state: &mut AppState, name: &str, body: &str) {
    let store = store(state);
    let previous = editor_template_id(window);
    let result = match previous.as_deref() {
        Some(id) => store.update(id, name, body),
        None => store.create(name, body),
    };
    match result {
        Ok(saved) => {
            if previous.is_some_and(|id| id == state.settings.default_meeting_template) {
                state.settings.default_meeting_template = saved.id.clone();
                persist_settings(state);
            }
            show_template(window, state, &saved.id);
            set_message(window, "Template saved.", false);
        }
        Err(error) => {
            tracing::warn!(
                failure_class = "meeting_template_rejected",
                error_kind = error.kind()
            );
            set_message(window, &error.to_string(), true);
        }
    }
    refresh_view(window, state);
}

/// Deletes the user template in the editor. If it was the default, new meeting notes use
/// General meeting instead.
fn delete_template(window: &MainWindow, state: &mut AppState) {
    let Some(id) = editor_template_id(window) else {
        return;
    };
    match store(state).delete(&id) {
        Ok(()) => {
            if id == state.settings.default_meeting_template {
                state.settings.default_meeting_template = DEFAULT_TEMPLATE_ID.to_owned();
                persist_settings(state);
            }
            show_template(window, state, DEFAULT_TEMPLATE_ID);
            set_message(window, &format!("Deleted the {id} template."), false);
        }
        Err(error) => {
            tracing::warn!(
                failure_class = "meeting_template_delete_failed",
                error_kind = error.kind()
            );
            set_message(window, &error.to_string(), true);
        }
    }
    refresh_view(window, state);
}

pub(super) fn bind(window: &MainWindow, state: &Rc<RefCell<AppState>>) {
    let ui_weak = window.as_weak();
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_templates_enabled_changed(move |enabled| {
            if let Some(ui) = ui_weak.upgrade() {
                set_templates_enabled(
                    &ui,
                    &mut state.borrow_mut(),
                    enabled,
                    Local::now().fixed_offset(),
                );
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_default_template_chosen(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                choose_default_template(&ui, &mut state.borrow_mut(), index);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_note_template_insert_requested(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                insert_note_template(
                    &ui,
                    &mut state.borrow_mut(),
                    index,
                    Local::now().fixed_offset(),
                );
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_template_edit_chosen(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                edit_template(&ui, &state.borrow(), index);
            }
        });
    }
    {
        let ui_weak = ui_weak.clone();
        window.on_template_new_requested(move |copy| {
            if let Some(ui) = ui_weak.upgrade() {
                new_template(&ui, copy);
            }
        });
    }
    {
        let state = state.clone();
        let ui_weak = ui_weak.clone();
        window.on_template_save_requested(move |name, body| {
            if let Some(ui) = ui_weak.upgrade() {
                save_template(&ui, &mut state.borrow_mut(), &name, &body);
            }
        });
    }
    {
        let state = state.clone();
        window.on_template_delete_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                delete_template(&ui, &mut state.borrow_mut());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{init_backend, meeting_at, test_settings};
    use super::super::update_meeting_view;
    use super::*;
    use noteiq_core::{model::CaptureSource, settings::Settings};

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-11-04T14:15:00+01:00").unwrap()
    }

    /// A window whose settings file lives in `folder`, so templates are stored beside it.
    fn window_in(folder: &std::path::Path) -> (MainWindow, AppState) {
        init_backend();
        let window = MainWindow::new().unwrap();
        let mut settings = test_settings();
        settings.auto_connect_meeting = false;
        let state = AppState::new(settings, folder.join("settings.toml"), CaptureSource::Cli);
        reset_view(&window, &state);
        (window, state)
    }

    fn names(window: &MainWindow) -> Vec<String> {
        window
            .get_template_names()
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    fn connect(window: &MainWindow, state: &mut AppState) {
        state
            .capture
            .connect_meeting(meeting_at("review", now(), -5, 25), Instant::now());
        update_meeting_view(window, state, now());
    }

    #[test]
    fn templates_stay_off_until_enabled_then_fill_an_empty_meeting_note() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = window_in(folder.path());
        assert!(!window.get_templates_enabled());
        assert_eq!(
            names(&window),
            ["General meeting", "1:1", "Customer call", "Stand-up"]
        );

        connect(&window, &mut state);
        assert_eq!(
            window.get_note_text(),
            "",
            "disabled templates change nothing"
        );

        set_templates_enabled(&window, &mut state, true, now());
        assert!(window.get_note_text().starts_with("### Agenda"));
        assert!(state.capture.capture.is_empty());
        let saved = Settings::load(&folder.path().join("settings.toml"), folder.path()).unwrap();
        assert!(saved.meeting_templates_enabled);

        state.capture.disconnect_meeting(Instant::now());
        update_meeting_view(&window, &mut state, now());
        assert_eq!(
            window.get_note_text(),
            "",
            "an untouched template leaves with its meeting"
        );
    }

    #[test]
    fn a_user_template_can_be_created_renamed_made_default_and_deleted() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = window_in(folder.path());
        edit_template(&window, &state, 3);
        assert_eq!(window.get_template_name_text(), "Stand-up");
        assert!(!window.get_template_editable(), "built-ins are read-only");

        new_template(&window, true);
        assert_eq!(window.get_template_name_text(), "Stand-up copy");
        save_template(&window, &mut state, "Daily", "### Done {sync}");
        assert!(window.get_template_message_error());
        assert!(
            window.get_template_is_new(),
            "a rejected template stays editable"
        );

        save_template(&window, &mut state, "Daily", "### Done\n\n- {title}");
        assert!(!window.get_template_message_error());
        assert!(folder.path().join("templates").join("Daily.md").exists());
        assert_eq!(names(&window).last().map(String::as_str), Some("Daily"));
        let daily = window.get_edit_template_index();
        choose_default_template(&window, &mut state, daily);
        assert_eq!(state.settings.default_meeting_template, "Daily");

        save_template(&window, &mut state, "Daily sync", "### Done\n\n- {title}");
        assert_eq!(state.settings.default_meeting_template, "Daily sync");
        assert_eq!(
            names(&window).last().map(String::as_str),
            Some("Daily sync")
        );

        state.settings.meeting_templates_enabled = true;
        connect(&window, &mut state);
        assert_eq!(window.get_note_text(), "### Done\n\n- review");

        delete_template(&window, &mut state);
        assert_eq!(state.settings.default_meeting_template, DEFAULT_TEMPLATE_ID);
        assert_eq!(names(&window).len(), 4);
        assert_eq!(window.get_template_name_text(), "General meeting");
    }

    #[test]
    fn inserting_a_template_into_a_note_with_text_adds_it_below() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = window_in(folder.path());
        state.capture.edit("My own notes", Instant::now());
        window.set_note_text("My own notes".into());

        insert_note_template(&window, &mut state, 3, now());
        let text = window.get_note_text().to_string();
        assert!(text.starts_with("My own notes\n\n### Yesterday"), "{text}");
        assert_eq!(state.capture.capture.body, text);
    }

    #[test]
    fn save_and_delete_act_on_the_template_shown_after_the_folder_changes() {
        let folder = tempfile::tempdir().unwrap();
        let (window, mut state) = window_in(folder.path());
        let templates = store(&state);
        templates.create("Beta", "### Beta").unwrap();
        reset_view(&window, &state);
        let beta = i32::try_from(names(&window).len() - 1).unwrap();

        // Another app adds a template that sorts before Beta while Settings is open.
        templates.create("Alpha", "### Alpha").unwrap();
        edit_template(&window, &state, beta);
        assert_eq!(window.get_template_name_text(), "Beta");
        assert_eq!(names(&window).last().map(String::as_str), Some("Beta"));

        templates.create("Aardvark", "### Aardvark").unwrap();
        delete_template(&window, &mut state);
        let left: Vec<_> = templates
            .user_templates()
            .unwrap()
            .into_iter()
            .map(|template| template.name)
            .collect();
        assert_eq!(left, ["Aardvark", "Alpha"], "only Beta is deleted");

        templates.create("Gamma", "### Gamma").unwrap();
        reset_view(&window, &state);
        let gamma = i32::try_from(names(&window).len() - 1).unwrap();
        templates.create("Able", "### Able").unwrap();
        edit_template(&window, &state, gamma);
        assert_eq!(window.get_template_name_text(), "Gamma");
        templates.create("Abacus", "### Abacus").unwrap();
        save_template(&window, &mut state, "Gamma", "### Gamma edited");
        assert_eq!(
            templates.get("Gamma").unwrap().unwrap().body,
            "### Gamma edited"
        );
        assert_eq!(templates.get("Abacus").unwrap().unwrap().body, "### Abacus");
    }
}
