//! Create-issue dialog (masterplan-v3 §4.2 — layout matches
//! `apps/web/src/components/create-issue-dialog.tsx` +
//! `issue-editor/dialog-shell.tsx` field-for-field).
//!
//! EXP-771: this file is only the WINDOW HOST now — the form, the create
//! pipeline and every field in it are [`crate::issue_composer::IssueComposer`]
//! in its `Dialog` presentation, shared byte-for-byte with the inline
//! sub-issue card. What lives here is what the dialog alone owns: the
//! [`NewIssue`] action, the window spec (EXP-288 compact-open + grow cap), and
//! the titlebar strip's board breadcrumb/select (EXP-287/EXP-449).

use gpui::{
    div, px, size, AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement,
    SharedString, Styled, Window,
};
use gpui_component::{h_flex, menu::DropdownMenu as _, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use crate::actions::NewIssue;
use crate::icons::registry;
use crate::issue_composer::IssueComposer;
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::navigation::{active_board_id, nav_for_window};
use crate::pickers::chip_button;

/// Register the App-global [`NewIssue`] handler (call once from `ui::init`).
/// The action is the §3.6 unit action the filter bar dispatches; the target
/// board is the window's active board (the top-bar picker scope — the
/// All Issues tool window's list).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &NewIssue, cx| {
        crate::navigation::on_active_window(cx, |window, cx| {
            let nav = nav_for_window(window, cx);
            let Some(board_id) = active_board_id(&nav, cx) else {
                return; // no board in scope — nothing to create into
            };
            open(window, cx, board_id);
        });
    });
}

/// Open the dialog. Resolves the board row (prefix, color, team) off
/// the synced collections; a no-op when the board is unknown (racing a
/// delete).
pub fn open(window: &mut Window, cx: &mut App, board_id: String) {
    let collections = Store::global(cx).collections();
    let Some(board) = collections.boards.read(cx).get(&board_id).cloned() else {
        log::warn!("[ui] NewIssue for unknown board {board_id}");
        return;
    };

    // Web: sm:max-w-[40rem] p-0 max-h-[85vh]. EXP-288: the dialog OPENS
    // compact (~3 description rows) and GROWS with the content up to the
    // pre-EXP-288 height (460 / 85% viewport — computed from the OPENER's
    // viewport now, the dialog can't read it later); still user-resizable
    // with a floor matching the compact start. Past the cap the editor
    // region scrolls with caret-follow, header/chips/footer pinned.
    let max_height = (window.viewport_size().height * 0.85).min(px(460.));
    let height = px(300.).min(max_height);
    let spec = DialogSpec::new("New issue", size(px(640.), height))
        .resizable(size(px(560.), px(300.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let team_id = board.team_id.clone();
        let view = cx.new(|cx| {
            IssueComposer::dialog(board.id.clone(), board.team_id.clone(), max_height, window, cx)
        });
        let busy = view.clone();
        let submit = view.clone();
        // WEAK on purpose: the title closure is an `Rc` the shell holds for
        // the window's whole life, so a strong handle would keep the content
        // view alive past the dialog's close.
        let title_view = view.downgrade();
        DialogContent::new(view.clone())
            .padless()
            // EXP-287: the board-pill breadcrumb rides the window's titlebar
            // strip (the shell owns the chrome now — see `native_dialog`).
            .title_content(move |_, cx| {
                let Some(view) = title_view.upgrade() else {
                    return "New issue".into_any_element();
                };
                let board_id = view.read(cx).board_id().to_string();
                let board = Store::global(cx)
                    .collections()
                    .boards
                    .read(cx)
                    .get(&board_id)
                    .cloned();
                match board {
                    Some(board) => title_board_select(&view, &team_id, &board, cx),
                    None => "New issue".into_any_element(),
                }
            })
            // EXP-449: the pill is a live board SELECT now — without this the
            // shell would never repaint it after a pick.
            .title_follows(&view)
            .can_close(move |cx| !busy.read(cx).submitting())
            // Enter anywhere in the dialog submits (web form submit).
            .on_enter(move |window, cx| {
                submit.update(cx, |view, cx| view.submit(window, cx));
            })
    });
}

/// EXP-287: the titlebar label — board chip · › · "New issue". Lives in the
/// window's `TitleBar` strip now that the shell owns the dialog chrome.
///
/// EXP-449: the chip is a live board SELECT — the board's own glyph tinted
/// with its color (the EXP-282 treatment that replaced the anonymous color
/// dot everywhere else) plus its prefix, over the team's boards. A
/// single-board team keeps a static glass chip. Nothing else in the dialog
/// resets on a pick: status/assignee/label options are team-scoped, the menu
/// only offers same-team boards, and `submit` reads `board_id` live.
fn title_board_select(
    view: &Entity<IssueComposer>,
    team_id: &str,
    board: &domain::rows::Board,
    cx: &App,
) -> AnyElement {
    let tint = board
        .color
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or(cx.theme().muted_foreground);
    let icon = crate::icons::board_icon(board).xsmall().text_color(tint);
    let prefix = SharedString::from(board.prefix.clone().unwrap_or_default());

    // Same "is there anywhere else to go" rule the issue header's Board chip
    // uses (EXP-57 `move_target_boards`): a single-board team gets a static
    // chip instead of a one-entry menu.
    let chip: AnyElement = if crate::issue_list::move_target_boards(cx, &board.id).is_empty() {
        crate::surface::glass_pill(
            "create-board-chip",
            crate::surface::PillSize::Sm,
            crate::surface::PillMode::Readonly,
            cx,
        )
        .child(icon)
        .child(crate::pickers::chip_label(prefix, false, cx))
        .into_any_element()
    } else {
        let current_id = board.id.clone();
        let team_id = team_id.to_string();
        let view = view.clone();
        chip_button("create-board-chip", cx)
            .icon(icon)
            .child(crate::pickers::chip_label(prefix, false, cx))
            .child(
                Icon::new(registry::UI_CHEVRON_DOWN)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            // A plain dropdown menu, not the searchable `board_picker_popover`
            // — that one needs a host-owned `Entity<InputState>` for its query,
            // which this `Fn` title closure has nowhere to keep.
            .dropdown_menu(move |mut menu, _window, cx| {
                menu = menu.check_side(gpui_component::Side::Right);
                for board in Store::global(cx).collections().boards_in_team(&team_id, cx) {
                    let is_current = board.id == current_id;
                    let tint = board
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(gpui::opaque_grey(0.5, 1.0));
                    let icon = crate::icons::board_icon(&board).xsmall().text_color(tint);
                    let name = SharedString::from(board.name.clone());
                    let picked = board.id.clone();
                    let view = view.clone();
                    menu = menu.item(
                        gpui_component::menu::PopupMenuItem::element(move |_, cx| {
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(icon.clone())
                                .child(
                                    div()
                                        .text_color(cx.theme().popover_foreground)
                                        .child(name.clone()),
                                )
                        })
                        .checked(is_current)
                        .disabled(is_current)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.set_board_id(picked.clone(), cx);
                            });
                        }),
                    );
                }
                menu
            })
            .into_any_element()
    };

    h_flex()
        .gap_1p5()
        .items_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(chip)
        .child(Icon::new(registry::UI_CHEVRON_RIGHT).xsmall())
        .child("New issue")
        .into_any_element()
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn rgb_hsla(r: u8, g: u8, b: u8) -> gpui::Hsla {
    gpui::Rgba {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        a: 1.0,
    }
    .into()
}

/// `#rrggbb` → Hsla (board/label colors are hex strings).
pub(crate) fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(rgb_hsla(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_parse_and_reject_bad_input() {
        assert!(parse_hex_color("#6366f1").is_some());
        assert!(parse_hex_color("6366f1").is_none());
        assert!(parse_hex_color("#66f1").is_none());
        assert!(parse_hex_color("#zzzzzz").is_none());
    }
}
