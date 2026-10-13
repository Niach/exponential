//! EXP-1253 — HOME's copy and geometry, pinned by
//! `fixtures/composer-home.json`. EXP-1294 (FEED-100): the greeting row is
//! gone — the page wears the dawn ([`crate::chat_screen`]) over the
//! composer — so only the placeholder, the element ids and the Getting
//! started gap are read here.

/// The composer's placeholder with no subject picked.
pub(crate) const HOME_PLACEHOLDER: &str = "What do you want to build?";

/// The fixture's `testIds`: the IDE has no DOM, so they name the home's
/// element ids (a capture or a test finds the same pieces as on the web).
/// `mark` names the dawn.
pub(crate) const TEST_ID_HOME: &str = "agent-home";
pub(crate) const TEST_ID_MARK: &str = "agent-home-mark";

/// While the team has no board: the gap from the options line to Getting
/// started (px).
pub(crate) const COMPOSER_TO_GETTING_STARTED_PX: f32 = 32.;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/composer-home.json");

    #[test]
    fn copy_and_geometry_are_the_fixture() {
        let fixture: Value = serde_json::from_str(FIXTURE).expect("composer-home.json parses");
        assert_eq!(fixture["placeholder"].as_str().unwrap(), HOME_PLACEHOLDER);
        assert_eq!(
            fixture["geometry"]["composerToGettingStarted"].as_f64().unwrap() as f32,
            COMPOSER_TO_GETTING_STARTED_PX
        );
        let ids = &fixture["testIds"];
        assert_eq!(ids["home"].as_str().unwrap(), TEST_ID_HOME);
        assert_eq!(ids["mark"].as_str().unwrap(), TEST_ID_MARK);
    }
}
