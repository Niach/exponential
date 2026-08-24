import XCTest

/// Shared drive-the-app machinery for the two screenshot suites
/// (`StoreScreenshots` — the 8 App Store shots — and `StyleguideScreenshots` —
/// the cross-platform styleguide set).
///
/// Everything here is a pure mechanical extraction of what `StoreScreenshots`
/// grew over EXP-348/EXP-393: launching with the UI-testing flag + a system
/// interruption monitor, the InstanceView → LoginView sign-in flow, the
/// springboard save-password sheet dismissal, and the tap/scroll/settle
/// helpers. No suite may fork this behavior — both capture against the same
/// seeded backend (`apps/web/scripts/seed-screenshots.ts`).

/// Credentials + instance target of the seeded demo backend.
enum ScreenshotSeed {
    static let demoEmail = "demo@exponential.at"
    static let demoPassword = "screenshots-demo"

    /// The instance URL defaults to http://localhost:5173 and can be overridden
    /// with the SNAPSHOT_INSTANCE_URL environment variable (bridged through
    /// TEST_RUNNER_SNAPSHOT_INSTANCE_URL by the Snapfile — xcodebuild only
    /// forwards TEST_RUNNER_-prefixed variables into the runner process).
    static var instanceUrl: String {
        ProcessInfo.processInfo.environment["SNAPSHOT_INSTANCE_URL"]
            ?? "http://localhost:5173"
    }
}

/// Where the app landed after the instance URL was accepted.
enum SignInStage {
    /// A keychain account survived the relaunch and the app booted straight
    /// into the main UI — the sign-in flow is a no-op.
    case alreadySignedIn
    /// LoginView is on screen with its email field ready.
    case loginReady
}

/// Where a COLD launch landed, before anything has been tapped.
enum LaunchStage {
    /// InstanceView is up (the cloud buttons + the self-hosted link, or the
    /// bare URL field when the cloud account already exists).
    case instancePicker
    /// A keychain account survived the relaunch — the main UI is already up.
    case alreadySignedIn
}

extension XCTestCase {

    // MARK: - Launch

    /// Builds the app under test with the screenshot flags and the belt-and-braces
    /// system-alert monitor, hands it to fastlane snapshot, and launches it.
    @MainActor
    func launchScreenshotApp() -> XCUIApplication {
        let app = XCUIApplication()
        // -uiTesting suppresses the push-permission request (AppDependencies)
        // so no system alert ever sits on top of a capture.
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

        setupSnapshot(app)
        app.launch()
        return app
    }

    // MARK: - Sign in

    /// Waits out the cold launch WITHOUT touching anything and reports what is
    /// on screen. Split out of `presentLoginScreen` (EXP-566) so the styleguide
    /// suite can photograph the untouched instance picker before the sign-in
    /// flow starts driving it.
    @MainActor
    func awaitLaunchStage(_ app: XCUIApplication, timeout: TimeInterval = 30) -> LaunchStage {
        let urlField = app.textFields["instance-url-field"]
        let selfHostLink = app.buttons["instance-self-host-link"]
        let issuesTab = app.buttons["tab-issues"]
        let deadline = Date().addingTimeInterval(timeout)
        while !selfHostLink.exists && !urlField.exists && !issuesTab.exists && Date() < deadline {
            usleep(500_000)
        }
        return issuesTab.exists ? .alreadySignedIn : .instancePicker
    }

    /// InstanceView: replace the prefilled "https://" with the target URL and
    /// continue, leaving LoginView on screen (or reporting that the app booted
    /// straight into the main UI).
    ///
    /// On a retry after a partially-successful run the keychain account
    /// survives the relaunch and the app boots straight into the main UI —
    /// detect that and skip the sign-in flow entirely.
    @MainActor
    @discardableResult
    func presentLoginScreen(_ app: XCUIApplication) -> SignInStage {
        let instanceUrl = ScreenshotSeed.instanceUrl

        let urlField = app.textFields["instance-url-field"]
        let selfHostLink = app.buttons["instance-self-host-link"]
        if awaitLaunchStage(app) == .alreadySignedIn {
            dismissSavePasswordSheet(timeout: 2)
            return .alreadySignedIn
        }
        // Cloud is the primary path now (EXP-14) — reveal the self-hosted URL
        // field before pointing the app at the local backend.
        if selfHostLink.exists && !urlField.exists {
            selfHostLink.tap()
        }
        XCTAssertTrue(urlField.waitForExistence(timeout: 5), "Neither InstanceView nor the main UI appeared")
        focus(urlField)
        clearText(of: urlField)
        urlField.typeText(instanceUrl)

        let continueButton = app.buttons["instance-continue-button"]
        XCTAssertTrue(continueButton.waitForExistence(timeout: 10))
        continueButton.tap()

        // LoginView appears once /api/auth-config resolves.
        let emailField = app.textFields["login-email-field"]
        XCTAssertTrue(emailField.waitForExistence(timeout: 30), "Login email field never appeared — is the backend running at \(instanceUrl)?")
        return .loginReady
    }

    /// Fills in the demo credentials on an already-visible LoginView and submits.
    @MainActor
    func submitLogin(_ app: XCUIApplication) {
        let emailField = app.textFields["login-email-field"]
        XCTAssertTrue(emailField.waitForExistence(timeout: 30), "Login email field never appeared")
        focus(emailField)
        emailField.typeText(ScreenshotSeed.demoEmail)

        // Plain textField (not secureTextField): under -uiTesting the app
        // renders the password field unsecured so the system save-password
        // sheet can never appear (see LoginView.glassTextField).
        let passwordField = app.textFields["login-password-field"]
        XCTAssertTrue(passwordField.waitForExistence(timeout: 10))
        focus(passwordField)
        passwordField.typeText(ScreenshotSeed.demoPassword)

        let signInButton = app.buttons["login-submit-button"]
        XCTAssertTrue(signInButton.waitForExistence(timeout: 10))
        signInButton.tap()

        // iOS offers to save the password into the keychain right after a
        // SecureField submit — a springboard sheet that photobombs the first
        // capture (and blocks every later tap). Give it time to animate in.
        dismissSavePasswordSheet(timeout: 8)
    }

    /// The full InstanceView → LoginView → main UI sign-in flow.
    @MainActor
    func signIn(_ app: XCUIApplication) {
        guard presentLoginScreen(app) == .loginReady else { return }
        submitLogin(app)
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
        XCTAssertTrue(row.isHittable, "Issue \"\(title)\" never became tappable on the board")
        row.tap()
    }

    /// Pops the top view controller off the navigation stack (leading nav-bar
    /// back button).
    @MainActor
    func goBack(_ app: XCUIApplication) {
        let backButton = app.navigationBars.buttons.firstMatch
        if backButton.waitForExistence(timeout: 10) {
            backButton.tap()
        }
    }

    /// Give in-flight animations (and Electric row inserts) a moment to settle
    /// before capturing.
    @MainActor
    func settle(_ seconds: UInt32) {
        sleep(seconds)
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

    /// Dismisses a detent sheet that has no close button of its own (the issue
    /// filter sheet): tap the dimmed area above it, then fall back to a swipe
    /// down. `anchor` is an element that only exists while the sheet is up.
    @MainActor
    func dismissSheet(_ app: XCUIApplication, whileVisible anchor: XCUIElement) {
        guard anchor.exists else { return }
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.06)).tap()
        for _ in 0..<3 {
            if !anchor.exists { return }
            app.swipeDown(velocity: .fast)
            usleep(500_000)
        }
    }
}
