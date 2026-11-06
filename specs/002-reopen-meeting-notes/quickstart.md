# Validation

Run `cargo test --locked -p noteiq-core --test saved_note_tests`.
On native Windows ARM64, initialize VS ARM64 build tools and run:

```text
cargo test --locked -p noteiq-windows --features desktop --lib --test ui_acceptance
```

For the complete native gate, use the existing target cache:

```text
cargo test --locked -p noteiq-core -p noteiq-windows --features noteiq-windows/desktop --target aarch64-pc-windows-msvc --target-dir target\windows-native
cargo clippy --locked -p noteiq-windows --features desktop --all-targets --target aarch64-pc-windows-msvc --target-dir target\windows-native -- -D warnings
cargo build --locked -p noteiq-windows --features desktop --release --target aarch64-pc-windows-msvc --target-dir target\windows-native
cargo fmt --all -- --check
dprint check --allow-no-files
git diff --check
```

Use a disposable destination. Save two captures for distinct occurrences with
the same event identity. Start a draft, select each occurrence, and verify its
original file/text opens and the previous draft remains saved. Add Summary and
custom properties externally, reopen and append, then verify preservation.
Externally modify a reopened file again and verify saving reports a conflict.
Select No meeting and verify persisted meeting properties disappear.
From a saved meeting note, select a different meeting and verify the
confirmation keeps the same note when changing its meeting, saves the old note
and opens a blank capture when creating a new note, and leaves the original
selection and text unchanged when cancelled. Make the save fail before choosing
new note and verify the current note remains open.

No live WorkIQ account or actual vault data is required for automated tests.
