#if DEBUG
import ExpCore
import Foundation

/// CAPTURE-ONLY seam for the screenshot suites (EXP-1267), compiled into DEBUG
/// builds only and inert unless the process was launched with `-uiTesting`.
///
/// The suites used to erase the simulator before every run and type the demo
/// credentials into InstanceView → LoginView: a minute of every run, and a
/// springboard save-password sheet to dodge. Instead:
///
/// - `-uiTestingReset` returns the app to a fresh-install state at launch:
///   every keychain account is dropped and the app's UserDefaults domain is
///   cleared (segment choices, the per-run view prefs). Per-user SQLite caches
///   are KEPT on purpose — they are keyed by instance + userId, so the next
///   sign-in as the same user resumes its synced rows instead of
///   re-downloading every shape, and a reseed mints new user ids (a new file).
/// - `EXP_UITEST_SESSION` (launch environment, JSON
///   `{instanceUrl, token, user}`) then persists that session exactly like a
///   login does (`AuthRepository.setInstanceUrl` + `setToken`), so the app
///   boots straight into the main UI (or the wizard) of that account. The UI
///   test minted or re-validated the token over HTTP before launching; see
///   `ScreenshotSession` in ExponentialUITests/ScreenshotFlow.swift.
///
/// Nothing in the product reads or writes either: no route, no setting, no
/// deep link, and a release build does not contain this file.
enum UITestingSession {
    private static var isUITesting: Bool {
        ProcessInfo.processInfo.arguments.contains("-uiTesting")
    }

    /// One-shot DB cleanup flags `AppDependencies` keeps in UserDefaults.
    /// Clearing them would re-run the orphan sweep, which deletes the cached
    /// databases of every account not signed in right now (the demo user's,
    /// while the styleguide suite is signed in as the newcomer).
    private static let preservedDefaultsKeys = ["peruser_db_cleanup_v1", "peruser_db_cleanup_v2"]

    /// Before `AuthRepository` reads the store: drop every account and the
    /// app's defaults when the suite asked for a fresh start.
    static func resetIfRequested(accountStore: AccountStore) {
        guard isUITesting, ProcessInfo.processInfo.arguments.contains("-uiTestingReset") else { return }
        accountStore.removeAccounts(ids: accountStore.accounts.map(\.id))
        if let domain = Bundle.main.bundleIdentifier {
            UserDefaults.standard.removePersistentDomain(forName: domain)
        }
        for key in preservedDefaultsKeys {
            UserDefaults.standard.set(true, forKey: key)
        }
    }

    private struct Payload: Decodable {
        let instanceUrl: String
        let token: String
        let user: AuthUser
    }

    /// After `AuthRepository` exists: persist the injected session, if any.
    static func injectIfProvided(into auth: AuthRepository) {
        guard isUITesting,
              let raw = ProcessInfo.processInfo.environment["EXP_UITEST_SESSION"],
              let data = raw.data(using: .utf8)
        else { return }
        guard let payload = try? JSONDecoder().decode(Payload.self, from: data),
              !payload.token.isEmpty, !payload.user.id.isEmpty
        else {
            NSLog("EXP-1267 uiTesting: EXP_UITEST_SESSION did not decode — the suite falls back to the login UI")
            return
        }
        auth.setInstanceUrl(payload.instanceUrl)
        auth.setToken(
            payload.token,
            email: payload.user.email,
            name: payload.user.name,
            userId: payload.user.id,
            isAdmin: payload.user.isAdmin ?? false,
            onboardingCompletedAt: payload.user.onboardingCompletedAt,
            onboardingKnown: true
        )
        NSLog("EXP-1267 uiTesting: signed in as \(payload.user.email) without the login UI")
    }
}
#endif
