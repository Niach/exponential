import XCTest

/// Automated App Store screenshots (fastlane snapshot).
///
/// Drives the real app against a seeded local backend: sign in on the
/// InstanceView/LoginView flow, wait for Electric to sync the demo team,
/// then capture the store shots (board, issue detail, comments, board
/// switcher, inbox, agents, support inbox, search, create issue). Run via
/// `fastlane screenshots` (apps/ios) with the seeded dev server running
/// (`apps/web/scripts/seed-screenshots.ts` — demo@exponential.at /
/// screenshots-demo, team "Acme", board "Mobile App", showcase issue APP-5,
/// live coding sessions, helpdesk threads).
///
/// The instance URL defaults to http://localhost:5173 and can be overridden
/// with the SNAPSHOT_INSTANCE_URL environment variable.
final class StoreScreenshots: XCTestCase {

    private static let demoEmail = "demo@exponential.at"
    private static let demoPassword = "screenshots-demo"
    private static let showcaseTitle = "Reduce cold start below 800 ms"
    private static let showcaseIdentifier = "APP-5"

    @MainActor
    func testCaptureAppStoreScreenshots() throws {
        continueAfterFailure = false

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

        signIn(app)

        // Wait for the board: Electric sync can take a while right after the
        // first login. 01_board itself is captured LAST — the save-password
        // sheet pops at an unpredictable moment several seconds after login
        // (it photobombed the iPad board shot twice); by the end of the run
        // it has provably appeared and been dismissed.
        let showcaseRowTitle = app.staticTexts[Self.showcaseTitle]
        XCTAssertTrue(
            showcaseRowTitle.waitForExistence(timeout: 120),
            "Issue list never synced (missing showcase issue \(Self.showcaseIdentifier))"
        )
        dismissSavePasswordSheet(timeout: 3)

        // ── 02: issue detail (APP-5) ────────────────────────────────────────
        // The detail ScrollView renders its whole content tree, so the comment
        // header existing (even offscreen) means the detail is fully loaded.
        // Retry the tap: a late springboard sheet can swallow the first one.
        let showcaseRow = app.buttons["issue-row-\(Self.showcaseIdentifier)"]
        let commentsHeader = app.staticTexts["comment-thread-header"]
        var detailOpened = false
        for _ in 0..<3 {
            if showcaseRow.waitForExistence(timeout: 10) {
                showcaseRow.tap()
            } else {
                showcaseRowTitle.tap()
            }
            // 30s per attempt: the comment thread renders only once the
            // comments shape has synced, which can lag the issues shape by
            // tens of seconds right after the first login.
            if commentsHeader.waitForExistence(timeout: 30) {
                detailOpened = true
                break
            }
            dismissSavePasswordSheet(timeout: 2)
        }
        XCTAssertTrue(detailOpened, "Issue detail did not open")
        settle(2)
        snapshot("02_issue-detail")

        // ── 03: comment thread ──────────────────────────────────────────────
        // Skip when the comments are already on screen (iPad: the 13" detail
        // shows the whole thread without scrolling, so 03 would duplicate 02).
        if !commentsHeader.isHittable {
            var scrollAttempts = 0
            while !commentsHeader.isHittable && scrollAttempts < 12 {
                app.swipeUp()
                scrollAttempts += 1
            }
            settle(2)
            snapshot("03_comments")
        }

        // ── 04: board switcher ────────────────────────────────────────────
        // The sheet shows each board's tinted icon glyph next to its name
        // and prefix.
        goBack(app)
        XCTAssertTrue(showcaseRowTitle.waitForExistence(timeout: 20), "Did not return to the board")
        let switcherButton = app.buttons["Switch board"]
        XCTAssertTrue(switcherButton.waitForExistence(timeout: 10), "Board switcher control missing")
        switcherButton.tap()
        XCTAssertTrue(
            app.staticTexts["Product Feedback"].waitForExistence(timeout: 15),
            "Board switcher sheet did not show the seeded boards"
        )
        settle(2)
        snapshot("04_boards")

        // Dismiss without selecting (keep Mobile App current): tap the dimmed
        // area above the medium-detent sheet.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.12)).tap()

        // ── 05: inbox (My Work tab, Inbox segment — the default) ────────────
        // Wait for a real notification group — capturing the "You're all
        // caught up" empty state would silently ship an empty store shot.
        let inboxTab = app.buttons["tab-mywork"]
        XCTAssertTrue(inboxTab.waitForExistence(timeout: 15), "Tab bar missing after sheet dismissal")
        inboxTab.tap()
        XCTAssertTrue(
            app.staticTexts[Self.showcaseTitle].firstMatch.waitForExistence(timeout: 60),
            "Inbox never showed the seeded notifications"
        )
        settle(2)
        snapshot("05_inbox")

        // ── 06: agents (live coding sessions) ───────────────────────────────
        // The seed inserts a running session on APP-5 and an in-review session
        // with an open PR, both with a fresh heartbeat (the clients hide
        // sessions past the staleness window).
        let agentsTab = app.buttons["tab-agents"]
        XCTAssertTrue(agentsTab.waitForExistence(timeout: 15), "Agents tab missing")
        agentsTab.tap()
        let sessionRow = app.descendants(matching: .any)
            .matching(identifier: "agent-session-row").firstMatch
        XCTAssertTrue(
            sessionRow.waitForExistence(timeout: 60),
            "Agents tab never showed the seeded coding sessions"
        )
        settle(2)
        snapshot("06_agents")

        // ── 07: support inbox (helpdesk threads) ────────────────────────────
        // The tab only exists because the seed flips the team's
        // helpdesk_enabled on; threads come from tRPC polling, not Electric.
        let supportTab = app.buttons["tab-support"]
        XCTAssertTrue(
            supportTab.waitForExistence(timeout: 15),
            "Support tab missing — did the seed enable the team helpdesk?"
        )
        supportTab.tap()
        let threadRow = app.descendants(matching: .any)
            .matching(identifier: "support-thread-row").firstMatch
        XCTAssertTrue(
            threadRow.waitForExistence(timeout: 30),
            "Support inbox never showed the seeded threads"
        )
        settle(2)
        snapshot("07_support")

        // ── 08: search ──────────────────────────────────────────────────────
        app.buttons["tab-search"].tap()
        let searchField = app.textFields["search-field"]
        XCTAssertTrue(searchField.waitForExistence(timeout: 15), "Search field missing")
        focus(searchField)
        searchField.typeText("issue")
        XCTAssertTrue(
            app.staticTexts["Offline queue for issue edits"]
                .waitForExistence(timeout: 30),
            "Search results never appeared"
        )
        settle(2)
        snapshot("08_search")

        // ── 09: create issue ────────────────────────────────────────────────
        app.buttons["tab-issues"].tap()
        XCTAssertTrue(
            showcaseRowTitle.waitForExistence(timeout: 15),
            "Board did not come back before the compose capture"
        )
        let composeButton = app.buttons["compose-button"]
        XCTAssertTrue(composeButton.waitForExistence(timeout: 10), "Compose button missing")
        composeButton.tap()
        let titleField = app.textFields["issue-title-field"]
        XCTAssertTrue(titleField.waitForExistence(timeout: 10), "Create-issue sheet did not open")
        focus(titleField)
        titleField.typeText("Weekly summary email digest")
        settle(2)
        snapshot("09_new-issue")
        app.buttons["Cancel"].tap()

        // ── 01: home issue list (captured last, see above) ──────────────────
        app.buttons["tab-issues"].tap()
        XCTAssertTrue(
            showcaseRowTitle.waitForExistence(timeout: 15),
            "Board did not come back for the final capture"
        )
        dismissSavePasswordSheet(timeout: 2)
        settle(2)
        snapshot("01_board")
    }

    // MARK: - Sign in

    @MainActor
    private func signIn(_ app: XCUIApplication) {
        let instanceUrl = ProcessInfo.processInfo.environment["SNAPSHOT_INSTANCE_URL"]
            ?? "http://localhost:5173"

        // InstanceView: replace the prefilled "https://" with the target URL.
        // On a retry after a partially-successful run the keychain account
        // survives the relaunch and the app boots straight into the main UI —
        // detect that and skip the sign-in flow entirely.
        let urlField = app.textFields["instance-url-field"]
        let selfHostLink = app.buttons["instance-self-host-link"]
        let issuesTab = app.buttons["tab-issues"]
        let deadline = Date().addingTimeInterval(30)
        while !selfHostLink.exists && !urlField.exists && !issuesTab.exists && Date() < deadline {
            usleep(500_000)
        }
        if issuesTab.exists {
            dismissSavePasswordSheet(timeout: 2)
            return
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
        focus(emailField)
        emailField.typeText(Self.demoEmail)

        // Plain textField (not secureTextField): under -uiTesting the app
        // renders the password field unsecured so the system save-password
        // sheet can never appear (see LoginView.glassTextField).
        let passwordField = app.textFields["login-password-field"]
        XCTAssertTrue(passwordField.waitForExistence(timeout: 10))
        focus(passwordField)
        passwordField.typeText(Self.demoPassword)

        let signInButton = app.buttons["login-submit-button"]
        XCTAssertTrue(signInButton.waitForExistence(timeout: 10))
        signInButton.tap()

        // iOS offers to save the password into the keychain right after a
        // SecureField submit — a springboard sheet that photobombs the first
        // capture (and blocks every later tap). Give it time to animate in.
        dismissSavePasswordSheet(timeout: 8)
    }

    /// Dismisses the springboard "Save Password?" sheet if it shows up within
    /// `timeout`, in whatever language the simulator speaks.
    @MainActor
    private func dismissSavePasswordSheet(timeout: TimeInterval) {
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
    private func focus(_ element: XCUIElement) {
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
    private func clearText(of element: XCUIElement) {
        guard let current = element.value as? String, !current.isEmpty else { return }
        let deletes = String(repeating: XCUIKeyboardKey.delete.rawValue, count: current.count + 2)
        element.typeText(deletes)
    }

    /// Pops the top view controller off the navigation stack (leading nav-bar
    /// back button).
    @MainActor
    private func goBack(_ app: XCUIApplication) {
        let backButton = app.navigationBars.buttons.firstMatch
        if backButton.waitForExistence(timeout: 10) {
            backButton.tap()
        }
    }

    /// Give in-flight animations (and Electric row inserts) a moment to settle
    /// before capturing.
    @MainActor
    private func settle(_ seconds: UInt32) {
        sleep(seconds)
    }
}
