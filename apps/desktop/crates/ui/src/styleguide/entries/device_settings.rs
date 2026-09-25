//! EXP-1020 — `device-settings` (3 Special components): the per-machine
//! settings surface, ONE layout on all four clients.
//!
//! The IDE's implementation is `crate::device_settings` (the dialog the
//! machines row's gear opens) and `settings::agents` (the same agent-defaults
//! card for THIS install). Worktrees are not here: a machine's worktrees are
//! a local surface, Settings → Worktrees, which is also the only place that
//! cleans them.
//!
//! WHY THIS DESCRIBES INSTEAD OF RENDERING: the IDE entry signature is
//! `fn() -> Div` with no `&App` (`entries/mod.rs`, EXP-1029's contract), and
//! every glass recipe this component is built from takes one for the theme.
//! A hand-rolled unthemed lookalike would be worse than a description — the
//! styleguide's own rule is that a lookalike can disagree with the product
//! silently. The web entry renders the REAL component as an island; giving
//! the IDE entries an `&App` is EXP-1063.

use gpui::{div, Div, ParentElement as _};

pub(crate) const ID: &str = "device-settings";
pub(crate) const OWNER: &str = "EXP-1020";

/// The agreed row order. `render` takes no `App`, so this entry names the
/// order rather than painting it; the live surface is the gear on any row of
/// Settings → Devices.
pub(crate) fn render() -> Div {
    div()
        .child("Device settings — one layout ×4:")
        .child("1. identity row (icon + name)")
        .child("2. Default device")
        .child("3. Sharing (server machines, headline)")
        .child("4. agent defaults, NO headline: Default account (always changeable),")
        .child("   agent tabs, Model, Subagent model (claude only), Effort,")
        .child("   Ultracode, Plan mode, then the Workflow settings sub-shell row")
        .child("5. Update (headline)")
        .child("6. Remove device — a plain row, no headline of its own")
        .child("A headline appears only where something else shares the page.")
        .child("IDE: ui::device_settings + ui::settings::agents")
}
