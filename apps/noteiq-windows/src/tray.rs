use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuId, MenuItem},
};

pub struct TrayController {
    _icon: TrayIcon,
    show_id: MenuId,
    new_id: MenuId,
    quit_id: MenuId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayAction {
    Show,
    New,
    Quit,
}

impl TrayController {
    pub fn new(image: &slint::Image) -> Result<Self, String> {
        let pixels = image
            .to_rgba8()
            .ok_or("The NoteIQ tray icon could not be decoded.")?;
        let tray_image =
            tray_icon::Icon::from_rgba(pixels.as_bytes().to_vec(), pixels.width(), pixels.height())
                .map_err(|_| "The NoteIQ tray icon could not be created.")?;
        let menu = Menu::new();
        let show = MenuItem::new("Show NoteIQ", true, None);
        let new = MenuItem::new("New note", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append(&show)
            .and_then(|()| menu.append(&new))
            .and_then(|()| menu.append(&quit))
            .map_err(|_| "The NoteIQ tray menu could not be created.")?;
        let icon = TrayIconBuilder::new()
            .with_tooltip("NoteIQ quick capture")
            .with_icon(tray_image)
            .with_menu(Box::new(menu))
            .build()
            .map_err(|_| "The NoteIQ tray icon could not be created.")?;
        Ok(Self {
            _icon: icon,
            show_id: show.id().clone(),
            new_id: new.id().clone(),
            quit_id: quit.id().clone(),
        })
    }

    pub fn bind(&self, window: slint::Weak<crate::ui_generated::MainWindow>) {
        let show_id = self.show_id.clone();
        let new_id = self.new_id.clone();
        let quit_id = self.quit_id.clone();
        let menu_window = window.clone();
        // Native tray windows do not wake winit through the receiver queues.
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(action) = menu_action(&event.id, &show_id, &new_id, &quit_id) {
                dispatch_action(&menu_window, action);
            }
        }));
        let icon_id = self._icon.id().clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if event.id() == &icon_id && activates_capture(&event) {
                dispatch_action(&window, TrayAction::Show);
            }
        }));
    }
}

fn menu_action(id: &MenuId, show: &MenuId, new: &MenuId, quit: &MenuId) -> Option<TrayAction> {
    if id == show {
        Some(TrayAction::Show)
    } else if id == new {
        Some(TrayAction::New)
    } else if id == quit {
        Some(TrayAction::Quit)
    } else {
        None
    }
}

fn dispatch_action(window: &slint::Weak<crate::ui_generated::MainWindow>, action: TrayAction) {
    if let Err(error) = window.upgrade_in_event_loop(move |window| {
        window.invoke_tray_action_requested(match action {
            TrayAction::Show => 0,
            TrayAction::New => 1,
            TrayAction::Quit => 2,
        });
    }) {
        tracing::error!(%error, "Could not deliver the tray action");
    }
}

fn activates_capture(event: &TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_delivery_wakes_the_event_loop_with_a_hidden_window() {
        use slint::ComponentHandle;
        use std::{cell::Cell, rc::Rc, time::Duration};

        i_slint_backend_testing::init_integration_test_with_system_time();
        let window = crate::ui_generated::MainWindow::new().unwrap();
        let delivered = Rc::new(Cell::new(false));
        let received = delivered.clone();
        window.on_tray_action_requested(move |action| {
            assert_eq!(action, 2);
            received.set(true);
            slint::quit_event_loop().unwrap();
        });
        let timeout = slint::Timer::default();
        timeout.start(slint::TimerMode::SingleShot, Duration::from_secs(5), || {
            slint::quit_event_loop().unwrap();
        });
        let weak = window.as_weak();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            dispatch_action(&weak, TrayAction::Quit);
        });
        slint::run_event_loop_until_quit().unwrap();
        worker.join().unwrap();
        assert!(delivered.get(), "Quit must not depend on timer polling");
    }

    #[test]
    fn menu_actions_are_routed_by_id_including_quit() {
        let show = MenuId::new("show");
        let new = MenuId::new("new");
        let quit = MenuId::new("quit");
        for (id, expected) in [
            (&show, Some(TrayAction::Show)),
            (&new, Some(TrayAction::New)),
            (&quit, Some(TrayAction::Quit)),
            (&MenuId::new("unrelated"), None),
        ] {
            assert_eq!(menu_action(id, &show, &new, &quit), expected);
        }
    }

    fn click(button: MouseButton, button_state: MouseButtonState) -> TrayIconEvent {
        TrayIconEvent::Click {
            id: "test".into(),
            position: Default::default(),
            rect: Default::default(),
            button,
            button_state,
        }
    }

    #[test]
    fn only_a_released_left_click_opens_the_capture() {
        assert!(activates_capture(&click(
            MouseButton::Left,
            MouseButtonState::Up
        )));
        for event in [
            click(MouseButton::Left, MouseButtonState::Down),
            click(MouseButton::Right, MouseButtonState::Up),
            click(MouseButton::Middle, MouseButtonState::Up),
            TrayIconEvent::Enter {
                id: "test".into(),
                position: Default::default(),
                rect: Default::default(),
            },
            TrayIconEvent::Move {
                id: "test".into(),
                position: Default::default(),
                rect: Default::default(),
            },
            TrayIconEvent::Leave {
                id: "test".into(),
                position: Default::default(),
                rect: Default::default(),
            },
        ] {
            assert!(!activates_capture(&event), "{event:?}");
        }
    }
}
