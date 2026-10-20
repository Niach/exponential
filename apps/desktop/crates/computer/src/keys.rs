//! Key names, one vocabulary on every OS: the `key` tool's combos
//! (`ctrl+shift+t`, `Return`, `cmd+space`) and what `type` feeds the
//! backend per character. The names are xdotool's / X keysym names, which
//! every computer-use prompt already speaks, plus the obvious aliases.

/// A key that is not a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Named {
    Return,
    Tab,
    Escape,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,
    Insert,
    Space,
    /// F1..F24.
    F(u8),
    Control,
    Shift,
    Alt,
    /// Super / Command / the Windows key.
    Super,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Named(Named),
    /// Any character; a backend adds Shift where its layout needs it.
    Char(char),
}

/// A parsed combo: the modifiers held, then the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Combo {
    pub modifiers: Vec<Named>,
    pub key: Key,
}

impl Named {
    pub(crate) fn is_modifier(self) -> bool {
        matches!(self, Named::Control | Named::Shift | Named::Alt | Named::Super)
    }

    /// The X keysym (`X11/keysymdef.h`).
    #[cfg(target_os = "linux")]
    pub(crate) fn keysym(self) -> u32 {
        match self {
            Named::Return => 0xff0d,
            Named::Tab => 0xff09,
            Named::Escape => 0xff1b,
            Named::Backspace => 0xff08,
            Named::Delete => 0xffff,
            Named::Home => 0xff50,
            Named::End => 0xff57,
            Named::PageUp => 0xff55,
            Named::PageDown => 0xff56,
            Named::Left => 0xff51,
            Named::Right => 0xff53,
            Named::Up => 0xff52,
            Named::Down => 0xff54,
            Named::Insert => 0xff63,
            Named::Space => 0x20,
            Named::F(n) => 0xffbe + u32::from(n.clamp(1, 24) - 1),
            Named::Control => 0xffe3,
            Named::Shift => 0xffe1,
            Named::Alt => 0xffe9,
            Named::Super => 0xffeb,
        }
    }
}

/// One token of a combo (case-insensitive; a single character is itself).
pub(crate) fn parse_key(token: &str) -> Result<Key, String> {
    let mut chars = token.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        return Ok(Key::Char(ch));
    }
    let named = match token.to_ascii_lowercase().as_str() {
        "return" | "enter" | "kp_enter" => Named::Return,
        "tab" => Named::Tab,
        "escape" | "esc" => Named::Escape,
        "backspace" => Named::Backspace,
        "delete" | "del" => Named::Delete,
        "home" => Named::Home,
        "end" => Named::End,
        "pageup" | "page_up" | "prior" => Named::PageUp,
        "pagedown" | "page_down" | "next" => Named::PageDown,
        "left" | "arrowleft" => Named::Left,
        "right" | "arrowright" => Named::Right,
        "up" | "arrowup" => Named::Up,
        "down" | "arrowdown" => Named::Down,
        "insert" => Named::Insert,
        "space" => Named::Space,
        "ctrl" | "control" | "ctrl_l" | "control_l" => Named::Control,
        "shift" | "shift_l" => Named::Shift,
        "alt" | "option" | "opt" | "alt_l" => Named::Alt,
        "super" | "cmd" | "command" | "win" | "meta" | "super_l" => Named::Super,
        "plus" => return Ok(Key::Char('+')),
        "minus" => return Ok(Key::Char('-')),
        other => {
            if let Some(n) = other.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                if (1..=24).contains(&n) {
                    return Ok(Key::Named(Named::F(n)));
                }
            }
            return Err(format!("Unknown key \"{token}\"."));
        }
    };
    Ok(Key::Named(named))
}

/// `ctrl+shift+t`: every token but the last must be a modifier.
pub(crate) fn parse_combo(combo: &str) -> Result<Combo, String> {
    let combo = combo.trim();
    if combo.is_empty() {
        return Err("Name a key.".to_string());
    }
    // Split on "+"; two empty parts in a row are the plus key itself
    // ("ctrl++", a bare "+").
    let parts: Vec<&str> = combo.split('+').collect();
    let mut tokens: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < parts.len() {
        if parts[index].is_empty() {
            if parts.get(index + 1).is_some_and(|next| next.is_empty()) {
                tokens.push("+");
                index += 2;
                continue;
            }
            return Err(format!("A stray \"+\" in \"{combo}\"; name the plus key \"plus\"."));
        }
        tokens.push(parts[index]);
        index += 1;
    }
    if tokens.is_empty() {
        return Err("Name a key.".to_string());
    }
    let (last, modifiers) = tokens.split_last().expect("one token");
    let mut held = Vec::new();
    for token in modifiers {
        match parse_key(token)? {
            Key::Named(named) if named.is_modifier() => held.push(named),
            _ => return Err(format!("\"{token}\" is not a modifier; the key comes last.")),
        }
    }
    Ok(Combo { modifiers: held, key: parse_key(last)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_parse_modifiers_then_the_key() {
        assert_eq!(
            parse_combo("ctrl+shift+T").unwrap(),
            Combo { modifiers: vec![Named::Control, Named::Shift], key: Key::Char('T') }
        );
        assert_eq!(parse_combo("Return").unwrap(), Combo { modifiers: vec![], key: Key::Named(Named::Return) });
        assert_eq!(parse_combo("cmd+space").unwrap().key, Key::Named(Named::Space));
        assert_eq!(parse_combo("alt+F4").unwrap().key, Key::Named(Named::F(4)));
        assert_eq!(parse_combo("ctrl++").unwrap().key, Key::Char('+'));
        assert_eq!(parse_combo("+").unwrap().key, Key::Char('+'));
        assert_eq!(parse_combo("ctrl+plus").unwrap().key, Key::Char('+'));
        assert_eq!(parse_combo("ü").unwrap().key, Key::Char('ü'));
    }

    #[test]
    fn a_non_modifier_before_the_key_and_unknown_names_are_refused() {
        assert!(parse_combo("a+b").unwrap_err().contains("not a modifier"));
        assert!(parse_combo("ctrl+bogus").unwrap_err().contains("Unknown key"));
        assert!(parse_combo("").is_err());
        assert!(parse_combo("a+").is_err());
        assert!(parse_combo("f25").is_err());
    }
}
