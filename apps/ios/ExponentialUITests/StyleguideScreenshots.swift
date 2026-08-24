import XCTest

/// Styleguide screenshots (fastlane snapshot, `fastlane styleguide_screenshots`).
///
/// A SECOND capture suite next to StoreScreenshots: where the store set sells
/// the product in eight slides, this one photographs the SURFACES — the shots a
/// cross-platform design review compares side by side against the web, desktop
/// and Android captures of the same eleven screens.
///
/// The eleven names below are a CROSS-PLATFORM CONTRACT and byte-exact: the
/// other clients emit the same `sg_*` basenames so the compositor can pair them
/// up. Never rename or reorder them without changing every client.
///
///   sg_sign-in · sg_board-filters · sg_issue-comments · sg_issue-create ·
///   sg_search · sg_my-issues · sg_agents · sg_reviews · sg_support-thread ·
///   sg_settings-team · sg_settings-personal
///
/// Same prerequisites as StoreScreenshots — a seeded dev server
/// (`apps/web/scripts/seed-screenshots.ts`: demo@exponential.at /
/// screenshots-demo, team "Acme", board "Mobile App", showcase issue APP-5,
/// open PRs, actions, helpdesk threads). Unlike the store suite this one does
/// NOT need a live relay or an online desktop: sg_agents settles for whatever
/// the Agents surface shows, so the run stays green without
/// `screenshots:desktop`.
///
/// Every shot is gated on real seeded content, never on a container element —
/// an empty list still renders its container and would silently ship a blank
/// styleguide page. Shared launch/sign-in/tap helpers live in
/// ScreenshotFlow.swift.
final class StyleguideScreenshots: XCTestCase {

    private static let teamName = "Acme"
    /// APP-5 — the showcase issue: the only one the seed gives a comment thread
    /// (four comments, incl. an `@mention` + `#APP-2` issue ref).
    private static let showcaseTitle = "Reduce cold start below 800 ms"
    /// First comment on APP-5, plain text before any markdown decoration.
    private static let showcaseCommentFragment = "Profiled on a mid-range device"
    /// Assigned to the demo user, so it is on My Issues.
    private static let myIssueTitle = "Dark mode contrast pass across settings"
    /// One of the four seeded open PRs on the Reviews queue.
    private static let reviewTitle = "Batch-edit labels from the board"
    private static let searchQuery = "cold start"
    /// The seeded helpdesk thread from Emma Fischer, and the last inbound
    /// message on it — proof the thread body actually loaded over tRPC.
    private static let supportThreadTitle = "Can't sign in on the iPad app"
    private static let supportReporter = "Emma Fischer"
    private static let supportReplyFragment = "thank you for the quick turnaround"
    /// A seeded board — the anchor that says we are on TEAM settings rather
    /// than the outer Settings screen (both carry the nav title "Settings").
    private static let seededBoardName = "Mobile App"

    @MainActor
    func testCaptureStyleguideScreenshots() throws {
        continueAfterFailure = false

        let app = launchScreenshotApp()

        // ── sg_sign-in: the login screen itself ─────────────────────────────
        // Captured after the instance URL is accepted and BEFORE any
        // credentials are typed, so the shot shows the empty email/password
        // form rather than a half-filled one.
        //
        // The Snapfile erases the simulator, so the app always boots onto
        // InstanceView. If a keychain account somehow survived, the app comes
        // up already signed in and there is no login screen to photograph —
        // say so loudly and carry on with the other ten.
        let stage = presentLoginScreen(app)
        if stage == .loginReady {
            settle(1)
            snapshot("sg_sign-in")
            submitLogin(app)
        } else {
            print("EXP-566 sg_sign-in SKIPPED: the app booted already signed in (stale keychain — is erase_simulator on?)")
        }

        // Wait for Electric to sync the board; the first login can take a while.
        let showcaseRowTitle = app.staticTexts[Self.showcaseTitle]
        XCTAssertTrue(
            showcaseRowTitle.waitForExistence(timeout: 120),
            "Issue list never synced (missing showcase issue \(Self.showcaseTitle))"
        )
        dismissSavePasswordSheet(timeout: 3)

        // ── sg_board-filters: the board's filter sheet ───────────────────────
        // The trigger is the nav-bar "Filters" toolbar item; the sheet headline
        // carries the SAME string, so scope the tap to the navigation bar and
        // the assertion to staticTexts.
        let filtersButton = app.navigationBars.buttons["Filters"]
        XCTAssertTrue(filtersButton.waitForExistence(timeout: 20), "Filters toolbar item missing on the board")
        filtersButton.tap()
        let filterSheetHeadline = app.staticTexts["Filters"]
        XCTAssertTrue(filterSheetHeadline.waitForExistence(timeout: 15), "Filter sheet did not open")
        // Its three category rows are the real content — the sheet chrome alone
        // renders even before the team's statuses/labels have synced.
        XCTAssertTrue(
            app.buttons["Status"].waitForExistence(timeout: 15),
            "Filter sheet never showed its categories"
        )
        settle(2)
        snapshot("sg_board-filters")
        dismissSheet(app, whileVisible: filterSheetHeadline)

        // ── sg_issue-comments: APP-5 scrolled to its comment thread ──────────
        // Tap the row's TITLE text, never the `issue-row-*` element (EXP-348).
        XCTAssertTrue(showcaseRowTitle.waitForExistence(timeout: 20), "Did not return to the board")
        openIssue(app, title: Self.showcaseTitle)
        let commentsHeader = app.staticTexts["comment-thread-header"]
        XCTAssertTrue(commentsHeader.waitForExistence(timeout: 60), "Issue detail did not open")
        // The comments shape can lag the issues shape by tens of seconds right
        // after the first login — gate on a real comment body, not the header.
        XCTAssertTrue(
            anyElement(app, containing: Self.showcaseCommentFragment).waitForExistence(timeout: 60),
            "The comment thread on APP-5 never synced"
        )
        // The detail is one long ScrollView with no scroll-to anchor; walk down
        // until the "Activity" header is on screen, then two more swipes so the
        // thread — not the header — fills the frame.
        scrollUntilVisible(app, commentsHeader, attempts: 14)
        app.swipeUp()
        app.swipeUp()
        settle(2)
        snapshot("sg_issue-comments")
        goBack(app)

        // ── sg_issue-create: the new-issue sheet ────────────────────────────
        // The compose button is only mounted on board routes (AppNavigator
        // `resolvedComposeTarget`), so come back to the issues tab first. The
        // title field takes focus on appear, so the sheet is captured with the
        // keyboard up — which is the state a user actually sees.
        app.buttons["tab-issues"].tap()
        XCTAssertTrue(showcaseRowTitle.waitForExistence(timeout: 20), "Board did not come back for the compose shot")
        let composeButton = app.buttons["compose-button"]
        XCTAssertTrue(composeButton.waitForExistence(timeout: 15), "Compose button missing on the board")
        composeButton.tap()
        let titleField = app.textFields["issue-title-field"]
        XCTAssertTrue(titleField.waitForExistence(timeout: 15), "Create-issue sheet did not open")
        focus(titleField)
        titleField.typeText("Prefetch avatars before the first board paint")
        settle(2)
        snapshot("sg_issue-create")
        // Cancel — the styleguide run must not write anything to the seed.
        app.buttons["Cancel"].firstMatch.tap()

        // ── sg_search: the search view with seeded results ───────────────────
        let searchTab = app.buttons["tab-search"]
        XCTAssertTrue(searchTab.waitForExistence(timeout: 15), "Search tab missing")
        searchTab.tap()
        let searchField = app.textFields["search-field"]
        XCTAssertTrue(searchField.waitForExistence(timeout: 15), "Search field missing")
        focus(searchField)
        searchField.typeText(Self.searchQuery)
        XCTAssertTrue(
            app.staticTexts[Self.showcaseTitle].firstMatch.waitForExistence(timeout: 30),
            "Search never returned the seeded issue for \"\(Self.searchQuery)\""
        )
        settle(2)
        snapshot("sg_search")

        // ── sg_my-issues: My Work → the "My Issues" segment ──────────────────
        // The segment is a GlassSegmentedControl button carrying only an
        // accessibility LABEL. Its choice is persisted in @AppStorage, so the
        // tap is deliberately unconditional (it is idempotent).
        let myWorkTab = app.buttons["tab-mywork"]
        XCTAssertTrue(myWorkTab.waitForExistence(timeout: 15), "My Work tab missing")
        myWorkTab.tap()
        let myIssuesSegment = app.buttons["My Issues"]
        XCTAssertTrue(myIssuesSegment.waitForExistence(timeout: 15), "My Issues segment missing")
        myIssuesSegment.tap()
        XCTAssertTrue(
            app.staticTexts[Self.myIssueTitle].firstMatch.waitForExistence(timeout: 60),
            "My Issues never showed the issues assigned to the demo user"
        )
        settle(2)
        snapshot("sg_my-issues")

        // ── sg_agents: the machines / command centre ─────────────────────────
        // Deliberately NOT gated on a machine row: without a relay + an online
        // desktop the surface renders its empty state, and this suite is meant
        // to run without `screenshots:desktop`. Give the device list a chance
        // to arrive, then photograph whatever the surface settled on.
        let agentsTab = app.buttons["tab-agents"]
        XCTAssertTrue(agentsTab.waitForExistence(timeout: 15), "Agents tab missing")
        agentsTab.tap()
        XCTAssertTrue(
            app.navigationBars["Agents"].waitForExistence(timeout: 30),
            "Agents surface never appeared"
        )
        _ = app.staticTexts["My machines"].waitForExistence(timeout: 20)
        settle(2)
        snapshot("sg_agents")

        // ── sg_reviews: the cross-board open-PR queue ───────────────────────
        let reviewsTab = app.buttons["tab-reviews"]
        XCTAssertTrue(reviewsTab.waitForExistence(timeout: 15), "Reviews tab missing")
        reviewsTab.tap()
        XCTAssertTrue(
            app.staticTexts[Self.reviewTitle].firstMatch.waitForExistence(timeout: 60),
            "Reviews tab never showed the seeded open PRs"
        )
        settle(2)
        snapshot("sg_reviews")

        // ── sg_support-thread: the Emma Fischer helpdesk thread ─────────────
        // The tab exists only because the seed flips the team's
        // helpdesk_enabled on; threads come from tRPC polling, not Electric.
        let supportTab = app.buttons["tab-support"]
        XCTAssertTrue(
            supportTab.waitForExistence(timeout: 15),
            "Support tab missing — did the seed enable the team helpdesk?"
        )
        supportTab.tap()
        XCTAssertTrue(
            anyElement(app, identified: "support-thread-row").waitForExistence(timeout: 30),
            "Support inbox never showed the seeded threads"
        )
        // Rows carry the identifier but the reporter/title are plain text
        // inside the NavigationLink — tap the title, which activates the link.
        let supportRowTitle = app.staticTexts[Self.supportThreadTitle].firstMatch
        XCTAssertTrue(
            supportRowTitle.waitForExistence(timeout: 30),
            "The \(Self.supportReporter) thread is missing from the support inbox"
        )
        supportRowTitle.tap()
        // The thread detail has no identifiers at all; the last inbound message
        // is the only proof the conversation actually loaded.
        XCTAssertTrue(
            anyElement(app, containing: Self.supportReplyFragment).waitForExistence(timeout: 60),
            "The support thread body never loaded"
        )
        settle(2)
        snapshot("sg_support-thread")
        goBack(app)

        // ── sg_settings-team: team settings ─────────────────────────────────
        // The gear only lives on the issues tab's nav bar.
        app.buttons["tab-issues"].tap()
        let settingsLink = app.buttons["nav-settings-link"]
        XCTAssertTrue(settingsLink.waitForExistence(timeout: 20), "Settings toolbar link missing")
        settingsLink.tap()
        XCTAssertTrue(
            app.staticTexts["Teams"].waitForExistence(timeout: 20),
            "Settings screen never appeared"
        )
        // The team row aggregates an avatar + the name, so its own label is not
        // simply the team name — match the button by the staticText it contains.
        let teamRow = app.buttons.containing(.staticText, identifier: Self.teamName).firstMatch
        XCTAssertTrue(teamRow.waitForExistence(timeout: 20), "Team \"\(Self.teamName)\" missing from Settings")
        teamRow.tap()
        // TeamSettingsView carries the nav title "Settings" too — anchor on the
        // seeded board listed in its Boards section instead.
        XCTAssertTrue(
            app.staticTexts[Self.seededBoardName].waitForExistence(timeout: 30),
            "Team settings never listed the seeded boards"
        )
        settle(2)
        snapshot("sg_settings-team")
        goBack(app)

        // ── sg_settings-personal: the account / server detail ────────────────
        // There is no separate profile screen (EXP-311): the signed-in
        // identity, sign out and delete account live on the server row's
        // detail view. The row is titled by the SERVER, with the email below,
        // so match on the email.
        XCTAssertTrue(
            app.staticTexts["Servers"].waitForExistence(timeout: 20),
            "Did not return to the Settings screen"
        )
        let serverRow = app.buttons.containing(.staticText, identifier: ScreenshotSeed.demoEmail).firstMatch
        XCTAssertTrue(
            serverRow.waitForExistence(timeout: 20),
            "No server row for \(ScreenshotSeed.demoEmail) in Settings"
        )
        serverRow.tap()
        XCTAssertTrue(
            app.buttons["Sign out"].waitForExistence(timeout: 20),
            "Account settings did not open"
        )
        settle(2)
        snapshot("sg_settings-personal")
    }
}
