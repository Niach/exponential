// EXP-273: make cargo watch the icon assets.
//
// `icons.rs` expands `icon_named!(ExpIcon, "../../assets/icons")`, which reads
// that directory AT MACRO EXPANSION TIME to derive one enum variant per SVG.
// Cargo has no idea the macro touched those files, so without this script a
// newly generated icon does not produce a variant until something else forces
// the crate to rebuild — the symptom is a "no variant named X" error (or a
// stale glyph) that a `cargo clean -p ui` mysteriously fixes.
//
// Directory-level `rerun-if-changed` covers adds, removes and edits: cargo
// stats the directory tree, so `bun run --filter @exp/icons generate` writing
// a new SVG is enough to trigger re-expansion.
fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons");
}
