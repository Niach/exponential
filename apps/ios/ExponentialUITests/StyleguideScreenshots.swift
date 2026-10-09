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
///   sg_sign-in · sg_board-switcher · sg_onboarding-create-team ·
///   sg_board-empty ·
///   sg_board-bulk-edit · sg_issue-comments · sg_issue-properties ·
///   sg_issue-create · sg_search · sg_my-issues · sg_agents ·
///   sg_chat · sg_composer-menu · sg_chat-issues · sg_chat-action ·
///   sg_machine-settings · sg_action-create · sg_action-triggers ·
///   sg_trigger-editor · sg_action-runs · sg_action-suggestions · sg_reviews ·
///   sg_session-row · sg_pr-row · sg_guide · sg_guide-section ·
///   sg_settings-root · sg_settings-team ·
///   sg_settings-account · sg_onboarding · sg_onboarding-invite ·
///   sg_onboarding-devices
///
/// EXP-909 retired `sg_usage`: the cross-device Accounts section it
/// photographed is gone — every machine lists its own logins under its row, so
/// `sg_agents` is the one shot of them. It still needs the relay stub's demo
/// device: its heartbeat announces the agent accounts those rows list.
///
/// EXP-725 added `sg_onboarding-invite` + `sg_onboarding-devices`: the wizard
/// runs the same four steps on every client now, and the last two need a
/// RESOLVED team, which the newcomer has not got. They are captured as the
/// starter identity, relaunched with `-uiTestingOnboardingStep invite` so the
/// wizard parks on step 3 without anything being submitted.
///
/// EXP-698 r5 added `sg_onboarding-create-team`: the board switcher grew a
/// "New team" row (and a per-team "Create board" one), so the create-or-join
/// team form is now reachable from the app's home instead of only from the
/// zero-team empty state a seeded account never shows. It is captured right
/// after `sg_board-switcher`, off that row.
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
/// the empty "Launch Marketing", showcase issue APP-5, open PRs, actions with
/// their triggers and runs) PLUS, since EXP-642, the relay stub:
/// `bun run screenshots:desktop` (apps/web) registers the demo user's OWN
/// device row, which is what `sg_machine-settings` (gated `isMine &&
/// registered`) and the three `sg_chat*` shots photograph. No steer
/// RELAY traffic is needed beyond that registration — nothing here watches a
/// live session.
///
/// Every shot is gated on real seeded content, never on a container element —
/// an empty list still renders its container and would silently ship a blank
/// styleguide page. The exceptions are called out where they happen (the
/// action page's Triggers and Runs tabs, whose rows an older seed lacks). Shared
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
    /// APP-14: the seed's realPr issue, so its Guide has files from GitHub.
    private static let guideTitle = "Group board issues by assignee"
    /// EXP-1204: the seeded chat run with an open PR of its own — its subject
    /// is the Reviews "Agent runs" row title and the RunChanges header.
    private static let runChangesTitle = "Fix the error-type comparison in the resolver"
    private static let searchQuery = "cold start"
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
    /// The login that device announces for its agents (`screenshot-demo.ts`
    /// `DEMO_AGENT_STATUS`) — EXP-909: the first login row UNDER the machine
    /// row names it.
    private static let demoAccountEmail = "demo@exponential.at"

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

        // The app lands on the Agent tab — switch to Issues for the board.
        let issuesTab = app.buttons["tab-issues"]
        XCTAssertTrue(issuesTab.waitForExistence(timeout: 60), "Tab bar never appeared")
        issuesTab.tap()

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

        // ── sg_onboarding-create-team: the create-or-join team form ──────────
        // Reached from the switcher's bottom "New team" row, which is also how
        // it DISMISSES the switcher — the row parks its intent and the sheet is
        // presented from the switcher's `onDismiss` (two presentations in one
        // transaction get dropped). So there is a dismissal animation in
        // between: wait on the sheet's own identifier, never on a delay.
        let newTeamRow = app.buttons["board-switcher-new-team"]
        XCTAssertTrue(newTeamRow.waitForExistence(timeout: 15), "Board switcher has no New team row")
        // The sheet is fitted to its content, so the row is on screen for the
        // seeded team — scroll it up only if a longer board list ever pushes it
        // under the fold.
        scrollUntilVisible(app, newTeamRow, attempts: 3)
        newTeamRow.tap()
        let teamSetupSheet = app.descendants(matching: .any)["team-setup-sheet"].firstMatch
        XCTAssertTrue(
            teamSetupSheet.waitForExistence(timeout: 20),
            "New team row did not open the team setup sheet"
        )
        // The sheet opens on the choice page (polish round ×4); its "Create
        // a team" button PUSHES the create form this view photographs. Gate
        // on real content, not the container.
        let createTeamChoice = app.buttons["team-setup-create"]
        XCTAssertTrue(
            createTeamChoice.waitForExistence(timeout: 15),
            "Team setup sheet never rendered its Create a team choice"
        )
        createTeamChoice.tap()
        XCTAssertTrue(
            app.staticTexts["Team name"].waitForExistence(timeout: 15),
            "Create a team never pushed its form"
        )
        snapshot("sg_onboarding-create-team", settle: 1)
        // Nothing was submitted, so no team is created and the next steps start
        // from the same board list. The switcher is already gone — this only
        // has to close the setup sheet.
        dismissSheet(app, whileVisible: app.staticTexts["Set up a team"].firstMatch)
        XCTAssertTrue(
            showcaseRowTitle.waitForExistence(timeout: 20),
            "Dismissing the team setup sheet did not return to the board list"
        )

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
        //
        // A self-contained DETOUR: it opens a sheet and closes it again, and
        // no later shot depends on it. So unlike the rest of the walk it is
        // skipped whole when a `shots:` run did not ask for it — a broken
        // properties sheet must not cost a scoped run its unrelated shots.
        // `isWanted` still records the id as reached for the typo check.
        if ScreenshotShots.isWanted("sg_issue-properties") {
            let propertiesButton = app.buttons["issue-properties-button"]
            XCTAssertTrue(propertiesButton.waitForExistence(timeout: 20), "Properties button missing on the issue detail")
            let propertiesHeadline = app.staticTexts["Properties"]
            // ONE tap, on purpose (EXP-1160): this is the first sheet the
            // page presents since the screen opened, the case the paged
            // `TabView` broke (it presented the sheet twice and tore both
            // down). A retry here would hide that regression.
            propertiesButton.tap()
            XCTAssertTrue(
                propertiesHeadline.waitForExistence(timeout: 15),
                "Properties sheet did not open on its first tap"
            )
            XCTAssertTrue(
                anyElement(app, containing: "Priority").waitForExistence(timeout: 15),
                "Properties sheet never showed its property rows"
            )
            snapshot("sg_issue-properties", settle: 2)
            // EXP-687: no sheet has a close button any more. Let the sheet
            // finish animating out before the nav-bar back tap, or that tap
            // lands on the dismissing sheet.
            dismissSheet(app, whileVisible: propertiesHeadline)
            _ = propertiesHeadline.waitForNonExistence(timeout: 10)
            settle(1)
        }
        goBack(app)

        // ── sg_issue-create: the new-issue page ─────────────────────────────
        // The compose button is only mounted on board routes (AppNavigator
        // `composeTarget`), so come back to the issues tab first. The title
        // field takes focus on appear, so the page is captured with the
        // keyboard up — which is the state a user actually sees.
        app.buttons["tab-issues"].tap()
        XCTAssertTrue(showcaseRowTitle.waitForExistence(timeout: 20), "Board did not come back for the compose shot")
        let composeButton = app.buttons["compose-button"]
        XCTAssertTrue(composeButton.waitForExistence(timeout: 15), "Compose button missing on the board")
        composeButton.tap()
        let titleField = app.textFields["issue-title-field"]
        XCTAssertTrue(titleField.waitForExistence(timeout: 15), "Create-issue page did not open")
        focus(titleField)
        titleField.typeText("Prefetch avatars before the first board paint")
        snapshot("sg_issue-create", settle: 2)
        // EXP-1170: the page AUTOSAVES its draft while the title is typed, and
        // the styleguide run must not leave anything in the seed — clear the
        // title first: Back from an emptied page DELETES the transient row the
        // autosave wrote (and writes nothing when none was written yet).
        clearText(of: titleField)
        app.buttons["Back"].firstMatch.tap()
        _ = titleField.waitForNonExistence(timeout: 10)

        // ── sg_search: the search view with seeded results ───────────────────
        // EXP-686: Search lost its tab — it is a push off the board header.
        let searchButton = app.buttons["board-search"]
        XCTAssertTrue(searchButton.waitForExistence(timeout: 15), "Board search button missing")
        searchButton.tap()
        let searchField = app.textFields["search-field"]
        XCTAssertTrue(searchField.waitForExistence(timeout: 15), "Search field missing")
        focus(searchField)
        searchField.typeText(Self.searchQuery)
        XCTAssertTrue(
            app.staticTexts[Self.showcaseTitle].firstMatch.waitForExistence(timeout: 30),
            "Search never returned the seeded issue for \"\(Self.searchQuery)\""
        )
        snapshot("sg_search", settle: 2)
        goBack(app)

        // ── sg_my-issues: Inbox → the "My issues" segment ────────────────────
        // The segment is a GlassSegmentedControl button carrying only an
        // accessibility LABEL. Its choice is persisted in @AppStorage, so the
        // tap is deliberately unconditional (it is idempotent).
        let myWorkTab = app.buttons["tab-mywork"]
        XCTAssertTrue(myWorkTab.waitForExistence(timeout: 15), "Inbox tab missing")
        myWorkTab.tap()
        let myIssuesSegment = app.buttons["My issues"]
        XCTAssertTrue(myIssuesSegment.waitForExistence(timeout: 15), "My issues segment missing")
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
        // shot either. EXP-686 renamed the surface to Devices (the shot id
        // stays sg_agents).
        let devicesTab = app.buttons["tab-devices"]
        XCTAssertTrue(devicesTab.waitForExistence(timeout: 15), "Devices tab missing")
        devicesTab.tap()
        XCTAssertTrue(
            app.navigationBars["Devices"].waitForExistence(timeout: 30),
            "Devices surface never appeared"
        )
        XCTAssertTrue(
            app.staticTexts[Self.demoDeviceName].firstMatch.waitForExistence(timeout: 60),
            "No \(Self.demoDeviceName) row — is `bun run screenshots:desktop` running?"
        )
        // EXP-944: device rows are COLLAPSED by default; the shot opens the
        // demo device so its logins and their usage are in frame.
        let deviceRow = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'device-row-'"))
            .firstMatch
        XCTAssertTrue(deviceRow.waitForExistence(timeout: 30), "No foldable device row")
        deviceRow.tap()
        // EXP-909: the machine's own logins sit under its row — the subject
        // the retired `sg_usage` shot used to have a section of its own for.
        XCTAssertTrue(
            anyElement(app, containing: Self.demoAccountEmail).waitForExistence(timeout: 60),
            "No \(Self.demoAccountEmail) login row — is the stub device reporting agent accounts?"
        )
        snapshot("sg_agents", settle: 2)

        // ── sg_chat / sg_chat-issues / sg_chat-action: the Agent page ────────
        // EXP-825: the ONE launcher. EXP-909: a device row starts nothing any
        // more (its one control is the settings gear), so the bar's Chat
        // arm opens the Agent page on the default device: an empty
        // composer is a chat; the `#` tool checks issues (two chips, a
        // batch); the ▶ tool picks an action (the Fix merge conflicts
        // builtin, its card's PR picked: EXP-1233). Nothing is ever submitted — a run
        // would land on a real machine.
        let chatButton = app.buttons["chat-button"].firstMatch
        XCTAssertTrue(
            chatButton.waitForExistence(timeout: 20),
            "The bar offers no Chat arm — is the demo team's device online with an agent?"
        )
        chatButton.tap()
        let composer = anyElement(app, identified: "agent-composer")
        XCTAssertTrue(composer.waitForExistence(timeout: 20), "Agent page did not open")
        // The submit label proves the composer resolved its subject (a chat).
        XCTAssertTrue(
            app.buttons["Start chat"].firstMatch.waitForExistence(timeout: 15),
            "The composer never settled on the chat subject"
        )
        snapshot("sg_chat", settle: 2)

        // ── sg_session-row: THE session row, big (EXP-1248) ─────────────────
        // The Recent sheet behind the page's history glyph draws every run as
        // the big SessionRow (mark at 12 + 14·depth, caption, device glyph,
        // children nested, no fold). A detour no later shot needs, so it is
        // skipped whole when a scoped run did not ask for it.
        if ScreenshotShots.isWanted("sg_session-row") {
            anyElement(app, identified: "agent-history-button").tap()
            let recentSheet = anyElement(app, identified: "recent-runs-sheet")
            XCTAssertTrue(recentSheet.waitForExistence(timeout: 20), "The Recent sheet did not open")
            if !anyElement(app, identified: "past-run-row").waitForExistence(timeout: 30) {
                print("EXP-1248 sg_session-row: no finished runs — reseed with `bun run seed:screenshots`")
            }
            snapshot("sg_session-row", settle: 2)
            dismissSheet(app, whileVisible: recentSheet)
            _ = recentSheet.waitForNonExistence(timeout: 10)
            settle(1)
        }

        // EXP-1249: the "+" is the composer's ONLY tool — its sheet of the
        // `composer-menu.json` rows is `sg_composer-menu`; Implement issue ›
        // hands off to the issue picker once the sheet is gone.
        anyElement(app, identified: "agent-composer-plus-button").tap()
        let plusMenu = anyElement(app, identified: "agent-composer-menu")
        XCTAssertTrue(plusMenu.waitForExistence(timeout: 20), "The + menu did not open")
        snapshot("sg_composer-menu", settle: 2)
        anyElement(app, identified: "agent-composer-menu-implement-issue").tap()
        let issuePicker = anyElement(app, identified: "agent-composer-issues-picker")
        XCTAssertTrue(issuePicker.waitForExistence(timeout: 20), "Issue picker did not open")
        for title in [Self.bulkFirstTitle, Self.bulkSecondTitle] {
            // EXP-1030: a picker row reads `IDENT Title` in ONE label (the ×4
            // contract), so it is matched on the title as a fragment.
            let row = anyElement(app, containing: title)
            XCTAssertTrue(row.waitForExistence(timeout: 60), "Issue picker never listed \"\(title)\"")
            row.tap()
        }
        // EXP-1030: the shared picker has no Done button — a multi pick stays
        // open across toggles and closes with the platform swipe (EXP-687).
        dismissSheet(app, whileVisible: issuePicker)
        _ = issuePicker.waitForNonExistence(timeout: 10)
        let issueChip = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "agent-composer-chip-issue-")
        ).firstMatch
        XCTAssertTrue(issueChip.waitForExistence(timeout: 15), "No issue chip after picking")
        snapshot("sg_chat-issues", settle: 2)

        anyElement(app, identified: "agent-composer-plus-button").tap()
        XCTAssertTrue(
            anyElement(app, identified: "agent-composer-menu").waitForExistence(timeout: 20),
            "The + menu did not reopen"
        )
        anyElement(app, identified: "agent-composer-menu-run-action").tap()
        let actionPicker = anyElement(app, identified: "agent-composer-actions-picker")
        XCTAssertTrue(actionPicker.waitForExistence(timeout: 20), "Action picker did not open")
        let fixRow = app.staticTexts["Fix merge conflicts"].firstMatch
        XCTAssertTrue(fixRow.waitForExistence(timeout: 20), "Action picker never listed the builtin")
        fixRow.tap()
        _ = actionPicker.waitForNonExistence(timeout: 10)
        XCTAssertTrue(
            anyElement(app, identified: "agent-composer-chip-action").waitForExistence(timeout: 15),
            "No action chip after picking"
        )
        // EXP-1233: the builtin draws its own card; picking the seeded open
        // PR (APP-14) completes it into the Fix merge conflicts look — the
        // headline's verb + the PR's issue chip, the card's PR row.
        let fixCard = anyElement(app, identified: "agent-composer-fix-conflicts")
        XCTAssertTrue(fixCard.waitForExistence(timeout: 15), "No Fix merge conflicts card")
        // The row is inert until the open-PR pool has synced: a tap that opens
        // nothing is retried once the sheet's title has had time to appear.
        let prSheet = app.staticTexts["Select a pull request"].firstMatch
        anyElement(app, identified: "agent-composer-fix-conflicts-pr").tap()
        if !prSheet.waitForExistence(timeout: 10) {
            anyElement(app, identified: "agent-composer-fix-conflicts-pr").tap()
            XCTAssertTrue(prSheet.waitForExistence(timeout: 20), "The PR picker did not open")
        }
        // A sheet row is a Button whose label is the option's `#N · IDENT`
        // (the issue picker above matches its rows the same way, any type).
        let prRow = app.descendants(matching: .any).matching(
            NSPredicate(format: "label BEGINSWITH %@ AND label CONTAINS %@", "#", "APP-14")
        ).firstMatch
        XCTAssertTrue(prRow.waitForExistence(timeout: 20), "The PR picker never listed APP-14's pull request")
        prRow.tap()
        _ = prRow.waitForNonExistence(timeout: 10)
        XCTAssertTrue(
            anyElement(app, identified: "agent-composer-chip-issue-APP-14").waitForExistence(timeout: 15),
            "No APP-14 chip after picking its pull request"
        )
        snapshot("sg_chat-action", settle: 2)
        // The Agent page is a bar root now (the chat arm switches to it) —
        // back to Devices through the bar.
        let devicesTabAgain = app.buttons["tab-devices"]
        XCTAssertTrue(devicesTabAgain.waitForExistence(timeout: 15), "Devices tab missing")
        devicesTabAgain.tap()
        XCTAssertTrue(
            app.navigationBars["Devices"].waitForExistence(timeout: 30),
            "Did not return to the Devices surface"
        )
        settle(1)

        // ── sg_machine-settings: the device settings sheet ───────────────────
        // EXP-909: a device row carries ONE control, the settings gear, and
        // only on own registered machines — which is why the relay stub is a
        // prerequisite. One tap, no menu hop.
        let machineSettings = app.buttons["machine-settings"].firstMatch
        XCTAssertTrue(
            machineSettings.waitForExistence(timeout: 20),
            "No device settings gear — the stub device must be the demo user's OWN, registered machine"
        )
        machineSettings.tap()
        let deviceSheet = anyElement(app, identified: "device-settings-sheet")
        XCTAssertTrue(deviceSheet.waitForExistence(timeout: 20), "Device settings sheet did not open")
        snapshot("sg_machine-settings", settle: 2)
        // EXP-694: the sheet autosaves and has no Done button — swipe it away.
        dismissSheet(app, whileVisible: deviceSheet)
        _ = deviceSheet.waitForNonExistence(timeout: 10)
        settle(1)

        // ── The Actions surface: five shots off one tab ──────────────────────
        // EXP-1187: Actions is a tab of its own (the More menu is gone).
        let actionsBarTab = app.buttons["tab-actions"]
        XCTAssertTrue(actionsBarTab.waitForExistence(timeout: 15), "Actions tab missing")
        actionsBarTab.tap()
        XCTAssertTrue(
            app.navigationBars["Actions"].waitForExistence(timeout: 30),
            "Actions surface never appeared"
        )
        // The segment choice is persisted in @AppStorage, so a retry after a
        // mid-Actions failure would land on Suggestions — select
        // the Actions segment explicitly (the tap is idempotent). The segments
        // carry identifiers (EXP-686) because "Actions" also reads as the tab
        // and the nav bar title.
        let actionsSegment = anyElement(app, identified: "actions-segment-actions")
        XCTAssertTrue(actionsSegment.waitForExistence(timeout: 15), "Actions segment missing")
        actionsSegment.tap()
        XCTAssertTrue(
            app.staticTexts[Self.seededActionName].firstMatch.waitForExistence(timeout: 60),
            "The Actions segment never listed the seeded team actions"
        )

        // ── sg_action-create: the composer on the Create action builtin ─────
        // "New action" rides the "Actions · count" section header (EXP-574).
        // EXP-825: it pushes the Agent page with the "Create action" builtin
        // picked (the action chip proves it) — describe it, and the creator
        // run writes the action. Only photographed, never submitted —
        // submitting would start a real builtin run on somebody's machine.
        let newActionButton = app.buttons["New action"]
        XCTAssertTrue(newActionButton.waitForExistence(timeout: 20), "New action entry missing")
        newActionButton.tap()
        let createComposer = anyElement(app, identified: "agent-composer")
        XCTAssertTrue(createComposer.waitForExistence(timeout: 20), "Agent page did not open")
        XCTAssertTrue(
            anyElement(app, identified: "agent-composer-chip-action").waitForExistence(timeout: 15),
            "The composer never picked the Create action builtin"
        )
        XCTAssertTrue(
            app.buttons["Run action"].firstMatch.waitForExistence(timeout: 15),
            "The composer never settled on the action subject"
        )
        snapshot("sg_action-create", settle: 2)
        // A pushed detail: back pops to the Actions tab.
        goBack(app)
        XCTAssertTrue(
            app.navigationBars["Actions"].waitForExistence(timeout: 30),
            "Did not return to the Actions surface"
        )
        settle(1)

        // ── sg_action-triggers: the action page on its Triggers tab ─────────
        // SLOP-2: an action carries its triggers. A row's body pushes the
        // action page (Prompt · Triggers · Runs as tabs on a pager); the
        // seeded "Nightly test triage" owns a daily schedule trigger and one
        // scheduled run. Gated on the tab's OWN content rather than on a
        // seeded trigger: an older seed shows the empty note — which is still
        // a legitimate capture of this surface, unlike a half-synced list.
        let seededAction = app.staticTexts[Self.seededActionName].firstMatch
        seededAction.tap()
        XCTAssertTrue(
            anyElement(app, identified: "action-tabs").waitForExistence(timeout: 30),
            "The action page did not open"
        )
        let triggersTab = anyElement(app, identified: "action-tab-triggers")
        XCTAssertTrue(triggersTab.waitForExistence(timeout: 15), "Triggers tab missing")
        triggersTab.tap()
        let triggerRow = anyElement(app, identified: "trigger-row")
        if !triggerRow.waitForExistence(timeout: 45) {
            print("SLOP-2 sg_action-triggers: no trigger rows — reseed with `bun run seed:screenshots`")
            XCTAssertTrue(
                app.staticTexts["No triggers. This action runs when someone starts it."]
                    .waitForExistence(timeout: 15),
                "The Triggers tab rendered neither rows nor its empty note"
            )
        }
        snapshot("sg_action-triggers", settle: 2)

        // ── sg_trigger-editor: the trigger form sheet ────────────────────────
        // "Add trigger" is owner-only AND steer-gated (hidden when the backend
        // has no STEER_RELAY_URL, since nothing could ever fire the trigger),
        // and so is a row's Edit — the form is the ONE trigger editor.
        let addTriggerButton = app.buttons["Add trigger"]
        XCTAssertTrue(
            addTriggerButton.waitForExistence(timeout: 15),
            "No \"Add trigger\" entry — the demo user must own the team and the backend needs STEER_RELAY_URL"
        )
        addTriggerButton.tap()
        let triggerSheet = anyElement(app, identified: "trigger-form-sheet")
        XCTAssertTrue(
            triggerSheet.waitForExistence(timeout: 20),
            "The trigger form sheet did not open"
        )
        snapshot("sg_trigger-editor", settle: 2)
        dismissSheet(app, whileVisible: triggerSheet)
        _ = triggerSheet.waitForNonExistence(timeout: 10)
        settle(1)

        // ── sg_action-runs: the action page on its Runs tab ─────────────────
        // Every run of the action, newest first; the seeded scheduled run
        // reads "Scheduled run". Same gate as the Triggers tab: rows, or
        // the empty note on an older seed.
        let runsTab = anyElement(app, identified: "action-tab-runs")
        XCTAssertTrue(runsTab.waitForExistence(timeout: 15), "Runs tab missing")
        runsTab.tap()
        // EXP-1248: live and ended runs are the same SessionRow now.
        let actionRunRow = anyElement(app, identified: "action-run-row")
        if !actionRunRow.waitForExistence(timeout: 45) {
            print("SLOP-2 sg_action-runs: no run rows — reseed with `bun run seed:screenshots`")
            XCTAssertTrue(
                app.staticTexts["No runs yet."].waitForExistence(timeout: 15),
                "The Runs tab rendered neither rows nor its empty note"
            )
        }
        snapshot("sg_action-runs", settle: 2)
        // A pushed detail: back pops to the Actions tab.
        goBack(app)
        XCTAssertTrue(
            app.navigationBars["Actions"].waitForExistence(timeout: 30),
            "Did not return to the Actions surface"
        )
        settle(1)

        // ── sg_action-suggestions: the Suggestions segment ────────────────────
        // Shipped constants (`ActionSuggestion.seeds`), not seeded rows — this
        // one can be gated hard on a row.
        let suggestionsSegment = anyElement(app, identified: "actions-segment-suggestions")
        XCTAssertTrue(suggestionsSegment.waitForExistence(timeout: 15), "Suggestions segment missing")
        suggestionsSegment.tap()
        XCTAssertTrue(
            anyElement(app, identified: "suggestion-row").waitForExistence(timeout: 20),
            "The Suggestions segment never rendered its seed cards"
        )
        snapshot("sg_action-suggestions", settle: 2)
        // Leave the surface on Actions so a retry starts where it started.
        actionsSegment.tap()

        // ── sg_reviews: the cross-board open-PR queue ───────────────────────
        let reviewsTab = app.buttons["tab-reviews"]
        XCTAssertTrue(reviewsTab.waitForExistence(timeout: 15), "Reviews tab missing")
        reviewsTab.tap()
        XCTAssertTrue(
            reviewRow(app, titled: Self.reviewTitle).waitForExistence(timeout: 60),
            "Reviews tab never showed the seeded open PRs"
        )
        snapshot("sg_reviews", settle: 2)

        // ── sg_pr-row: THE pull-request row (EXP-1248) ──────────────────────
        // The same queue IS the PrRow surface: one line per PR, a tree nested
        // with guides, a stack on its rail over the base-branch row.
        XCTAssertTrue(
            anyElement(app, identified: "pr-row").waitForExistence(timeout: 30),
            "Reviews drew no PrRow"
        )
        snapshot("sg_pr-row", settle: 1)

        // ── sg_guide / sg_guide-section: the Work screen's Guide (EXP-1251) ─
        // A Reviews row opens its issue on the Guide face: APP-14's real PR has
        // no report, so its whole diff is ONE Changes section; that row opens
        // the section page. Both pop back to Reviews for sg_run-changes.
        let wantsGuide = ScreenshotShots.isWanted("sg_guide")
        let wantsGuideSection = ScreenshotShots.isWanted("sg_guide-section")
        if wantsGuide || wantsGuideSection {
            reviewRow(app, titled: Self.guideTitle).tap()
            let guideTab = anyElement(app, identified: "work-face-guide")
            if guideTab.waitForExistence(timeout: 20) { guideTab.tap() }
            let changesRow = anyElement(app, identified: "guide-changes-row")
            // Below the fold on a phone (the PR body comes first), so scroll
            // to it; the shot then shows the body's end, the Changes row and
            // Show complete diff.
            revealGuideRow(app, [changesRow], deadline: Date().addingTimeInterval(60))
            XCTAssertTrue(
                changesRow.exists,
                "The Guide drew no Changes row — are the seeded PR's files reachable on GitHub?"
            )
            snapshot("sg_guide", settle: 2)
            changesRow.tap()
            let sectionPage = anyElement(app, identified: "guide-section-page")
            XCTAssertTrue(sectionPage.waitForExistence(timeout: 30), "The Guide section page did not open")
            snapshot("sg_guide-section", settle: 2)
            anyElement(app, identified: "guide-section-back").tap()
            _ = anyElement(app, identified: "work-face-guide").waitForExistence(timeout: 15)
            goBack(app)
            XCTAssertTrue(
                reviewRow(app, titled: Self.reviewTitle).waitForExistence(timeout: 30),
                "Back from the Guide did not land on Reviews"
            )
            settle(1)
        }

        // ── sg_run-changes: an issue-less run's PR diff (EXP-1194/1204) ─────
        // The Reviews "Agent runs" band lists Jonas's finished chat run, whose
        // own pull request is open; its row pushes `RunChangesView` (the
        // run's Guide, EXP-1251), fed by `codingSessions.prFiles` — a real
        // public PR the seed points the run at (SCREENSHOT_RUN_PR_URL). Its
        // Changes row opens the section page, where the file cards live.
        let runRow = reviewRow(app, titled: Self.runChangesTitle)
        XCTAssertTrue(
            runRow.waitForExistence(timeout: 60),
            "Reviews tab never showed the seeded agent run"
        )
        runRow.tap()
        openGuideDiff(
            app,
            failure: "The run's PR diff never loaded — check SCREENSHOT_RUN_PR_URL is a reachable public PR"
        )
        snapshot("sg_run-changes", settle: 2)
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
        // The wizard opens on web's choice page: the mark over "Welcome to
        // Exponential" and the two outline buttons (polish round ×4).
        XCTAssertTrue(
            app.staticTexts["Welcome to Exponential"].waitForExistence(timeout: 90),
            "The onboarding wizard never appeared for \(ScreenshotSeed.newcomerEmail) — reseed with `bun run seed:screenshots`"
        )
        XCTAssertTrue(
            app.buttons["team-setup-create"].waitForExistence(timeout: 30),
            "The team step never rendered its Create a team choice"
        )
        snapshot("sg_onboarding", settle: 2)

        // ── sg_onboarding-invite / sg_onboarding-devices ─────────────────────
        // The wizard's last two steps (EXP-725) need a RESOLVED team, which
        // the newcomer has not got — creating one would mutate the seed. The
        // starter identity owns one and is still un-onboarded, so signing in
        // as them and relaunching with `-uiTestingOnboardingStep invite`
        // parks the wizard on step 3. NOTHING is submitted here either: no
        // invite is minted, no board is created.
        //
        // The wizard's persistent "Sign out" (EXP-725) is the way off the
        // newcomer's session — the root becomes the LoginView for the same
        // instance, exactly as the ServerDetail sign-out above did.
        app.buttons["Sign out"].firstMatch.tap()
        submitLogin(
            app,
            email: ScreenshotSeed.starterEmail,
            password: ScreenshotSeed.starterPassword
        )

        // The keychain account survives the relaunch, so the app boots
        // straight back into the wizard — with the capture hook armed.
        app.terminate()
        app.launchArguments += ["-uiTestingOnboardingStep", "invite"]
        app.launch()

        let inviteStep = app.otherElements["onboarding-invite-step"]
        XCTAssertTrue(
            inviteStep.waitForExistence(timeout: 90),
            "The wizard never parked on its invite step — is -uiTestingOnboardingStep wired?"
        )
        // Gate on the real control, not just the container: at the seat cap
        // the creator renders NOTHING (App Store 3.1.1) and the shot would be
        // a blank styleguide page.
        XCTAssertTrue(
            app.buttons["invite-generate"].waitForExistence(timeout: 30),
            "The invite step never rendered its Generate button — is the starter team over its seat cap?"
        )
        snapshot("sg_onboarding-invite", settle: 2)

        app.buttons["Skip for now"].firstMatch.tap()
        XCTAssertTrue(
            app.otherElements["onboarding-devices-step"].waitForExistence(timeout: 30),
            "The wizard never reached its devices step"
        )
        snapshot("sg_onboarding-devices", settle: 2)

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

    /// A Reviews row by its title. EXP-1248: a row is a plain Button over
    /// `PrRow`, which COMBINES its texts into one element, so the title is no
    /// standalone staticText: match the button whose label contains it.
    @MainActor
    private func reviewRow(_ app: XCUIApplication, titled title: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", title)).firstMatch
    }

    /// EXP-1251: a review lands on the Guide, which draws Changes rows, never
    /// file cards: open the diff (the first Changes row, else Show complete
    /// diff) as the section page and wait for its file cards.
    /// The Guide body is a LazyVStack: a row below the fold is not in the
    /// accessibility tree until it scrolls on screen. Swipe up until one of
    /// `rows` exists or the deadline passes (a loading Guide just keeps
    /// waiting; the swipes are harmless on a short page).
    @MainActor
    private func revealGuideRow(_ app: XCUIApplication, _ rows: [XCUIElement], deadline: Date) {
        while Date() < deadline && !rows.contains(where: { $0.exists }) {
            if rows.contains(where: { $0.waitForExistence(timeout: 2) }) { return }
            app.swipeUp()
        }
    }

    @MainActor
    private func openGuideDiff(_ app: XCUIApplication, failure: String) {
        // run-changes = the COMPLETE diff page (section=all), as web; a
        // report-less Guide's one Changes section already is the whole diff.
        let changesRow = anyElement(app, identified: "guide-changes-row")
        let completeDiff = anyElement(app, identified: "guide-show-complete-diff")
        revealGuideRow(app, [completeDiff], deadline: Date().addingTimeInterval(45))
        if completeDiff.exists {
            completeDiff.tap()
        } else {
            revealGuideRow(app, [changesRow], deadline: Date().addingTimeInterval(15))
            XCTAssertTrue(changesRow.exists, failure)
            changesRow.tap()
        }
        XCTAssertTrue(
            anyElement(app, identified: "guide-section-page").waitForExistence(timeout: 30),
            "The Guide section page did not open"
        )
        XCTAssertTrue(
            anyElement(app, identified: "changes-file-row").waitForExistence(timeout: 60),
            failure
        )
    }
}
