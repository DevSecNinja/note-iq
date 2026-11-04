use std::path::Path;

pub fn open_note(path: &Path) -> Result<(), String> {
    opener::open(path).map_err(|error| {
        let _ = error;
        "Windows could not open this note with the registered Markdown app.".to_owned()
    })
}
