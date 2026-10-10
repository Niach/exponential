import XCTest

/// Shared drive-the-app machinery for the two screenshot suites
/// (`StoreScreenshots` — the 8 App Store shots — and `StyleguideScreenshots` —
/// the cross-platform styleguide set).
///
/// Launching with the UI-testing flag + a system interruption monitor, the
/// session reuse that signs the suites in without the login UI (EXP-1267), the
/// InstanceView → LoginView fallback, the wait budgets (`Wait`) and the
/// tap/scroll/settle helpers. No suite may fork this behavior — both capture
/// against the same seeded backend (`apps/web/scripts/seed-screenshots.ts`).

/// Credentials + instance target of the seeded demo backend.
enum ScreenshotSeed {
    static let demoEmail = "demo@exponential.at"
    static let demoPassword = "screenshots-demo"

    /// The seed's SECOND identity (EXP-627): verified, member of nothing, with
    /// a null `onboardingCompletedAt`. Signing in as them is the only way to
    /// photograph the first-run wizard — the demo user completed it long ago
    /// and is bounced straight to a board. Keep in lockstep with
    /// `apps/web/scripts/screenshot-demo.ts` (NEWCOMER_EMAIL / NEWCOMER_PASSWORD).
    static let newcomerEmail = "newcomer@exponential.at"
    static let newcomerPassword = "screenshots-newcomer"

    /// The seed's THIRD identity (EXP-725): verified, OWNER of a team, with a
    /// null `onboardingCompletedAt` — the only way to photograph the wizard's
    /// invite and devices steps, which both need a RESOLVED team. The
    /// newcomer cannot reach them without creating one (which would mutate
    /// the seed). Keep in lockstep with
    /// `apps/web/scripts/screenshot-demo.ts` (STARTER_EMAIL / STARTER_PASSWORD).
    static let starterEmail = "starter@exponential.at"
    static let starterPassword = "screenshots-starter"

    /// The instance URL defaults to http://localhost:5173 and can be overridden
    /// with the SNAPSHOT_INSTANCE_URL environment variable (bridged through
    /// TEST_RUNNER_SNAPSHOT_INSTANCE_URL by the Snapfile — xcodebuild only
    /// forwards TEST_RUNNER_-prefixed variables into the runner process).
    static var instanceUrl: String {
        ProcessInfo.processInfo.environment["SNAPSHOT_INSTANCE_URL"]
            ?? "http://localhost:5173"
    }
}

/// Reaches the fastlane-provided global `snapshot(_:)` from inside the
/// same-named XCTestCase overload below, where unqualified name lookup would
/// otherwise find the member first.
@MainActor
private func captureSnapshot(_ name: String) {
    // No network-indicator wait: every capture is gated on real content and
    // settled explicitly by the caller.
    snapshot(name, timeWaitingForIdle: 0)
}

/// The optional per-run shot allowlist (EXP-642 / A4).
///
/// `bundle exec fastlane screenshots shots:01_board,02_issue-detail` sets
/// `TEST_RUNNER_EXP_SHOTS`, which xcodebuild forwards into the runner process
/// as `EXP_SHOTS` (the prefix is stripped). Unset = capture everything, which
/// is what a plain lane run does.
///
/// Only the CAPTURE is skipped, never the navigation: the suites are one long
/// scripted walk through the app, and skipping a tap would strand every later
/// shot on the wrong screen. A subset run is therefore not faster, only
/// narrower — which is exactly what the automation needs when a diff touched
/// two views. The one exception is a self-contained DETOUR (open a sheet,
/// capture, close it — e.g. `sg_issue-properties`): a suite may gate the whole
/// detour on `isWanted`, so a step no later shot needs cannot fail a scoped
/// run.
///
/// `offered` is `nonisolated(unsafe)` mutable static state on purpose: the
/// capture suites are single-threaded scripts, and isolating it to the main
/// actor would make the tearDown typo check unoverridable (XCTestCase.tearDown
/// is not main-actor isolated).
enum ScreenshotShots {

    /// nil = no allowlist (capture everything).
    static let requested: Set<String>? = {
        guard let raw = ProcessInfo.processInfo.environment["EXP_SHOTS"] else { return nil }
        let ids = raw
            .split(whereSeparator: { $0 == "," || $0 == " " || $0 == "\n" })
            .map(String.init)
            .filter { !$0.isEmpty }
        return ids.isEmpty ? nil : Set(ids)
    }()

    /// Every id a suite actually reached — the typo check below compares this
    /// against `requested`.
    nonisolated(unsafe) private(set) static var offered: Set<String> = []

    /// Records `name` as reached and reports whether it should be captured.
    @discardableResult
    static func isWanted(_ name: String) -> Bool {
        offered.insert(name)
        guard let requested else { return true }
        return requested.contains(name)
    }

    /// Requested ids the suite never reached — a misspelt `shots:` value, or a
    /// name from the other lane.
    static var unreached: [String] {
        guard let requested else { return [] }
        return requested.subtracting(offered).sorted()
    }
}

/// How long a step may take before the run FAILS (EXP-1267).
///
/// A missing element must cost seconds, not minutes: with `continueAfterFailure
/// = false` the first unmet wait ends the suite, and the lane retries a failed
/// run at most once. Pick the class by what the element waits ON:
enum Wait {
    /// A cold launch reaching its first screen (picker, login, main UI or the
    /// wizard) — the app process starts and reads the keychain, nothing more.
    static let launch: TimeInterval = 15
    /// UI navigation: a push, a sheet, a tab switch, a control on a screen that
    /// is already up. Everything it needs is local.
    static let nav: TimeInterval = 8
    /// Synced content AFTER the first sync landed: every shape starts at
    /// sign-in, so once the board's rows are in, the rest follow within
    /// seconds (and a reused session resumes from its local cache).
    static let sync: TimeInterval = 15
    /// The FIRST sync after sign-in, gated once on the board's showcase row.
    /// A cold cache downloads every shape before it.
    static let firstSync: TimeInterval = 60
    /// Loads that depend on something beyond the local backend: PR files
    /// fetched from GitHub, the relay replaying a transcript, the stand-in
    /// desktop's device row.
    static let network: TimeInterval = 30
}

extension XCTestCase {
    /// Waits for `element` and FAILS the run naming it when it never shows:
    /// the human message first, then the query XCUITest resolved (identifier,
    /// type, predicate), so a red run says what was missing without opening
    /// the xcresult.
    @MainActor
    func expect(
        _ element: XCUIElement,
        within timeout: TimeInterval,
        _ message: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard !element.waitForExistence(timeout: timeout) else { return }
        XCTFail(
            "\(message) — missing after \(Int(timeout))s: \(element.description)",
            file: file,
            line: line
        )
    }
}

/// Signs the suites in WITHOUT the login UI (EXP-1267).
///
/// The test runner talks to the instance over HTTP (the simulator shares the
/// host's network, so `http://localhost:5173` resolves) and hands the app a
/// ready session through `EXP_UITEST_SESSION`; the app's DEBUG-only
/// `UITestingSession` persists it exactly like a login. The token is REUSED
/// across runs and test cases: it is cached per lane (in the snapshot cache
/// dir, so two lanes running side by side never share one) and re-validated
/// with one `get-session` read before every launch. A reseed deletes the
/// demo users, so that read comes back empty and a fresh token is minted with
/// the seed's password. Only when the instance cannot be reached or rejects
/// the password does `payload` return nil, and the suite falls back to typing
/// the credentials into the login UI.
enum ScreenshotSession {
    /// The JSON `EXP_UITEST_SESSION` carries, or nil when no session could be
    /// established over HTTP (→ the UI login fallback).
    @MainActor
    static func payload(
        email: String,
        password: String,
        instanceUrl: String = ScreenshotSeed.instanceUrl
    ) -> String? {
        var base = instanceUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        while base.hasSuffix("/") { base.removeLast() }
        let cache = cacheFile(base: base, email: email)

        if let data = try? Data(contentsOf: cache),
           let cached = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let token = cached["token"] as? String,
           let user = sessionUser(base: base, token: token),
           (user["email"] as? String)?.lowercased() == email.lowercased() {
            NSLog("EXP-1267 session: reusing the cached session for \(email)")
            return encode(base: base, token: token, user: user)
        }

        guard let token = mint(base: base, email: email, password: password) else {
            NSLog("EXP-1267 session: could not sign \(email) in over HTTP at \(base) — falling back to the login UI")
            return nil
        }
        guard let user = sessionUser(base: base, token: token) else {
            NSLog("EXP-1267 session: \(base) minted a token for \(email) but get-session has no user — falling back to the login UI")
            return nil
        }
        try? FileManager.default.createDirectory(
            at: cache.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        if let data = try? JSONSerialization.data(withJSONObject: ["token": token]) {
            try? data.write(to: cache, options: .atomic)
        }
        NSLog("EXP-1267 session: minted a new session for \(email)")
        return encode(base: base, token: token, user: user)
    }

    /// Forgets the cached token for `email` — after the app rejected it.
    @MainActor
    static func forget(email: String, instanceUrl: String = ScreenshotSeed.instanceUrl) {
        var base = instanceUrl
        while base.hasSuffix("/") { base.removeLast() }
        try? FileManager.default.removeItem(at: cacheFile(base: base, email: email))
    }

    @MainActor
    private static func cacheFile(base: String, email: String) -> URL {
        let root = Snapshot.cacheDirectory ?? FileManager.default.temporaryDirectory
        let key = "\(base)-\(email)".map { $0.isLetter || $0.isNumber ? $0 : "_" }
        return root
            .appendingPathComponent("exp-sessions", isDirectory: true)
            .appendingPathComponent(String(key) + ".json")
    }

    private static func encode(base: String, token: String, user: [String: Any]) -> String? {
        let payload: [String: Any] = ["instanceUrl": base, "token": token, "user": user]
        guard let data = try? JSONSerialization.data(withJSONObject: payload) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// `POST /api/auth/sign-in/email` — the bearer token, or nil.
    private static func mint(base: String, email: String, password: String) -> String? {
        guard let url = URL(string: "\(base)/api/auth/sign-in/email"),
              let body = try? JSONSerialization.data(withJSONObject: ["email": email, "password": password])
        else { return nil }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.httpBody = body
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        // Better Auth's CSRF check wants an Origin; send the instance's own,
        // exactly as the app's HTTPClient does.
        if let scheme = url.scheme, let host = url.host {
            request.setValue(
                url.port.map { "\(scheme)://\(host):\($0)" } ?? "\(scheme)://\(host)",
                forHTTPHeaderField: "Origin"
            )
        }
        guard let (status, data) = send(request), (200...299).contains(status),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let token = json["token"] as? String, !token.isEmpty
        else { return nil }
        return token
    }

    /// `GET /api/auth/get-session` with the bearer — the session's user, or
    /// nil for a dead token (Better Auth answers it 200 + `null`), a failed
    /// request or an unreachable instance.
    private static func sessionUser(base: String, token: String) -> [String: Any]? {
        guard let url = URL(string: "\(base)/api/auth/get-session") else { return nil }
        var request = URLRequest(url: url)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        guard let (status, data) = send(request), (200...299).contains(status),
              let json = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) as? [String: Any],
              let user = json["user"] as? [String: Any],
              let id = user["id"] as? String, !id.isEmpty
        else { return nil }
        return user
    }

    private final class Box: @unchecked Sendable {
        var value: (Int, Data)?
    }

    private static let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 8
        configuration.timeoutIntervalForResource = 10
        return URLSession(configuration: configuration)
    }()

    /// One blocking request (the suites are scripted, single-threaded walks).
    private static func send(_ request: URLRequest) -> (Int, Data)? {
        let box = Box()
        let done = DispatchSemaphore(value: 0)
        session.dataTask(with: request) { data, response, _ in
            if let http = response as? HTTPURLResponse {
                box.value = (http.statusCode, data ?? Data())
            }
            done.signal()
        }.resume()
        _ = done.wait(timeout: .now() + 12)
        return box.value
    }
}

/// Where a launch landed, before anything has been tapped.
enum LaunchStage {
    /// InstanceView is up (the cloud buttons + the self-hosted link, or the
    /// bare URL field when the cloud account already exists).
    case instancePicker
    /// LoginView for an instance the app already knows.
    case login
    /// An account holds a token — the main UI or, for an account that never
    /// finished it, the first-run wizard.
    case signedIn
    /// None of the above within the timeout.
    case unknown
}

/// The launch arguments fastlane's `setupSnapshot` added (language, locale,
/// `-FASTLANE_SNAPSHOT`), captured once so every relaunch starts from them.
@MainActor
private var screenshotBaseArguments: [String] = []

extension XCTestCase {

    // MARK: - Launch

    /// Builds the app under test with the screenshot flags and the
    /// belt-and-braces system-alert monitor and hands it to fastlane snapshot.
    /// Does NOT launch it — `launchSignedIn` / `launchSignedOut` do.
    @MainActor
    func makeScreenshotApp() -> XCUIApplication {
        let app = XCUIApplication()
        // -uiTesting suppresses the push-permission request (AppDependencies)
        // so no system alert ever sits on top of a capture, and arms the
        // DEBUG-only UITestingSession seam.
        app.launchArguments += ["-uiTesting"]

        // Belt and braces: if any system alert appears anyway, dismiss it.
        addUIInterruptionMonitor(withDescription: "System dialog") { alert in
            for label in ["Allow", "OK", "Don't Allow", "Not Now", "Später", "Nicht jetzt", "Cancel"] {
                let button = alert.buttons[label]
                if button.exists {
                    button.tap()
                    return true
                }
            }
            return false
        }

        // The suites settle explicitly before every capture; fastlane's own
        // extra second per shot (waitForAnimations) only made runs longer.
        setupSnapshot(app, waitForAnimations: false)
        screenshotBaseArguments = app.launchArguments
        return app
    }

    /// (Re)launches the app in a fresh-install state with NO account: the
    /// untouched InstanceView (sg_sign-in). Replaces the simulator erase the
    /// lanes used to pay for on every run.
    @MainActor
    func launchSignedOut(_ app: XCUIApplication) {
        relaunch(app, arguments: ["-uiTestingReset"], session: nil)
    }

    /// (Re)launches the app signed in as `email`, fresh-install state
    /// otherwise. The fast path hands the app a reused (or freshly minted)
    /// session — no typing, no save-password sheet. It falls back to the login
    /// UI when no session can be had over HTTP or the app did not come up
    /// signed in with it. Returns whether the fast path held.
    @MainActor
    @discardableResult
    func launchSignedIn(
        _ app: XCUIApplication,
        email: String = ScreenshotSeed.demoEmail,
        password: String = ScreenshotSeed.demoPassword,
        extraArguments: [String] = []
    ) -> Bool {
        let session = ScreenshotSession.payload(email: email, password: password)
        relaunch(app, arguments: ["-uiTestingReset"] + extraArguments, session: session)
        if session != nil {
            if awaitLaunchStage(app) == .signedIn { return true }
            NSLog("EXP-1267 session: the app did not come up signed in as \(email) — falling back to the login UI")
            ScreenshotSession.forget(email: email)
        }
        signIn(app, email: email, password: password)
        return false
    }

    @MainActor
    private func relaunch(_ app: XCUIApplication, arguments: [String], session: String?) {
        if app.state != .notRunning { app.terminate() }
        app.launchArguments = screenshotBaseArguments + arguments
        if let session {
            app.launchEnvironment["EXP_UITEST_SESSION"] = session
        } else {
            app.launchEnvironment.removeValue(forKey: "EXP_UITEST_SESSION")
        }
        app.launch()
    }

    // MARK: - Sign in (the UI fallback)

    /// Waits out a launch WITHOUT touching anything and reports what is on
    /// screen.
    @MainActor
    func awaitLaunchStage(_ app: XCUIApplication, timeout: TimeInterval = Wait.launch) -> LaunchStage {
        let urlField = app.textFields["instance-url-field"]
        let selfHostLink = app.buttons["instance-self-host-link"]
        let loginEmail = app.buttons["login-continue-with-email-button"]
        let issuesTab = app.buttons["tab-issues"]
        // The first-run wizard of an account that never finished it: its
        // signed-in footer, or the step a capture hook parked it on.
        let wizardMarks = ["onboarding-signed-in-as", "onboarding-invite-step", "onboarding-devices-step"]
            .map { anyElement(app, identified: $0) }
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if issuesTab.exists || wizardMarks.contains(where: { $0.exists }) { return .signedIn }
            if loginEmail.exists { return .login }
            if selfHostLink.exists || urlField.exists { return .instancePicker }
            usleep(250_000)
        } while Date() < deadline
        return .unknown
    }

    /// InstanceView: replace the prefilled "https://" with the target URL and
    /// continue, leaving LoginView's email form on screen. Copes with a launch
    /// that is already on LoginView (Back to the picker) or parked on the
    /// first-run wizard (its persistent Sign out).
    @MainActor
    func presentLoginScreen(_ app: XCUIApplication) {
        let instanceUrl = ScreenshotSeed.instanceUrl
        var stage = awaitLaunchStage(app)
        if stage == .signedIn, app.buttons["Sign out"].exists {
            // EXP-725: an account parked on the first-run WIZARD; its
            // persistent "Sign out" is the way back to the login flow.
            app.buttons["Sign out"].firstMatch.tap()
            stage = awaitLaunchStage(app)
        }
        if stage == .login, app.buttons["Back"].exists {
            // LoginView for a stale instance URL: Back returns to the picker.
            app.buttons["Back"].firstMatch.tap()
            stage = awaitLaunchStage(app)
        }
        guard stage == .instancePicker else {
            XCTFail("Expected the instance picker before the UI sign-in, found \(stage)")
            return
        }
        let urlField = app.textFields["instance-url-field"]
        let selfHostLink = app.buttons["instance-self-host-link"]
        // Cloud is the primary path now (EXP-14) — reveal the self-hosted URL
        // field before pointing the app at the local backend.
        if selfHostLink.exists && !urlField.exists {
            selfHostLink.tap()
        }
        expect(urlField, within: Wait.nav, "The self-hosted URL field never appeared")
        focus(urlField)
        clearText(of: urlField)
        urlField.typeText(instanceUrl)

        let continueButton = app.buttons["instance-continue-button"]
        expect(continueButton, within: Wait.nav, "InstanceView has no Continue button")
        continueButton.tap()

        // LoginView appears once /api/auth-config resolves. EXP-857: the email
        // step is revealed by a button now, so the form is one tap away.
        let revealEmail = app.buttons["login-continue-with-email-button"]
        expect(revealEmail, within: Wait.sync, "Login screen never appeared — is the backend running at \(instanceUrl)?")
        revealEmail.tap()
        expect(app.textFields["login-email-field"], within: Wait.nav, "Login email field never appeared")
    }

    /// Fills in credentials on an already-visible LoginView and submits.
    @MainActor
    func submitLogin(
        _ app: XCUIApplication,
        email: String = ScreenshotSeed.demoEmail,
        password: String = ScreenshotSeed.demoPassword
    ) {
        // EXP-857: reveal the email step if the screen is still on its buttons,
        // and take the password branch when the instance also offers codes.
        let revealEmail = app.buttons["login-continue-with-email-button"]
        if revealEmail.exists {
            revealEmail.tap()
        }
        let usePassword = app.buttons["login-use-password-link"]
        if usePassword.waitForExistence(timeout: 2) {
            usePassword.tap()
        }

        let emailField = app.textFields["login-email-field"]
        expect(emailField, within: Wait.nav, "Login email field never appeared")
        focus(emailField)
        emailField.typeText(email)

        // Plain textField (not secureTextField): under -uiTesting the app
        // renders the password field unsecured so the system save-password
        // sheet can never appear (see LoginView.glassTextField).
        let passwordField = app.textFields["login-password-field"]
        expect(passwordField, within: Wait.nav, "Login password field never appeared")
        focus(passwordField)
        passwordField.typeText(password)

        let signInButton = app.buttons["login-submit-button"]
        expect(signInButton, within: Wait.nav, "Login submit button never appeared")
        signInButton.tap()

        // iOS may offer to save the password into the keychain right after a
        // submit — a springboard sheet that photobombs the first capture (and
        // blocks every later tap). Only the UI login path can raise it.
        dismissSavePasswordSheet(timeout: 5)
    }

    /// The full InstanceView → LoginView → main UI sign-in flow (the fallback
    /// when no session could be injected).
    @MainActor
    func signIn(
        _ app: XCUIApplication,
        email: String = ScreenshotSeed.demoEmail,
        password: String = ScreenshotSeed.demoPassword
    ) {
        presentLoginScreen(app)
        submitLogin(app, email: email, password: password)
    }

    /// Dismisses the springboard "Save Password?" sheet if it shows up within
    /// `timeout`, in whatever language the simulator speaks.
    @MainActor
    func dismissSavePasswordSheet(timeout: TimeInterval) {
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            for label in ["Not Now", "Später", "Nicht jetzt"] {
                let dismiss = springboard.buttons[label]
                if dismiss.exists && dismiss.isHittable {
                    dismiss.tap()
                    return
                }
            }
            usleep(500_000)
        }
    }

    /// Taps the field until it actually owns keyboard focus — a plain tap
    /// right after boot sometimes loses the race and typeText() then fails
    /// with "Neither element nor any descendant has keyboard focus".
    @MainActor
    func focus(_ element: XCUIElement) {
        for _ in 0..<5 {
            element.tap()
            let focused = (element.value(forKey: "hasKeyboardFocus") as? Bool) ?? false
            if focused { return }
            usleep(500_000)
        }
    }

    // MARK: - Helpers

    /// Deletes the element's current text. The field must already be focused;
    /// tapping the (wide, short-text) field puts the caret at the end, so a
    /// stream of delete keystrokes clears it.
    @MainActor
    func clearText(of element: XCUIElement) {
        guard let current = element.value as? String, !current.isEmpty else { return }
        let deletes = String(repeating: XCUIKeyboardKey.delete.rawValue, count: current.count + 2)
        element.typeText(deletes)
    }

    /// Opens an issue from the board by its title, scrolling the list until the
    /// row is on screen.
    ///
    /// Taps the row's TITLE text, never the `issue-row-*` button element: since
    /// the glass chip rework the row element's accessibility activation point
    /// lands on the leading priority control, so an element tap opens the
    /// priority picker sheet instead of navigating (EXP-348).
    @MainActor
    func openIssue(_ app: XCUIApplication, title: String) {
        let row = app.staticTexts[title]
        var scrollAttempts = 0
        while !row.isHittable && scrollAttempts < 12 {
            app.swipeUp()
            scrollAttempts += 1
        }
        guard row.isHittable else {
            XCTFail("Issue \"\(title)\" never became tappable on the board — missing: \(row.description)")
            return
        }
        row.tap()
    }

    /// Pops the top view controller off the navigation stack (leading nav-bar
    /// back button).
    @MainActor
    func goBack(_ app: XCUIApplication) {
        let backButton = app.navigationBars.buttons.firstMatch
        if backButton.waitForExistence(timeout: Wait.nav) {
            backButton.tap()
        }
    }

    /// Give in-flight animations (and Electric row inserts) a moment to settle
    /// before capturing.
    @MainActor
    func settle(_ seconds: UInt32) {
        sleep(seconds)
    }

    // MARK: - Capture

    /// Settle, then capture — unless `name` is outside this run's `shots:`
    /// allowlist, in which case nothing is written and nothing is waited for.
    ///
    /// Every capture in both suites goes through this overload rather than the
    /// bare fastlane `snapshot(_:)`, so the allowlist can never be bypassed by
    /// accident. The literal `snapshot("…"` at each call site is load-bearing:
    /// `packages/view-catalog/src/views.test.ts` greps for it to prove the
    /// suites and the catalog list the same shots.
    /// `popRects` opts the shot into the store compositor's pop-out sidecar
    /// (EXP-627): the rect is measured AFTER the settle, so it describes the
    /// same frame the PNG does.
    @MainActor
    func snapshot(_ name: String, settle seconds: Double, popRects app: XCUIApplication? = nil) {
        guard ScreenshotShots.isWanted(name) else {
            NSLog("EXP-642 shots: skipping \(name) — not in EXP_SHOTS")
            return
        }
        if seconds > 0 { usleep(useconds_t(seconds * 1_000_000)) }
        if let app { PopRects.dump(name, app) }
        captureSnapshot(name)
    }

    /// Fails the run when a `shots:` id was never reached — almost always a
    /// typo, which would otherwise look like a perfectly green empty run.
    /// Call from `tearDown`, guarded on the suite having run to completion so
    /// an earlier failure is not buried under a second one.
    func assertRequestedShotsWereReached(suiteFinished: Bool) {
        guard suiteFinished else { return }
        let missing = ScreenshotShots.unreached
        XCTAssertTrue(
            missing.isEmpty,
            "EXP-642 shots: \(missing.joined(separator: ", ")) — no such shot in this suite"
        )
    }

    /// Scrolls until `element` is on screen (or `attempts` swipes have gone by),
    /// reporting whether it ended up hittable.
    @MainActor
    @discardableResult
    func scrollUntilVisible(_ app: XCUIApplication, _ element: XCUIElement, attempts: Int = 10) -> Bool {
        var swipes = 0
        while !element.isHittable && swipes < attempts {
            app.swipeUp()
            swipes += 1
        }
        return element.isHittable
    }

    /// First element of ANY type whose label contains `fragment` — markdown
    /// bodies surface as TextViews rather than StaticTexts, so a
    /// `staticTexts[...]` lookup would miss them.
    @MainActor
    func anyElement(_ app: XCUIApplication, containing fragment: String) -> XCUIElement {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", fragment)
        ).firstMatch
    }

    /// First element of ANY type carrying `identifier` (SwiftUI puts the same
    /// identifier on several element types depending on the container).
    @MainActor
    func anyElement(_ app: XCUIApplication, identified identifier: String) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    /// The Guide body is a LazyVStack: a row below the fold is not in the
    /// accessibility tree until it scrolls on screen. Swipe up until one of
    /// `rows` exists or the deadline passes (a loading Guide just keeps
    /// waiting; the swipes are harmless on a short page).
    @MainActor
    func revealGuideRow(_ app: XCUIApplication, _ rows: [XCUIElement], deadline: Date) {
        while Date() < deadline && !rows.contains(where: { $0.exists }) {
            if rows.contains(where: { $0.waitForExistence(timeout: 2) }) { return }
            app.swipeUp()
        }
    }

    /// EXP-1251: a review lands on the Guide, which draws Changes rows, never
    /// file cards. Open the COMPLETE diff page ("Changes", section=all, the
    /// catalog's review-diff view, as web) — the Guide's last row, under every
    /// section — or, on a report-less Guide that has no such row, its ONE
    /// Changes section, which already is the whole diff. Then wait for the
    /// file cards, which come from GitHub.
    ///
    /// EXP-1267: both rows are awaited TOGETHER. Waiting for the complete-diff
    /// row first cost every report-less Guide (the seeded APP-14) its full
    /// deadline before the Changes row was even looked at.
    @MainActor
    func openGuideDiffPage(_ app: XCUIApplication, failure: String) {
        let completeDiff = anyElement(app, identified: "guide-show-complete-diff")
        let changesRow = anyElement(app, identified: "guide-changes-row")
        revealGuideRow(app, [completeDiff, changesRow], deadline: Date().addingTimeInterval(Wait.network))
        // The complete-diff row is the Guide's LAST: a Changes row on screen
        // may still have it one swipe below.
        if changesRow.exists && !completeDiff.exists {
            revealGuideRow(app, [completeDiff], deadline: Date().addingTimeInterval(3))
        }
        if completeDiff.exists {
            completeDiff.tap()
        } else {
            guard changesRow.exists else {
                XCTFail("\(failure) — missing: \(changesRow.description)")
                return
            }
            changesRow.tap()
        }
        expect(anyElement(app, identified: "guide-section-page"), within: Wait.nav, "The Guide section page did not open")
        expect(anyElement(app, identified: "changes-file-row"), within: Wait.network, failure)
    }

    /// Dismisses a sheet. Since EXP-687 NO sheet has a close button, so this is
    /// the only way out of one: tap the dimmed area above a short sheet, then
    /// fall back to dragging the sheet's own header down — never a plain
    /// `swipeDown`, which a full-height sheet's Form just scrolls. `anchor` is
    /// an element that only exists while the sheet is up.
    @MainActor
    func dismissSheet(_ app: XCUIApplication, whileVisible anchor: XCUIElement) {
        guard anchor.exists else { return }
        // Key everything on the anchor's frame: a fitted sheet opens well below
        // any fixed screen fraction, so a tap/drag at a fixed height would land
        // on the dimming view (harmless) or the status bar (dead). First an
        // outside tap just above the sheet's edge, then drags from the anchor
        // itself (a header text, or the top of a sheet container) downwards.
        let screenHeight = app.frame.height
        let aboveSheet = max(anchor.frame.minY - 40, 60) / screenHeight
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: aboveSheet)).tap()
        for _ in 0..<3 {
            if !anchor.exists { return }
            let grip = anchor.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.02))
            let bottom = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.95))
            grip.press(forDuration: 0.05, thenDragTo: bottom)
            usleep(600_000)
        }
    }
}
