// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! `to_esc_str` — gpui [`Keystroke`] → terminal escape bytes (masterplan-v3
//! §6.8).
//!
//! PROVENANCE (the sharpest licensing edge in the crate, §6.8): this table is
//! written from the **ECMA-48 / xterm control-sequence documentation**
//! ("Xterm Control Sequences", ctlseqs — PC-Style Function Keys) and
//! alacritty's **published** default bindings documentation (Apache-2.0).
//! Zed's GPL `crates/terminal/src/mappings/keys.rs` was **not** read while
//! writing this file and no line of it enters our tree.
//!
//! The encoding rules implemented here, from the xterm reference:
//!
//! - Modifier parameter: `p = 1 + (shift·1 + alt·2 + ctrl·4)`; a modified
//!   editing key sends `CSI <n> ; <p> ~`, a modified cursor/home/end key
//!   sends `CSI 1 ; <p> <final>`, and modified F1–F4 leave SS3 for
//!   `CSI 1 ; <p> {P,Q,R,S}`.
//! - DECCKM (`TermMode::APP_CURSOR`): unmodified cursor keys + Home/End flip
//!   `CSI <final>` → `SS3 <final>` (`\x1bO…`). Modified keys always use the
//!   CSI form regardless of DECCKM.
//! - C0 controls: Ctrl+letter → byte `letter & 0x1f`; the `@ [ \ ] ^ _ ?`
//!   and digit variants per the xterm C0 table.
//! - Alt-as-Meta prefixes `ESC` to the base bytes (`alt_is_meta`; on macOS
//!   Option composes characters instead when disabled).
//!
//! `TermMode::APP_KEYPAD` (DECKPAM) affects only the *numeric keypad* keys,
//! which gpui does not distinguish from the main row at this rev — so no
//! keypad table is emitted (documented gap, same visible behavior as a
//! terminal with the keypad in numeric mode).

use alacritty_terminal::term::TermMode;
use gpui::Keystroke;
use std::borrow::Cow;

/// Translate one keystroke into the bytes to write to the PTY.
///
/// Returns `None` when the key is not the terminal's to handle (e.g. any
/// `cmd`/`super`-modified chord — those belong to app keybindings), when it
/// produces no bytes (bare modifier, unknown named key), or for plain/shift
/// printables — those must stay unhandled so the platform text-input path
/// (IME) delivers them via `replace_text_in_range` (see the note at the end
/// of this function).
pub fn to_esc_str(
    keystroke: &Keystroke,
    mode: &TermMode,
    alt_is_meta: bool,
) -> Option<Cow<'static, [u8]>> {
    let mods = keystroke.modifiers;

    // cmd/super chords are application shortcuts, never terminal input.
    if mods.platform {
        return None;
    }

    let modifier_param = modifier_param(mods.shift, mods.alt, mods.control);
    let modified = modifier_param > 1;
    let app_cursor = mode.contains(TermMode::APP_CURSOR);

    // -- Named keys ---------------------------------------------------------
    match keystroke.key.as_str() {
        "enter" => return Some(meta_prefixed(b"\r", mods.alt && alt_is_meta)),
        "escape" => return Some(meta_prefixed(b"\x1b", mods.alt && alt_is_meta)),
        "tab" => {
            return Some(if mods.shift {
                // Back-tab (CBT report form): CSI Z.
                Cow::Borrowed(&b"\x1b[Z"[..])
            } else {
                meta_prefixed(b"\t", mods.alt && alt_is_meta)
            });
        }
        "backspace" => {
            // DEL by default (xterm/alacritty default); Ctrl flips to BS.
            let base: &[u8] = if mods.control { b"\x08" } else { b"\x7f" };
            return Some(meta_prefixed(base, mods.alt && alt_is_meta));
        }
        "up" => return Some(cursor_key(b'A', app_cursor, modified, modifier_param)),
        "down" => return Some(cursor_key(b'B', app_cursor, modified, modifier_param)),
        "right" => return Some(cursor_key(b'C', app_cursor, modified, modifier_param)),
        "left" => return Some(cursor_key(b'D', app_cursor, modified, modifier_param)),
        "home" => return Some(cursor_key(b'H', app_cursor, modified, modifier_param)),
        "end" => return Some(cursor_key(b'F', app_cursor, modified, modifier_param)),
        "insert" => return Some(editing_key(2, modified, modifier_param)),
        "delete" => return Some(editing_key(3, modified, modifier_param)),
        "pageup" => return Some(editing_key(5, modified, modifier_param)),
        "pagedown" => return Some(editing_key(6, modified, modifier_param)),
        "space" => {
            let base: &[u8] = if mods.control { b"\x00" } else { b" " };
            return Some(meta_prefixed(base, mods.alt && alt_is_meta));
        }
        key if key.len() > 1 && key.starts_with('f') => {
            if let Ok(n) = key[1..].parse::<u8>() {
                if (1..=20).contains(&n) {
                    return Some(function_key(n, modified, modifier_param));
                }
            }
        }
        _ => {}
    }

    // -- Ctrl + character → C0 control byte ---------------------------------
    if mods.control {
        if let Some(byte) = ctrl_byte(keystroke.key.as_str()) {
            let bytes = if mods.alt && alt_is_meta {
                vec![0x1b, byte]
            } else {
                vec![byte]
            };
            return Some(Cow::Owned(bytes));
        }
        return None;
    }

    // -- Alt/Option as Meta: ESC-prefix the (possibly shifted) base char ----
    if mods.alt && alt_is_meta {
        let ch = keystroke
            .key_char
            .clone()
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| keystroke.key.clone());
        if !ch.is_empty() && ch.chars().count() <= 2 {
            let mut bytes = Vec::with_capacity(1 + ch.len());
            bytes.push(0x1b);
            bytes.extend_from_slice(ch.as_bytes());
            return Some(Cow::Owned(bytes));
        }
        return None;
    }

    // -- Plain/shift printable input is deliberately NOT handled here -------
    // It reaches the PTY through the platform text-input path instead
    // (`InputHandler::replace_text_in_range` → `TerminalView::ime_commit`).
    // Consuming it in the key table would mark the key event handled, and a
    // handled key never reaches the IME — CJK composition (and macOS
    // dead-key/Option composition) would go dead in the terminal. Returning
    // `None` leaves the event unhandled, and every gpui platform then
    // delivers the text exactly once: macOS `handle_key_event` falls through
    // to `inputContext handleEvent:` (→ `insertText:`), Wayland/X11
    // `handle_input` call `replace_text_in_range` for unhandled printables.
    None
}

/// xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4).
fn modifier_param(shift: bool, alt: bool, ctrl: bool) -> u8 {
    1 + (shift as u8) + ((alt as u8) << 1) + ((ctrl as u8) << 2)
}

/// Cursor-style key (arrows, Home, End): CSI/SS3 per DECCKM, forced-CSI with
/// a modifier parameter when modified.
fn cursor_key(final_byte: u8, app_cursor: bool, modified: bool, param: u8) -> Cow<'static, [u8]> {
    if modified {
        Cow::Owned(format!("\x1b[1;{}{}", param, final_byte as char).into_bytes())
    } else if app_cursor {
        Cow::Owned(vec![0x1b, b'O', final_byte])
    } else {
        Cow::Owned(vec![0x1b, b'[', final_byte])
    }
}

/// Editing key (Insert/Delete/PgUp/PgDn + tilde-coded F5–F20):
/// `CSI <n> ~` / `CSI <n> ; <p> ~`.
fn editing_key(n: u8, modified: bool, param: u8) -> Cow<'static, [u8]> {
    if modified {
        Cow::Owned(format!("\x1b[{n};{param}~").into_bytes())
    } else {
        Cow::Owned(format!("\x1b[{n}~").into_bytes())
    }
}

/// F1–F20 per the xterm PC-style function-key table.
fn function_key(n: u8, modified: bool, param: u8) -> Cow<'static, [u8]> {
    match n {
        // F1–F4: SS3 P/Q/R/S, or CSI 1;p P/Q/R/S when modified.
        1..=4 => {
            let final_byte = b'P' + (n - 1);
            if modified {
                Cow::Owned(format!("\x1b[1;{}{}", param, final_byte as char).into_bytes())
            } else {
                Cow::Owned(vec![0x1b, b'O', final_byte])
            }
        }
        // F5–F20: tilde codes with the historical gaps (no 16/22/27/30).
        _ => {
            let code = match n {
                5 => 15,
                6 => 17,
                7 => 18,
                8 => 19,
                9 => 20,
                10 => 21,
                11 => 23,
                12 => 24,
                13 => 25,
                14 => 26,
                15 => 28,
                16 => 29,
                17 => 31,
                18 => 32,
                19 => 33,
                _ => 34, // F20
            };
            editing_key(code, modified, param)
        }
    }
}

/// The xterm C0 table for Ctrl+key.
fn ctrl_byte(key: &str) -> Option<u8> {
    let mut chars = key.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None; // multi-char named key — not a C0 candidate
    }
    Some(match ch {
        'a'..='z' => (ch as u8) & 0x1f,
        '@' | '2' => 0x00,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '7' | '/' | '-' => 0x1f,
        '?' | '8' => 0x7f,
        _ => return None,
    })
}

/// ESC-prefix `base` when Alt is acting as Meta.
fn meta_prefixed(base: &'static [u8], meta: bool) -> Cow<'static, [u8]> {
    if meta {
        let mut bytes = Vec::with_capacity(1 + base.len());
        bytes.push(0x1b);
        bytes.extend_from_slice(base);
        Cow::Owned(bytes)
    } else {
        Cow::Borrowed(base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn ks(key: &str, key_char: Option<&str>, mods: Modifiers) -> Keystroke {
        Keystroke {
            modifiers: mods,
            key: key.into(),
            key_char: key_char.map(str::to_owned),
        }
    }

    fn none() -> Modifiers {
        Modifiers::default()
    }
    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Default::default()
        }
    }
    fn ctrl() -> Modifiers {
        Modifiers {
            control: true,
            ..Default::default()
        }
    }
    fn alt() -> Modifiers {
        Modifiers {
            alt: true,
            ..Default::default()
        }
    }
    fn ctrl_shift() -> Modifiers {
        Modifiers {
            control: true,
            shift: true,
            ..Default::default()
        }
    }
    fn cmd() -> Modifiers {
        Modifiers {
            platform: true,
            ..Default::default()
        }
    }

    fn bytes(key: &str, key_char: Option<&str>, mods: Modifiers, mode: TermMode) -> Option<Vec<u8>> {
        to_esc_str(&ks(key, key_char, mods), &mode, true).map(|c| c.to_vec())
    }

    /// One xterm-reference row: (key, key_char, mods, mode, expected bytes).
    type MatrixCase<'a> = (&'a str, Option<&'a str>, Modifiers, TermMode, &'a [u8]);

    /// §6.8's fixed matrix: (key, mode) → bytes against the xterm reference.
    #[test]
    fn xterm_reference_matrix() {
        let cases: &[MatrixCase] = &[
            // C0 controls
            ("a", None, ctrl(), TermMode::NONE, b"\x01"),
            ("z", None, ctrl(), TermMode::NONE, b"\x1a"),
            ("c", None, ctrl(), TermMode::NONE, b"\x03"),
            ("space", None, ctrl(), TermMode::NONE, b"\x00"),
            ("[", None, ctrl(), TermMode::NONE, b"\x1b"),
            ("a", None, ctrl_shift(), TermMode::NONE, b"\x01"),
            // basics
            ("enter", Some("\n"), none(), TermMode::NONE, b"\r"),
            ("escape", None, none(), TermMode::NONE, b"\x1b"),
            ("tab", Some("\t"), none(), TermMode::NONE, b"\t"),
            ("tab", None, shift(), TermMode::NONE, b"\x1b[Z"),
            ("backspace", None, none(), TermMode::NONE, b"\x7f"),
            ("backspace", None, ctrl(), TermMode::NONE, b"\x08"),
            ("space", Some(" "), none(), TermMode::NONE, b" "),
            // arrows: normal vs application cursor (DECCKM)
            ("up", None, none(), TermMode::NONE, b"\x1b[A"),
            ("down", None, none(), TermMode::NONE, b"\x1b[B"),
            ("right", None, none(), TermMode::NONE, b"\x1b[C"),
            ("left", None, none(), TermMode::NONE, b"\x1b[D"),
            ("up", None, none(), TermMode::APP_CURSOR, b"\x1bOA"),
            ("left", None, none(), TermMode::APP_CURSOR, b"\x1bOD"),
            // modified arrows: CSI 1;p — even in app-cursor mode
            ("up", None, shift(), TermMode::NONE, b"\x1b[1;2A"),
            ("left", None, alt(), TermMode::NONE, b"\x1b[1;3D"),
            ("right", None, ctrl(), TermMode::NONE, b"\x1b[1;5C"),
            ("up", None, ctrl_shift(), TermMode::NONE, b"\x1b[1;6A"),
            ("right", None, ctrl(), TermMode::APP_CURSOR, b"\x1b[1;5C"),
            // home/end
            ("home", None, none(), TermMode::NONE, b"\x1b[H"),
            ("end", None, none(), TermMode::NONE, b"\x1b[F"),
            ("home", None, none(), TermMode::APP_CURSOR, b"\x1bOH"),
            ("home", None, ctrl(), TermMode::NONE, b"\x1b[1;5H"),
            // editing keys
            ("insert", None, none(), TermMode::NONE, b"\x1b[2~"),
            ("delete", None, none(), TermMode::NONE, b"\x1b[3~"),
            ("pageup", None, none(), TermMode::NONE, b"\x1b[5~"),
            ("pagedown", None, none(), TermMode::NONE, b"\x1b[6~"),
            ("delete", None, ctrl(), TermMode::NONE, b"\x1b[3;5~"),
            ("pageup", None, shift(), TermMode::NONE, b"\x1b[5;2~"),
            // function keys
            ("f1", None, none(), TermMode::NONE, b"\x1bOP"),
            ("f4", None, none(), TermMode::NONE, b"\x1bOS"),
            ("f1", None, ctrl(), TermMode::NONE, b"\x1b[1;5P"),
            ("f5", None, none(), TermMode::NONE, b"\x1b[15~"),
            ("f5", None, shift(), TermMode::NONE, b"\x1b[15;2~"),
            ("f10", None, none(), TermMode::NONE, b"\x1b[21~"),
            ("f11", None, none(), TermMode::NONE, b"\x1b[23~"),
            ("f12", None, none(), TermMode::NONE, b"\x1b[24~"),
            ("f20", None, none(), TermMode::NONE, b"\x1b[34~"),
        ];
        for (key, key_char, mods, mode, expected) in cases {
            let got = bytes(key, *key_char, *mods, *mode);
            assert_eq!(
                got.as_deref(),
                Some(*expected),
                "key={key:?} mods={mods:?} mode={mode:?}"
            );
        }
    }

    #[test]
    fn alt_as_meta_prefixes_esc() {
        assert_eq!(bytes("a", Some("a"), alt(), TermMode::NONE).unwrap(), b"\x1ba");
        assert_eq!(
            bytes("enter", None, alt(), TermMode::NONE).unwrap(),
            b"\x1b\r"
        );
        assert_eq!(
            bytes("backspace", None, alt(), TermMode::NONE).unwrap(),
            b"\x1b\x7f"
        );
        // Alt+Ctrl combines: ESC + C0.
        let m = Modifiers {
            alt: true,
            control: true,
            ..Default::default()
        };
        assert_eq!(bytes("b", None, m, TermMode::NONE).unwrap(), b"\x1b\x02");
    }

    #[test]
    fn alt_without_meta_defers_composed_char_to_ime() {
        // macOS Option-composition: with alt_is_meta = false the key table
        // must NOT consume the event — the composed char ("ß") arrives via
        // the platform text-input path (`replace_text_in_range`) instead.
        let ks = ks("s", Some("ß"), alt());
        assert_eq!(to_esc_str(&ks, &TermMode::NONE, false), None);
    }

    #[test]
    fn cmd_chords_are_not_terminal_input() {
        assert_eq!(bytes("a", Some("a"), cmd(), TermMode::NONE), None);
        assert_eq!(bytes("v", None, cmd(), TermMode::NONE), None);
    }

    #[test]
    fn bare_modifier_or_unknown_key_yields_none() {
        assert_eq!(bytes("shift", None, none(), TermMode::NONE), None);
        assert_eq!(bytes("fn", None, none(), TermMode::NONE), None);
    }

    #[test]
    fn plain_printables_defer_to_the_ime_path() {
        // Plain/shift printable input is deliberately unhandled (returns
        // `None`): consuming it would mark the key event handled and starve
        // the IME — CJK composition and dead keys would go dead. The bytes
        // reach the PTY via `InputHandler::replace_text_in_range` →
        // `TerminalView::ime_commit` exactly once.
        assert_eq!(bytes("a", Some("a"), none(), TermMode::NONE), None);
        assert_eq!(bytes("a", Some("A"), shift(), TermMode::NONE), None);
        assert_eq!(bytes("k", Some("か"), none(), TermMode::NONE), None);
    }

    #[test]
    fn modifier_param_encoding() {
        assert_eq!(modifier_param(false, false, false), 1);
        assert_eq!(modifier_param(true, false, false), 2);
        assert_eq!(modifier_param(false, true, false), 3);
        assert_eq!(modifier_param(true, true, false), 4);
        assert_eq!(modifier_param(false, false, true), 5);
        assert_eq!(modifier_param(true, false, true), 6);
        assert_eq!(modifier_param(false, true, true), 7);
        assert_eq!(modifier_param(true, true, true), 8);
    }
}
