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

    pub fn poll(&self) -> Option<TrayAction> {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            return if event.id == self.show_id {
                Some(TrayAction::Show)
            } else if event.id == self.new_id {
                Some(TrayAction::New)
            } else if event.id == self.quit_id {
                Some(TrayAction::Quit)
            } else {
                None
            };
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if event.id() == self._icon.id() && activates_capture(&event) {
                return Some(TrayAction::Show);
            }
        }
        None
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
