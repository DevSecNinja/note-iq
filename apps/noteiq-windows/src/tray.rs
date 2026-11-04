use tray_icon::{
    TrayIcon, TrayIconBuilder,
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
    pub fn new() -> Result<Self, String> {
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
            .with_icon(
                tray_icon::Icon::from_rgba([79, 70, 229, 255].repeat(16 * 16), 16, 16)
                    .map_err(|_| "The NoteIQ tray icon could not be created.")?,
            )
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
        tray_icon::TrayIconEvent::receiver()
            .try_recv()
            .ok()
            .map(|_| TrayAction::Show)
    }
}
