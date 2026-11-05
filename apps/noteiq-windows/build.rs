fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Debug builds carry element metadata so the headless UI tests can find controls by
    // accessible label; release builds stay lean.
    let debug_info = std::env::var("PROFILE").as_deref() == Ok("debug");
    let config = slint_build::CompilerConfiguration::new()
        .with_debug_info(debug_info)
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/main.slint", config).expect("compile the NoteIQ window");
    compile_icon();
}

fn compile_icon() {
    println!("cargo:rerun-if-changed=assets/noteiq.svg");
    let svg = std::fs::read("assets/noteiq.svg").expect("read the NoteIQ icon");
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default())
        .expect("parse the NoteIQ icon");
    let sizes = [16_u32, 20, 24, 32, 40, 48, 64, 128, 256];
    let frames = sizes.map(|size| {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("allocate icon pixels");
        let scale = size as f32 / tree.size().width();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        pixmap.encode_png().expect("encode the NoteIQ icon")
    });

    // Windows ICO stores a directory followed by PNG frames; 256px is encoded as size zero.
    let mut icon = Vec::new();
    icon.extend_from_slice(&[0, 0, 1, 0]);
    icon.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (size, frame) in sizes.iter().zip(&frames) {
        let dimension = if *size == 256 { 0 } else { *size as u8 };
        let length = u32::try_from(frame.len()).expect("icon frame fits in a DWORD");
        icon.extend_from_slice(&[dimension, dimension, 0, 0]);
        icon.extend_from_slice(&1_u16.to_le_bytes());
        icon.extend_from_slice(&32_u16.to_le_bytes());
        icon.extend_from_slice(&length.to_le_bytes());
        icon.extend_from_slice(&offset.to_le_bytes());
        offset += length;
    }
    for frame in frames {
        icon.extend_from_slice(&frame);
    }

    let path =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo output directory"))
            .join("noteiq.ico");
    std::fs::write(&path, icon).expect("write the NoteIQ icon");
    winresource::WindowsResource::new()
        .set_icon(path.to_str().expect("icon path is UTF-8"))
        .set("ProductName", "NoteIQ")
        .set("FileDescription", "NoteIQ quick capture")
        .set("OriginalFilename", "noteiq.exe")
        .compile()
        .expect("embed the NoteIQ Windows icon and version information");
}
