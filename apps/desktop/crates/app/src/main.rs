//! `exp-desktop` — the Exponential desktop IDE binary (masterplan-v3 §3.1).
//!
//! Phase-0 stub. Phase 1 lands the gpui bootstrap (§3.6):
//! `gpui::Application::new().with_assets(…).run(…)` → `gpui_component::init` →
//! forced Exponential Dark theme → keymap/menubar → global `Store` →
//! `open_main_window` (Root + Workspace/DockArea). Thin — it wires, it does
//! not implement.

fn main() {}
