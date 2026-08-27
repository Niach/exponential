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
/// `packages/view-catalog/src/views.test.ts` gates both directions, and
/// additionally requires the iOS and Android `sg_*` sets to be IDENTICAL.
///
///   sg_sign-in · sg_board-switcher · sg_board-filters · sg_board-empty ·
///   sg_board-bulk-edit · sg_issue-comments · sg_issue-properties ·
///   sg_issue-create · sg_search · sg_my-issues · sg_agents ·
///   sg_recent-runs · sg_start-coding-actions · sg_start-coding-chat ·
///   sg_machine-settings · sg_action-create · sg_automations-list ·
///   sg_automations · sg_action-suggestions · sg_reviews ·
///   sg_support-thread · sg_settings-root · sg_settings-team ·
///   sg_settings-account · sg_onboarding
///
/// EXP-642 reshuffled the front of the set: the old `sg_instance-picker` shot
/// IS the cloud Apple/Google chooser a first-run user meets, so it took over
/// the `sg_sign-in` name, and the password-form shot that used to carry it is
/// gone (the form is a self-hosting detail, not the sign-in surface). The
/// login flow itself is unchanged — the suite still signs in with it.
/// EXP-566 had earlier retired `sg_settings-personal` in favour of the properly
/// paired sg_settings-root (the top-level list) + sg_settings-account (the
/// server/account detail).
///
/// Prerequisites — a seeded dev server (`apps/web/scripts/seed-screenshots.ts`:
/// demo@exponential.at / screenshots-demo, team "Acme", boards "Mobile App" +
/// the empty "Launch Marketing", showcase issue APP-5, open PRs, actions,
/// automations, helpdesk threads) PLUS, since EXP-642, the relay stub:
/// `bun run screenshots:desktop` (apps/web) registers the demo user's OWN
/// device row, which is what `sg_machine-settings` (gated `isMine &&
/// registered`) and the two `sg_start-coding-*` shots photograph. No steer
/// RELAY traffic is needed beyond that registration — nothing here watches a
/// live session.
///
/// Every shot is gated on real seeded content, never on a container element —
/// an empty list still renders its container and would silently ship a blank
/// styleguide page. The exceptions are called out where they happen (the
/// automations pair, whose rows are newer than this suite). Shared
/// launch/sign-in/tap helpers live in ScreenshotFlow.swift, including the
/// `snapshot(_:settle:)` overload every capture below goes through (it honours
/// the lane's optional `shots:` allowlist).
final class StyleguideScreenshots: XCTestCase {

    /// Set at the very end of the capture walk, so the `shots:` typo check in
    /// tearDown never piles a second failure onto a run that already broke.
    private var finished = false

    override func tearDown() {
        assertRequestedShotsWereReached(suiteFinished: finished)
        super.tearDown()
    }

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
    /// The close-out the seed's freshest agent-ended run carries (EXP-637) —
    /// only the EXPANDED row shows it, so it is the post-tap gate (EXP-663).
    private static let recentRunSummary = "Triaged 4 failing specs"
    private static let supportReporter = "Emma Fischer"
    private static let supportReplyFragment = "thank you for the quick turnaround"
    /// A seeded board — the anchor that says we are on TEAM settings rather
    /// than the outer Settings screen (both carry the nav title "Settings").
    private static let seededBoardName = "Mobile App"
    /// The seed's SECOND board: created empty on purpose, so the "no issues
    /// yet" state is photographable without deleting anything (EXP-642).
    private static let emptyBoardName = "Launch Marketing"
    /// Two backlog issues (APP-11 / APP-13) — the bulk-edit selection. Both
    /// sit in the same group, so one scroll reaches both.
    private static let bulkFirstTitle = "Localize the app in German and Spanish"
    private static let bulkSecondTitle = "Audit accessibility labels for VoiceOver"
    /// One of the three seeded team actions, listed on the Actions segment.
    private static let seededActionName = "Nightly test triage"
    /// The device `bun run screenshots:desktop` registers for the demo user.
    private static let demoDeviceName = "Alex's MacBook Pro"

    @MainActor
    func testCaptureStyleguideScreenshots() throws {
        continueAfterFailure = false

        let app = launchScreenshotApp()

        // ── sg_sign-in: the pre-login server chooser ─────────────────────────
        // The Snapfile erases the simulator, so the app always boots onto
        // InstanceView — its cloud (Apple / Google) buttons plus the "Use a
        // self-hosted instance" link, untouched. `awaitLaunchStage`
        // deliberately taps NOTHING, so this is the state a first-run user
        // sees, and it is what the web/desktop `sign-in` shots show too.
        let launch = awaitLaunchStage(app)
        if launch == .instancePicker {
            snapshot("sg_sign-in", settle: 1)
        } else {
            print("EXP-566 sg_sign-in SKIPPED: the app booted already signed in (stale keychain — is erase_simulator on?)")
        }

        // The password form is deliberately NOT photographed any more
        // (EXP-642) — the lane still drives it to get signed in.
        let stage = presentLoginScreen(app)
        if stage == .loginReady {
            submitLogin(app)
        } else {
            print("EXP-566 sign-in SKIPPED: the app booted already signed in (stale keychain — is erase_simulator on?)")
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
        snapshot("sg_board-switcher", settle: 2)
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
        snapshot("sg_board-filters", settle: 2)
        dismissSheet(app, whileVisible: filterSheetHeadline)

        // ── sg_board-empty: a board with no issues on it ─────────────────────
        // The seed's second board ("Launch Marketing") is created empty for
        // exactly this shot, so nothing has to be deleted to reach the state.
        switchBoard(app, to: Self.emptyBoardName)
        XCTAssertTrue(
            app.staticTexts["No issues yet"].waitForExistence(timeout: 60),
            "\(Self.emptyBoardName) did not render its empty state"
        )
        snapshot("sg_board-empty", settle: 2)
        switchBoard(app, to: Self.seededBoardName)
        XCTAssertTrue(
            showcaseRowTitle.waitForExistence(timeout: 60),
            "Did not get back to \(Self.seededBoardName)"
        )

        // ── sg_board-bulk-edit: multi-select + the bulk action bar ───────────
        // Long-press enters selection mode (with a haptic tick); a plain tap on
        // a second row then adds it. Both rows are in the backlog group at the
        // bottom of the list, so scroll them into view first.
        let bulkFirst = app.staticTexts[Self.bulkFirstTitle]
        XCTAssertTrue(
            scrollUntilVisible(app, bulkFirst, attempts: 14),
            "\(Self.bulkFirstTitle) never scrolled into view"
        )
        bulkFirst.press(forDuration: 1.0)
        let bulkBar = anyElement(app, identified: "bulk-selection-bar")
        XCTAssertTrue(
            bulkBar.waitForExistence(timeout: 15),
            "Long-press did not enter multi-select"
        )
        let bulkSecond = app.staticTexts[Self.bulkSecondTitle]
        XCTAssertTrue(
            scrollUntilVisible(app, bulkSecond, attempts: 8),
            "\(Self.bulkSecondTitle) never scrolled into view"
        )
        bulkSecond.tap()
        snapshot("sg_board-bulk-edit", settle: 2)
        // Leave selection mode — every later shot assumes the plain list.
        app.buttons["Clear selection"].firstMatch.tap()
        _ = bulkBar.waitForNonExistence(timeout: 10)

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
        snapshot("sg_issue-comments", settle: 2)

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
        snapshot("sg_issue-properties", settle: 2)
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
        snapshot("sg_issue-create", settle: 2)
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
        snapshot("sg_search", settle: 2)

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
        snapshot("sg_my-issues", settle: 2)

        // ── sg_agents: the machines / command centre ─────────────────────────
        // Since EXP-642 this lane needs the relay stub (`screenshots:desktop`):
        // the demo user's own device row is what the next three shots are
        // taken from, and an empty machines list is not a useful reference
        // shot either.
        let agentsTab = app.buttons["tab-agents"]
        XCTAssertTrue(agentsTab.waitForExistence(timeout: 15), "Agents tab missing")
        agentsTab.tap()
        XCTAssertTrue(
            app.navigationBars["Agents"].waitForExistence(timeout: 30),
            "Agents surface never appeared"
        )
        XCTAssertTrue(
            app.staticTexts[Self.demoDeviceName].firstMatch.waitForExistence(timeout: 60),
            "No \(Self.demoDeviceName) row — is `bun run screenshots:desktop` running?"
        )
        snapshot("sg_agents", settle: 2)

        // ── sg_recent-runs: the EXP-637 close-out ────────────────────────────
        // The seed's two action runs end with ended_by='agent' + an outcome +
        // the agent's summary (seed-screenshots.ts) — the trio "Recent runs"
        // gates on. Collapsed, a row shows only the outcome, so the shot is
        // taken EXPANDED: the summary the agent wrote plus the Resume pill the
        // stub device's `resume-run` cap enables.
        // The section sits below the machines and Running in a LazyVStack, so
        // the row is not even in the accessibility tree until it scrolls on
        // screen: swipe first, assert after. Matched by identifier on ANY
        // element type — the row carries `recent-run-row` on its container and
        // `ended-run-row` on the toggle button, and SwiftUI decides which one
        // the hit-testable element ends up exposing.
        let endedRun = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier IN {'ended-run-row', 'recent-run-row'}")
        ).firstMatch
        var swipesDown = 0
        while !endedRun.exists && swipesDown < 8 {
            app.swipeUp()
            swipesDown += 1
        }
        XCTAssertTrue(
            scrollUntilVisible(app, endedRun),
            "No Recent runs row — the seed needs runs the AGENT closed out (ended_by 'agent')"
        )
        endedRun.tap()
        XCTAssertTrue(
            anyElement(app, containing: Self.recentRunSummary).waitForExistence(timeout: 15),
            "The Recent runs row expanded without its summary — is `summary` seeded?"
        )
        snapshot("sg_recent-runs", settle: 2)
        // Collapse again and scroll back up, so the start-coding steps below
        // see the same surface they always did (scrollUntilVisible only swipes
        // up; the machines list sits ABOVE Recent runs).
        endedRun.tap()
        let deviceRow = app.staticTexts[Self.demoDeviceName].firstMatch
        var swipesBack = 0
        while !deviceRow.isHittable && swipesBack < 6 {
            app.swipeDown()
            swipesBack += 1
        }
        settle(1)

        // ── sg_start-coding-actions / -chat: the unified launch sheet ────────
        // The machine row's play glyph opens the sheet the Agents surface owns;
        // it wires teamId + onRunAction, so the Issues | Actions | Chat
        // segmented control is there. The tabs carry identifiers because
        // "Actions" and "Chat" also read as ordinary buttons elsewhere.
        // Nothing is ever submitted — a run would land on a real machine.
        let startCoding = app.buttons["Start coding"].firstMatch
        XCTAssertTrue(
            startCoding.waitForExistence(timeout: 20),
            "The machine row offers no start action — is the stub device online with an agent?"
        )
        startCoding.tap()
        let startSheet = anyElement(app, identified: "start-coding-sheet")
        XCTAssertTrue(startSheet.waitForExistence(timeout: 20), "Start-coding sheet did not open")

        let actionsTab = anyElement(app, identified: "start-coding-tab-actions")
        XCTAssertTrue(actionsTab.waitForExistence(timeout: 15), "Start-coding sheet has no Actions tab")
        actionsTab.tap()
        XCTAssertTrue(
            app.staticTexts[Self.seededActionName].firstMatch.waitForExistence(timeout: 60),
            "The Actions tab never listed the team's actions"
        )
        snapshot("sg_start-coding-actions", settle: 2)

        let chatTab = anyElement(app, identified: "start-coding-tab-chat")
        XCTAssertTrue(chatTab.waitForExistence(timeout: 15), "Start-coding sheet has no Chat tab")
        chatTab.tap()
        snapshot("sg_start-coding-chat", settle: 2)
        app.buttons["Cancel"].firstMatch.tap()
        _ = startSheet.waitForNonExistence(timeout: 10)
        settle(1)

        // ── sg_machine-settings: the device settings sheet ───────────────────
        // Own, registered machines only — the row menu is absent otherwise,
        // which is why the relay stub is a prerequisite.
        let machineMenu = app.buttons["machine-menu"].firstMatch
        XCTAssertTrue(
            machineMenu.waitForExistence(timeout: 20),
            "No machine row menu — the stub device must be the demo user's OWN, registered machine"
        )
        machineMenu.tap()
        let editItem = app.buttons["Edit"].firstMatch
        XCTAssertTrue(editItem.waitForExistence(timeout: 15), "The machine menu never opened")
        editItem.tap()
        let deviceSheet = anyElement(app, identified: "device-settings-sheet")
        XCTAssertTrue(deviceSheet.waitForExistence(timeout: 20), "Device settings sheet did not open")
        snapshot("sg_machine-settings", settle: 2)
        app.buttons["Done"].firstMatch.tap()
        _ = deviceSheet.waitForNonExistence(timeout: 10)
        settle(1)

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
        snapshot("sg_action-create", settle: 2)
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
        snapshot("sg_automations-list", settle: 2)

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
            let editAutomation = app.buttons["Edit"].firstMatch
            XCTAssertTrue(editAutomation.waitForExistence(timeout: 15), "The automation row menu never opened")
            editAutomation.tap()
        }
        let automationSheet = anyElement(app, identified: "automation-form-sheet")
        XCTAssertTrue(
            automationSheet.waitForExistence(timeout: 20),
            "The automation form sheet did not open"
        )
        snapshot("sg_automations", settle: 2)
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
        snapshot("sg_action-suggestions", settle: 2)
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
        snapshot("sg_reviews", settle: 2)

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
        snapshot("sg_support-thread", settle: 2)
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
        snapshot("sg_settings-root", settle: 2)

        // ── sg_settings-team: team settings ─────────────────────────────────
        teamRow.tap()
        // TeamSettingsView carries the nav title "Settings" too — anchor on the
        // seeded board listed in its Boards section instead.
        XCTAssertTrue(
            app.staticTexts[Self.seededBoardName].waitForExistence(timeout: 30),
            "Team settings never listed the seeded boards"
        )
        snapshot("sg_settings-team", settle: 2)
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
        snapshot("sg_settings-account", settle: 2)

        // ── sg_onboarding: the first-run create-or-join wizard ───────────────
        // LAST on purpose: it switches the signed-in identity. AppNavigator
        // shows LoginView at the root only when EVERY account is tokenless, so
        // "Add server" on the same instance can never reach a login while the
        // demo user is signed in (its cover just re-points the pending row).
        // Sign the demo account out instead — we are on its ServerDetail
        // screen right after sg_settings-account — and the root becomes the
        // LoginView for that instance.
        //
        // The newcomer (`newcomer@exponential.at`) is a member of nothing with
        // a null `onboardingCompletedAt`, so the app opens the wizard. NOTHING
        // is submitted: creating a team or accepting an invite would mutate the
        // seed and burn the invite the desktop/web lanes photograph.
        app.buttons["Sign out"].firstMatch.tap()
        submitLogin(
            app,
            email: ScreenshotSeed.newcomerEmail,
            password: ScreenshotSeed.newcomerPassword
        )
        let getStarted = app.buttons["Get started"]
        XCTAssertTrue(
            getStarted.waitForExistence(timeout: 90),
            "The onboarding wizard never appeared for \(ScreenshotSeed.newcomerEmail) — reseed with `bun run seed:screenshots`"
        )
        getStarted.tap()
        XCTAssertTrue(
            app.staticTexts["Set up your team"].waitForExistence(timeout: 30),
            "The wizard never reached its team step"
        )
        // The mobile wizard shows the Create and Join cards on ONE step — this
        // single shot is the whole `onboarding` view on iOS/Android (there is
        // no separate create-team / join screen to photograph).
        XCTAssertTrue(
            app.buttons["Create team"].waitForExistence(timeout: 30),
            "The team step never rendered its Create card"
        )
        snapshot("sg_onboarding", settle: 2)

        finished = true
    }

    // MARK: - Helpers

    /// Board switcher → the named board.
    ///
    /// The sheet's rows are Buttons whose glyph + name + prefix SwiftUI merges
    /// into ONE element, so there is no contained staticText to address — match
    /// on the button's own concatenated label instead. The nav-row trigger
    /// overrides its label to "Switch board", so it can never be the match.
    @MainActor
    private func switchBoard(_ app: XCUIApplication, to name: String) {
        let switcherButton = app.buttons["Switch board"]
        XCTAssertTrue(switcherButton.waitForExistence(timeout: 20), "Board switcher trigger missing")
        switcherButton.tap()
        let headline = app.staticTexts["Switch board"]
        XCTAssertTrue(headline.waitForExistence(timeout: 15), "Board switcher sheet did not open")
        let row = app.buttons.matching(
            NSPredicate(format: "label CONTAINS %@", name)
        ).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 30), "Board \"\(name)\" missing from the switcher")
        row.tap()
        _ = headline.waitForNonExistence(timeout: 10)
        settle(1)
    }

}
