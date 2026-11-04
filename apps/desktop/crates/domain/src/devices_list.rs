//! #EXP-1643: THE account panel's pure half, the desktop mirror of
//! `packages/domain-contract/src/devices-list.ts` (`openDeviceAccount`,
//! `DEVICE_CARD`). The spec is `fixtures/devices-list.json` (its `$comment`);
//! the tests below replay `accountPanelCases` whole. The rest of the page's
//! derivation lives with its card in `crates/ui/src/machines.rs`.

/// `card.deviceIconSize`: the device's own icon leading the card.
pub const CARD_DEVICE_ICON_SIZE: f32 = 20.;
/// `card.panelGap`: above and below the panel's hairline.
pub const CARD_PANEL_GAP: f32 = 12.;

/// The login a card's panel shows. `remembered`: `None` = this device was never
/// touched on this client, `Some(None)` = closed by hand, `Some(Some(key))` =
/// the login opened last.
pub fn open_device_account<'a>(
    login_keys: &[&'a str],
    remembered: Option<Option<&str>>,
) -> Option<&'a str> {
    let remembered = match remembered {
        Some(None) => return None,
        Some(Some(key)) => Some(key),
        None => None,
    };
    if let Some(found) = remembered.and_then(|key| login_keys.iter().find(|login| **login == key)) {
        return Some(found);
    }
    match login_keys {
        [only] => Some(only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/devices-list.json"
        ))
        .expect("devices-list.json parses")
    }

    #[test]
    fn devices_list_open_account_replays_the_fixture() {
        let fixture = fixture();
        let cases = fixture["accountPanelCases"].as_array().expect("accountPanelCases");
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().expect("name");
            let keys: Vec<&str> = case["loginKeys"]
                .as_array()
                .expect("loginKeys")
                .iter()
                .map(|key| key.as_str().expect("key"))
                .collect();
            // Absent = never touched, null = closed by hand.
            let remembered = case.get("remembered").map(|value| value.as_str());
            assert_eq!(
                open_device_account(&keys, remembered),
                case["expect"].as_str(),
                "{name}"
            );
        }
    }

    #[test]
    fn devices_list_card_geometry_matches_the_fixture() {
        let fixture = fixture();
        let card = &fixture["card"];
        assert_eq!(card["deviceIconSize"].as_f64(), Some(CARD_DEVICE_ICON_SIZE as f64));
        assert_eq!(card["panelGap"].as_f64(), Some(CARD_PANEL_GAP as f64));
    }
}
