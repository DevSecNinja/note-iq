use rfd::FileDialog;
use std::path::{Path, PathBuf};

pub fn choose_destination(current: &Path) -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Choose your NoteIQ notes folder")
        .set_directory(current)
        .pick_folder()
}
