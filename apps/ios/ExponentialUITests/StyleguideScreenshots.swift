import XCTest

/// Styleguide screenshots (fastlane snapshot, `fastlane styleguide_screenshots`).
///
/// A SECOND capture suite next to StoreScreenshots: where the store set sells
/// the product in eight slides, this one photographs the SURFACES — the shots a
/// cross-platform design review compares side by side against the web, desktop
/// and Android captures of the same screens.
///
/// The names below are a CROSS-PLATFORM CONTRACT and byte-exact: the other
/// clients emit the same `sg_*` basenames so the compositor can pair them up.
/// Never rename or reorder them without changing every client — and never add
/// one here without the paired Android shot in StyleguideScreenshotsTest.kt.
///
///   sg_instance-picker · sg_sign-in · sg_board-switcher · sg_board-filters ·
///   sg_issue-comments · sg_issue-properties · sg_issue-create · sg_search ·
///   sg_my-issues · sg_agents · sg_action-create · sg_automations-list ·
///   sg_automations · sg_action-suggestions · sg_reviews · sg_support-thread ·
///   sg_settings-root · sg_settings-team · sg_settings-account
///
/// EXP-566 retired `sg_settings-personal`: it photographed ServerDetailView
/// here and the settings ROOT on Android, so the pair compared two different
/// screens. It is now the two properly-paired shots sg_settings-root (the
/// top-level list) and sg_settings-account (the server/account detail).
///
/// Same prerequisites as StoreScreenshots — a seeded dev server
/// (`apps/web/scripts/seed-screenshots.ts`: demo@exponential.at /
/// screenshots-demo, team "Acme", board "Mobile App", showcase issue APP-5,
/// open PRs, actions, automations, helpdesk threads). Unlike the store suite
/// this one does NOT need a live relay or an online desktop: sg_agents settles
/// for whatever the Agents surface shows, so the run stays green without
/// `screenshots:desktop`.
///
/// Every shot is gated on real seeded content, never on a container element —
/// an empty list still renders its container and would silently ship a blank
/// styleguide page. The two exceptions are called out where they happen
/// (sg_agents, and the automations pair, whose rows are newer than this
/// suite). Shared launch/sign-in/tap helpers live in ScreenshotFlow.swift.
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
    /// One of the three seeded team actions, listed on the Actions segment.
    private static let seededActionName = "Nightly test triage"

    @MainActor
    func testCaptureStyleguideScreenshots() throws {
        continueAfterFailure = false

        let app = launchScreenshotApp()

        // ── sg_instance-picker: the pre-login server chooser ─────────────────
        // The Snapfile erases the simulator, so the app always boots onto
        // InstanceView — its cloud buttons plus the "Use a self-hosted
        // instance" link, untouched. `awaitLaunchStage` deliberately taps
        // NOTHING, so this is the state a first-run user sees.
        let launch = awaitLaunchStage(app)
        if launch == .instancePicker {
            settle(1)
            snapshot("sg_instance-picker")
        } else {
            print("EXP-566 sg_instance-picker SKIPPED: the app booted already signed in (stale keychain — is erase_simulator on?)")
        }

        // ── sg_sign-in: the login screen itself ─────────────────────────────
        // Captured after the instance URL is accepted and BEFORE any
        // credentials are typed, so the shot shows the empty email/password
        // form rather than a half-filled one.
        //
        // If a keychain account somehow survived, the app comes up already
        // signed in and there is no login screen to photograph — say so loudly
        // and carry on with the rest.
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

        // ── sg_board-switcher: the server → team → board bottom sheet ────────
        // The trigger is the board-name control in the pinned nav row; its
        // accessibility LABEL is the same string as the sheet's headline, so
        // scope the tap to the button and the assertion to staticTexts.
        let switcherButton = app.buttons["Switch board"]
        XCTAssertTrue(switcherButton.waitForExistence(timeout: 20), "Board switcher trigger missing")
        switcherButton.tap()
        let switcherHeadline = app.staticTexts["Switch board"]
        XCTAssertTrue(switcherHeadline.waitForExistence(timeout: 15), "Board switcher sheet did not open")
        // The team block header is the real content — the sheet chrome alone
        // renders before the boards have been loaded off the synced rows.
        XCTAssertTrue(
            app.staticTexts[Self.teamName].firstMatch.waitForExistence(timeout: 30),
            "Board switcher never listed the seeded team"
        )
        settle(2)
        snapshot("sg_board-switcher")
        dismissSheet(app, whileVisible: switcherHeadline)

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

        // ── sg_issue-properties: the combined properties sheet ───────────────
        // Still on APP-5: the bottom bar's leading circle opens it (moderators
        // only — the demo user owns the team). The sheet rows carry the
        // property name inside the row BUTTON's merged label, so match on a
        // contained fragment rather than an exact staticText.
        let propertiesButton = app.buttons["issue-properties-button"]
        XCTAssertTrue(propertiesButton.waitForExistence(timeout: 20), "Properties button missing on the issue detail")
        propertiesButton.tap()
        let propertiesHeadline = app.staticTexts["Properties"]
        XCTAssertTrue(propertiesHeadline.waitForExistence(timeout: 15), "Properties sheet did not open")
        XCTAssertTrue(
            anyElement(app, containing: "Priority").waitForExistence(timeout: 15),
            "Properties sheet never showed its property rows"
        )
        settle(2)
        snapshot("sg_issue-properties")
        // "Close" is the GlassSheetChrome ✕ — the app's only one. Let the sheet
        // finish animating out before the nav-bar back tap, or that tap lands
        // on the dismissing sheet.
        app.buttons["Close"].firstMatch.tap()
        _ = propertiesHeadline.waitForNonExistence(timeout: 10)
        settle(1)
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

        // ── The Actions surface: four shots off one push ─────────────────────
        // Actions has no tab of its own (the bar is full) — the entry rides the
        // Agents toolbar, exactly like Android's Agents-header pill.
        let actionsLink = app.navigationBars["Agents"].buttons["Actions"]
        XCTAssertTrue(actionsLink.waitForExistence(timeout: 15), "Actions entry missing from the Agents toolbar")
        actionsLink.tap()
        XCTAssertTrue(
            app.navigationBars["Actions"].waitForExistence(timeout: 30),
            "Actions surface never appeared"
        )
        // The Agents toolbar link is ALSO a button labelled "Actions" — let the
        // push finish before addressing the segment, or the query matches two
        // elements and the tap throws.
        _ = app.navigationBars["Agents"].waitForNonExistence(timeout: 10)
        // The segment choice is persisted in @AppStorage, so a retry after a
        // mid-Actions failure would land on Automations/Suggestions — select
        // the Actions segment explicitly (the tap is idempotent).
        let actionsSegment = app.buttons["Actions"]
        XCTAssertTrue(actionsSegment.waitForExistence(timeout: 15), "Actions segment missing")
        actionsSegment.tap()
        XCTAssertTrue(
            app.staticTexts[Self.seededActionName].firstMatch.waitForExistence(timeout: 60),
            "The Actions segment never listed the seeded team actions"
        )

        // ── sg_action-create: the create-action sheet ────────────────────────
        // "New action" rides the "Actions · count" section header (EXP-574).
        // The sheet is only photographed, never submitted — creation would
        // start a real builtin run on somebody's machine.
        let newActionButton = app.buttons["New action"]
        XCTAssertTrue(newActionButton.waitForExistence(timeout: 20), "New action entry missing")
        newActionButton.tap()
        let createActionSheet = anyElement(app, identified: "create-action-sheet")
        XCTAssertTrue(
            createActionSheet.waitForExistence(timeout: 20),
            "Create-action sheet did not open"
        )
        // The description field is an `axis: .vertical` TextField — it surfaces
        // as a textView, not a textField — so address it by identifier alone.
        XCTAssertTrue(
            anyElement(app, identified: "create-action-description").waitForExistence(timeout: 15),
            "Create-action sheet never rendered its form"
        )
        settle(2)
        snapshot("sg_action-create")
        app.buttons["Cancel"].firstMatch.tap()
        _ = createActionSheet.waitForNonExistence(timeout: 10)
        settle(1)

        // ── sg_automations-list: the Automations segment ─────────────────────
        // Gated on the segment's OWN content rather than on a seeded automation
        // name: the `automations` rows are newer than this suite, so an older
        // seed shows the empty state — which is still a legitimate capture of
        // this surface, unlike a half-synced list.
        let automationsSegment = app.buttons["Automations"]
        XCTAssertTrue(automationsSegment.waitForExistence(timeout: 15), "Automations segment missing")
        automationsSegment.tap()
        let automationRow = anyElement(app, identified: "automation-row")
        if !automationRow.waitForExistence(timeout: 45) {
            print("EXP-566 sg_automations-list: no automation rows — reseed with `bun run seed:screenshots`")
            XCTAssertTrue(
                app.staticTexts["No automations yet."].waitForExistence(timeout: 15),
                "The Automations segment rendered neither rows nor its empty state"
            )
        }
        settle(2)
        snapshot("sg_automations-list")

        // ── sg_automations: the automation editor sheet ──────────────────────
        // "New automation" is owner-only AND steer-gated on iOS (the pill is
        // hidden when the backend has no STEER_RELAY_URL, since nothing could
        // ever run the binding). Android's twin is not steer-gated, so fall
        // back to Edit on a seeded row to keep the pair photographing the same
        // sheet on a relay-less backend.
        let newAutomationButton = app.buttons["New automation"]
        if newAutomationButton.waitForExistence(timeout: 10) {
            newAutomationButton.tap()
        } else {
            print("EXP-566 sg_automations: no \"New automation\" entry (steering off?) — editing a seeded automation instead")
            let rowMenu = app.buttons["Automation actions"].firstMatch
            XCTAssertTrue(
                rowMenu.waitForExistence(timeout: 15),
                "Neither the New-automation entry nor a seeded automation row is available"
            )
            rowMenu.tap()
            let editItem = app.buttons["Edit"].firstMatch
            XCTAssertTrue(editItem.waitForExistence(timeout: 15), "The automation row menu never opened")
            editItem.tap()
        }
        let automationSheet = anyElement(app, identified: "automation-form-sheet")
        XCTAssertTrue(
            automationSheet.waitForExistence(timeout: 20),
            "The automation form sheet did not open"
        )
        settle(2)
        snapshot("sg_automations")
        app.buttons["Cancel"].firstMatch.tap()
        _ = automationSheet.waitForNonExistence(timeout: 10)
        settle(1)

        // ── sg_action-suggestions: the Suggestions segment ────────────────────
        // Shipped constants (`ActionSuggestion.seeds`), not seeded rows — this
        // one can be gated hard on a row.
        let suggestionsSegment = app.buttons["Suggestions"]
        XCTAssertTrue(suggestionsSegment.waitForExistence(timeout: 15), "Suggestions segment missing")
        suggestionsSegment.tap()
        XCTAssertTrue(
            anyElement(app, identified: "suggestion-row").waitForExistence(timeout: 20),
            "The Suggestions segment never rendered its seed cards"
        )
        settle(2)
        snapshot("sg_action-suggestions")
        // Leave the surface on Actions so a retry starts where it started.
        actionsSegment.tap()
        goBack(app)

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

        // ── sg_settings-root: the top-level settings list ────────────────────
        // The gear only lives on the issues tab's nav bar. The root is the
        // Servers / Teams / General stack — the paired Android shot of the
        // SAME screen (EXP-566 split it out of the old sg_settings-personal).
        app.buttons["tab-issues"].tap()
        let settingsLink = app.buttons["nav-settings-link"]
        XCTAssertTrue(settingsLink.waitForExistence(timeout: 20), "Settings toolbar link missing")
        settingsLink.tap()
        XCTAssertTrue(
            app.staticTexts["Teams"].waitForExistence(timeout: 20),
            "Settings screen never appeared"
        )
        XCTAssertTrue(
            app.staticTexts["Servers"].waitForExistence(timeout: 20),
            "Settings screen never showed its Servers section"
        )
        // The team row aggregates an avatar + the name, so its own label is not
        // simply the team name — match the button by the staticText it contains.
        let teamRow = app.buttons.containing(.staticText, identifier: Self.teamName).firstMatch
        XCTAssertTrue(teamRow.waitForExistence(timeout: 20), "Team \"\(Self.teamName)\" missing from Settings")
        settle(2)
        snapshot("sg_settings-root")

        // ── sg_settings-team: team settings ─────────────────────────────────
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

        // ── sg_settings-account: the account / server detail ──────────────────
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
        snapshot("sg_settings-account")
    }
}
