use super::*;

fn provider(id: &str, name: &str, linked: bool, available: bool, linked_at: Option<&str>) -> api::users::SignInProvider {
    api::users::SignInProvider { id: id.into(), name: name.into(), kind: id.into(), available, linked, linked_at: linked_at.map(Into::into) }
}

fn passkey(id: &str, name: Option<&str>, backed_up: bool) -> api::users::SignInPasskey {
    api::users::SignInPasskey { id: id.into(), name: name.map(Into::into), created_at: Some("2026-09-02T09:00:00.000Z".into()), backed_up }
}

/// Every row kind: the email code, linked and unlinked providers, one no
/// longer offered, the password, and two passkeys.
fn many_ways_in() -> api::users::SignInMethods {
    api::users::SignInMethods {
        email: "dana@example.com".into(),
        email_verified: true,
        email_otp_enabled: true,
        password_enabled: true,
        passkey_enabled: true,
        providers: vec![
            provider("google", "Google", true, true, Some("2026-09-01T10:00:00.000Z")),
            provider("apple", "Apple", false, true, None),
            provider("github", "GitHub", false, false, None),
            provider("acme-sso", "Acme SSO", true, false, None),
            provider("credential", "Password", true, true, Some("2026-08-12T10:00:00.000Z")),
        ],
        passkeys: vec![passkey("pk1", Some("MacBook"), false), passkey("pk2", None, true)],
        ways_in: 5,
    }
}

/// The last way in: the code is off on this server, Google is all there is.
fn one_way_in() -> api::users::SignInMethods {
    api::users::SignInMethods {
        email: "dana@example.com".into(),
        email_verified: true,
        email_otp_enabled: false,
        password_enabled: false,
        passkey_enabled: true,
        providers: vec![provider("google", "Google", true, true, None), provider("apple", "Apple", false, true, None)],
        passkeys: vec![],
        ways_in: 1,
    }
}

fn sign_in_shot(methods: Option<api::users::SignInMethods>) -> SignInShot {
    SignInShot { methods, pending_link: None, copy_url: None, manage_url: Some("https://app.exponential.at/t/acme/settings/account".into()), loading: false }
}


fn account_shot() -> AccountShot {
    AccountShot {
        user_id: "user-dana".into(),
        name: Some("Dana Scully".into()),
        email: "dana@example.com".into(),
        picture: None,
        timezone: Some(Some("Europe/Vienna".into())),
        timezone_open: false,
        send_diagnostics: false,
        manage_url: Some("https://app.exponential.at/t/acme/settings/account".into()),
    }
}

fn account(name: &str, shot: AccountShot, sign_in: SignInShot) {
    // Settled, as the earlier pictures of this pane were taken.
    let diagnostics = shot.send_diagnostics;
    shoot_pane_settled(name, move |window, cx| AccountPane::for_shots(diagnostics, window, cx), move |pane, cx| pane.show(shot, sign_in, cx), Some(std::time::Duration::from_millis(1200)));
}

#[test]
#[ignore]
fn settings_account_initials() {
    account("account-initials", account_shot(), sign_in_shot(Some(many_ways_in())));
}

#[test]
#[ignore]
fn settings_account_picture() {
    let shot = AccountShot { picture: Some(("https://bench.invalid/avatar.png".into(), picture())), send_diagnostics: true, ..account_shot() };
    account("account-picture", shot, sign_in_shot(Some(one_way_in())));
}

#[test]
#[ignore]
fn settings_account_email_only() {
    let shot = AccountShot { name: None, timezone: Some(None), ..account_shot() };
    account("account-email-only", shot, sign_in_shot(None));
}

#[test]
#[ignore]
fn settings_account_loading() {
    let shot = AccountShot { timezone: None, manage_url: None, ..account_shot() };
    account("account-loading", shot, SignInShot { loading: true, ..sign_in_shot(None) });
}

#[test]
#[ignore]
fn settings_account_timezone_open() {
    let shot = AccountShot { timezone_open: true, ..account_shot() };
    account("account-timezone-open", shot, sign_in_shot(Some(many_ways_in())));
}

#[test]
#[ignore]
fn settings_account_linking() {
    let sign_in = SignInShot {
        pending_link: Some("apple".into()),
        copy_url: Some("https://app.exponential.at/api/auth/link/start?ticket=t-123&provider=apple&challenge=abc".into()),
        ..sign_in_shot(Some(many_ways_in()))
    };
    account("account-linking", AccountShot { send_diagnostics: true, ..account_shot() }, sign_in);
}
