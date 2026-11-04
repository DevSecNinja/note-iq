fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Debug builds carry element metadata so the headless UI tests can find controls by
    // accessible label; release builds stay lean.
    let debug_info = std::env::var("PROFILE").as_deref() == Ok("debug");
    let config = slint_build::CompilerConfiguration::new().with_debug_info(debug_info);
    slint_build::compile_with_config("ui/main.slint", config).expect("compile the NoteIQ window");
}
