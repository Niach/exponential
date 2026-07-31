import XCTest

/// Automated App Store screenshots (fastlane snapshot).
///
/// Drives the real app against a seeded local backend: sign in on the
/// InstanceView/LoginView flow, wait for Electric to sync the demo team, then
/// capture the eight store shots — board, issue detail, Start-coding dialog,
/// live steering, PR review, actions, inbox, support inbox. EXP-393 replaced
/// the comments / board-switcher / agents-list / reviews-list / search shots
/// with the three that actually differentiate the product (start coding,
/// steering, diff + merge), and kept the set identical to Android's, where
/// Play caps phone screenshots at 8.
///
/// Run via `fastlane screenshots` (apps/ios). Prerequisites, in order:
///   1. seeded dev server — `apps/web/scripts/seed-screenshots.ts`
///      (demo@exponential.at / screenshots-demo, team "Acme", board
///      "Mobile App", showcase issue APP-5, open PRs, actions, helpdesk)
///   2. a steer relay, with STEER_RELAY_URL + STEER_RELAY_SECRET exported for
///      the web server (`docker compose --profile steer up -d`)
///   3. `bun run screenshots:desktop` left running for the whole capture
/// Without 2+3 the instance reports steering disabled: the Start-coding and
/// steering shots are unreachable and the issue detail renders "Live steering
/// is unavailable on this instance." — which is what EXP-393 set out to fix.
///
/// The instance URL defaults to http://localhost:5173 and can be overridden
/// with the SNAPSHOT_INSTANCE_URL environment variable.
final class StoreScreenshots: XCTestCase {

    private static let demoEmail = "demo@exponential.at"
    private static let demoPassword = "screenshots-demo"
    private static let showcaseTitle = "Reduce cold start below 800 ms"
    private static let showcaseIdentifier = "APP-5"
    /// APP-3: repo-backed, member-assigned, and deliberately WITHOUT a coding
    /// session of its own — an issue the demo user is already coding on turns
    /// the bottom-bar circle into the session link instead of the start action.
    private static let startCodingTitle = "Dark mode contrast pass across settings"
    /// APP-14: the open PR whose diff is fetched from GitHub for real.
    private static let reviewTitle = "Group board issues by assignee"
    /// A fragment of the question screenshot-desktop.ts ends its transcript on
    /// (DEMO_FEED_QUESTION) — proof that relay frames actually arrived. It has
    /// to be the LAST event: the feed is bottom-anchored and lazy, so anything
    /// above the fold is never rendered and no query can find it.
    private static let feedQuestionFragment = "Lazy-load the markdown editor too"

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
        //
        // Tap the row's TITLE text, never the `issue-row-*` button element:
        // since the glass chip rework the row element's accessibility
        // activation point lands on the leading priority control, so an
        // element tap opens the priority picker sheet instead of navigating
        // (EXP-348 — it silently killed all three snapshot retries).
        let commentsHeader = app.staticTexts["comment-thread-header"]
        var detailOpened = false
        for _ in 0..<3 {
            if showcaseRowTitle.exists && showcaseRowTitle.isHittable {
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
        if !detailOpened {
            print("EXP-DEBUG hierarchy after failed detail open:\n\(app.debugDescription)")
        }
        XCTAssertTrue(detailOpened, "Issue detail did not open")
        // The live session row sits above the comment thread — it is what makes
        // this shot say "an agent is coding on this right now" rather than
        // "live steering is unavailable on this instance".
        XCTAssertTrue(
            app.staticTexts["Coding now"].firstMatch.waitForExistence(timeout: 30),
            "No live session on \(Self.showcaseIdentifier) — is screenshots:desktop running against the relay?"
        )
        settle(2)
        snapshot("02_issue-detail")

        // ── 04: live steering ───────────────────────────────────────────────
        // The bottom-bar circle turns into the session link once the demo user
        // has a live session of their own (EXP-312 — owner-only). Wait for the
        // FEED, not just the screen: it renders "Connecting…" / "Waiting for
        // activity…" placeholders until the first relay frame lands.
        let sessionLink = app.buttons["Coding session"]
        XCTAssertTrue(sessionLink.waitForExistence(timeout: 15), "Session link missing on the issue detail")
        sessionLink.tap()
        let agentFeed = app.descendants(matching: .any)
            .matching(identifier: "agent-feed").firstMatch
        XCTAssertTrue(
            agentFeed.waitForExistence(timeout: 60),
            "The steering feed never appeared — is screenshots:desktop publishing to the relay?"
        )
        // An EMPTY feed still renders the container (a dropped relay socket
        // leaves the view "Reconnecting…" with nothing in it), so the tag alone
        // would happily photograph a blank screen. Gate on real content —
        // matched across every element type, since markdown-rendered feed rows
        // surface as TextViews rather than StaticTexts.
        let feedQuestion = app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", Self.feedQuestionFragment)
        ).firstMatch
        XCTAssertTrue(
            feedQuestion.waitForExistence(timeout: 60),
            "The relay never replayed the transcript — is STEER_RELAY_URL reachable from the simulator?"
        )
        settle(3)
        snapshot("04_steering")
        goBack(app)

        // ── 03: Start-coding dialog ─────────────────────────────────────────
        // From a repo-backed issue the demo user is NOT already coding on, so
        // the circle offers the start action. The dialog needs an online
        // desktop: without one it refuses to open and shows a notice instead.
        goBack(app)
        XCTAssertTrue(showcaseRowTitle.waitForExistence(timeout: 20), "Did not return to the board")
        openIssue(app, title: Self.startCodingTitle)
        let startButton = app.buttons["Start coding"]
        XCTAssertTrue(startButton.waitForExistence(timeout: 20), "Start-coding control missing")
        startButton.tap()
        let startSheet = app.descendants(matching: .any)
            .matching(identifier: "start-coding-sheet").firstMatch
        XCTAssertTrue(
            startSheet.waitForExistence(timeout: 20),
            "Start-coding dialog did not open — is a desktop online on the relay?"
        )
        settle(2)
        snapshot("03_start-coding")
        app.buttons["Cancel"].firstMatch.tap()
        goBack(app)

        // ── 05: PR review (real diff + merge bar) ───────────────────────────
        // The Reviews tab rows open the Changes page directly. The file list
        // comes from GitHub via issues.prFiles — the seed points APP-14 at a
        // real public PR so there is an actual diff to show.
        let reviewsTab = app.buttons["tab-reviews"]
        XCTAssertTrue(reviewsTab.waitForExistence(timeout: 15), "Reviews tab missing")
        reviewsTab.tap()
        let reviewRow = app.staticTexts[Self.reviewTitle].firstMatch
        XCTAssertTrue(
            reviewRow.waitForExistence(timeout: 60),
            "Reviews tab never showed the seeded open PRs"
        )
        reviewRow.tap()
        let fileRows = app.descendants(matching: .any).matching(identifier: "changes-file-row")
        XCTAssertTrue(
            fileRows.firstMatch.waitForExistence(timeout: 60),
            "The PR diff never loaded — check SCREENSHOT_PR_URL is a reachable public PR"
        )
        // Every file starts collapsed; expand them so the shot shows patches
        // rather than a bare filename list.
        for index in 0..<min(fileRows.count, 5) {
            let row = fileRows.element(boundBy: index)
            if row.exists && row.isHittable { row.tap() }
        }
        settle(2)
        snapshot("05_review")
        goBack(app)

        // ── 06: actions (EXP-253) — the seed inserts three team actions above
        // the two client-side builtins. Reached from the Agents toolbar.
        let agentsTab = app.buttons["tab-agents"]
        XCTAssertTrue(agentsTab.waitForExistence(timeout: 15), "Agents tab missing")
        agentsTab.tap()
        let actionsLink = app.buttons["Actions"]
        XCTAssertTrue(actionsLink.waitForExistence(timeout: 15), "Actions toolbar entry missing")
        actionsLink.tap()
        let actionRow = app.descendants(matching: .any)
            .matching(identifier: "action-row").firstMatch
        XCTAssertTrue(
            actionRow.waitForExistence(timeout: 30),
            "Actions list never showed the seeded actions"
        )
        XCTAssertTrue(
            app.staticTexts["Update dependencies"].firstMatch.waitForExistence(timeout: 30),
            "Seeded team actions never synced"
        )
        settle(2)
        snapshot("06_actions")
        goBack(app)

        // ── 07: inbox (My Work tab, Inbox segment — the default) ────────────
        // Wait for a real notification group — capturing the "You're all
        // caught up" empty state would silently ship an empty store shot.
        let inboxTab = app.buttons["tab-mywork"]
        XCTAssertTrue(inboxTab.waitForExistence(timeout: 15), "My Work tab missing")
        inboxTab.tap()
        XCTAssertTrue(
            app.staticTexts[Self.showcaseTitle].firstMatch.waitForExistence(timeout: 60),
            "Inbox never showed the seeded notifications"
        )
        settle(2)
        snapshot("07_inbox")

        // ── 08: support inbox (helpdesk threads) ────────────────────────────
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
        snapshot("08_support")

        // ── 01: home issue list (captured last, see above) ──────────────────
        app.buttons["tab-issues"].tap()
        XCTAssertTrue(
            showcaseRowTitle.waitForExistence(timeout: 15),
            "Board did not come back for the final capture"
        )
        // Opening an issue earlier may have scrolled the list; the board shot
        // has to start at the top of the first group.
        for _ in 0..<3 { app.swipeDown() }
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

    /// Opens an issue from the board by its title, scrolling the list until the
    /// row is on screen.
    ///
    /// Taps the row's TITLE text, never the `issue-row-*` button element: since
    /// the glass chip rework the row element's accessibility activation point
    /// lands on the leading priority control, so an element tap opens the
    /// priority picker sheet instead of navigating (EXP-348).
    @MainActor
    private func openIssue(_ app: XCUIApplication, title: String) {
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
