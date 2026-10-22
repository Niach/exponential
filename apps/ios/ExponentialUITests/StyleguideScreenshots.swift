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
///   sg_chat · sg_composer-menu · sg_chat-issues ·
///   sg_machine-settings · sg_settings-team-agent · sg_threads ·
///   sg_session-row · sg_pr-row · sg_guide · sg_guide-section ·
///   sg_settings-root · sg_settings-team ·
///   sg_settings-account · sg_onboarding · sg_onboarding-invite ·
///   sg_onboarding-devices · sg_agent-home · sg_search-list ·
///   sg_getting-started · sg_feedback-states · sg_work-card ·
///   sg_script-card · sg_comment-call-card · sg_thread · sg_agent-channel ·
///   sg_session · sg_trace · sg_code-block · sg_ask-dock · sg_plan-card
///
/// EXP-1295 (Threads v2) repointed the shots whose screens left (Reviews,
/// the work faces, the Recent sheet, the builtin actions, the action page)
/// and retired `sg_chat-action`, `sg_action-runs`, `sg_action-create`,
/// `sg_run-changes` and `sg_trigger-editor`. EXP-1309 then moved every
/// Threads v2 shot off its hand-written specimen onto the LIVE app over the
/// seed web/desktop photograph, so they cannot drift from it again:
///   * `sg_agent-home` = the dawn home the app lands on (`agent-page`), and
///     `sg_session-row` = the SAME screen gated on THE session row in its
///     Running band (web/desktop photograph a styleguide island of the row;
///     the phone's one honest instance of it is the seeded band);
///   * `sg_threads` = the Threads tab (Agents band, Needs you · Running ·
///     Recent, each row's quiet word off its pull requests);
///     `sg_agent-channel` = the Triage agent's channel off its Agents row;
///     `sg_thread` = the cold-start thread (DEMO_THREAD_ID); `sg_session` =
///     its "Profile the launch path" session card's page in that thread
///     (EXP-1343; DEMO_SESSION_ID, fed by the relay stub), its pending
///     question in the composer's place;
///   * `sg_settings-team-agent` = Settings → team → Agents → Triage;
///   * `sg_pr-row` = the issue page's pull-request row, `sg_guide` = the
///     Guide tab of APP-15's thread (web's `guide` thread, EXP-1303),
///     `sg_guide-section` = APP-14's Changes section page.
///
/// EXP-1284 added `sg_thread` + `sg_agent-channel` (the THREAD screen).
///
/// EXP-1259 added the last four: the Agent HOME the app lands on (the
/// greeting over the composer, nothing beneath it), the `#` typeahead's
/// search-list sheet, Getting started's block (Settings' entry sheet) and the
/// Feedback states screen (`FeedbackStatesScreen`, opened by
/// `-uiTestingStyleguide feedback-states` on a signed-out relaunch). The run thread's three cards
/// (`sg_work-card`, `sg_script-card`, `sg_comment-call-card`, EXP-1240/1279/
/// 1241) ride the same seam (`ThreadCardsScreen`).
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
/// the empty "Launch Marketing", showcase issue APP-5, open PRs and the
/// threads holding them, the team agents, actions) PLUS, since EXP-642, the
/// relay stub: `bun run screenshots:desktop` (apps/web) registers the demo
/// user's OWN device row, which is what `sg_machine-settings` (gated `isMine
/// && registered`) and the `sg_chat*` shots photograph, and (EXP-1309)
/// publishes the scripted feed `sg_session` watches over the steer relay.
///
/// Every shot is gated on real seeded content, never on a container element —
/// an empty list still renders its container and would silently ship a blank
/// styleguide page. The capture-only card + feedback screens at the end draw
/// fake rows by design (no seed counterpart). Shared
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
    /// EXP-1341: the cold-start thread (DEMO_THREAD_ID) is titled off its
    /// linked issues (APP-5, APP-10, FB-3), session-title.json `titles.issues`:
    /// `#APP-5 Reduce cold start below 800 ms + 2`; matched on the issue title.
    private static let coldStartThreadTitle = "Reduce cold start below 800 ms"
    /// First comment on APP-5, plain text before any markdown decoration.
    private static let showcaseCommentFragment = "Profiled on a mid-range device"
    /// Assigned to the demo user, so it is on My Issues.
    private static let myIssueTitle = "Dark mode contrast pass across settings"
    /// APP-14: the seed's realPr issue, so its thread's Guide has files from
    /// GitHub.
    private static let guideTitle = "Group board issues by assignee"
    /// APP-15: its thread is web's `guide` thread (`DEMO_GUIDE_THREAD_ID`),
    /// the seeded Guide's Summary + one section, its PR the stack's bottom.
    private static let batchGuideTitle = "Batch-edit labels from the board"
    private static let batchGuideSection = "Label picker on the selection bar"
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
    /// The device `bun run screenshots:desktop` registers for the demo user.
    private static let demoDeviceName = "Alex's MacBook Pro"
    /// The login that device announces for its agents (`screenshot-demo.ts`
    /// `DEMO_AGENT_STATUS`) — EXP-909: the first login row UNDER the machine
    /// row names it.
    private static let demoAccountEmail = "demo@exponential.at"
    /// The Triage agent (`DEMO_AGENT_ID`): its channel and settings page.
    private static let agentName = "Triage"
    /// `DEMO_AGENT_REQUEST`, the person's ask in Triage's channel.
    private static let agentRequest = "Label the Android ones too"
    /// The cold-start thread's child session (`DEMO_SESSION_ID`).
    private static let sessionTitle = "Profile the launch path"
    /// Its id (`DEMO_SESSION_ID`): the strip chip of its thread page.
    private static let sessionId = "7c1f0a52-6f3c-4e19-9a2b-2d5d0f7c8e41"

    @MainActor
    func testCaptureStyleguideScreenshots() throws {
        continueAfterFailure = false

        let app = makeScreenshotApp()

        // ── sg_sign-in: the pre-login server chooser ─────────────────────────
        // EXP-1267: the simulator is no longer erased; `launchSignedOut`
        // relaunches the app in a fresh-install state with no account, so it
        // always boots onto InstanceView — its cloud (Apple / Google) buttons
        // plus the "Use a self-hosted instance" link, untouched.
        // `awaitLaunchStage` deliberately taps NOTHING, so this is the state a
        // first-run user sees, and it is what the web/desktop `sign-in` shots
        // show too.
        launchSignedOut(app)
        let launch = awaitLaunchStage(app)
        guard launch == .instancePicker else {
            XCTFail("sg_sign-in: expected the untouched instance picker, found \(launch) — is the -uiTestingReset seam compiled in (DEBUG)?")
            return
        }
        snapshot("sg_sign-in", settle: 1)

        // The password form is deliberately NOT photographed any more
        // (EXP-642): relaunch signed in with the reused demo session (the
        // login UI only as the fallback).
        let injected = launchSignedIn(app)

        // ── sg_agent-home: the home the app lands on (EXP-1253/1295) ────────
        // EXP-1316: the column pinned to the bottom, the dawn standing on the
        // composer card, its options line under it; no Running band.
        expect(
            anyElement(app, identified: "agent-page"),
            within: Wait.firstSync,
            "The app did not land on the home"
        )
        expect(
            anyElement(app, identified: "agent-composer"),
            within: Wait.sync,
            "The home drew no composer"
        )
        snapshot("sg_agent-home", settle: 2)

        // ── sg_session-row: THE session row (EXP-1248/1309) ─────────────────
        // EXP-1316: Home lost its Running band — the rows live on the Threads
        // tab. Gate on the cold-start thread's row there (it waits on me),
        // with the private spike and the busy ones beside it.
        let threadsTab = app.buttons["tab-threads"]
        expect(threadsTab, within: Wait.sync, "Threads tab missing")
        threadsTab.tap()
        expect(
            threadRow(app, containing: Self.coldStartThreadTitle),
            within: Wait.sync,
            "The Threads tab never listed the cold-start thread — reseed with `bun run seed:screenshots`"
        )
        snapshot("sg_session-row", settle: 2)

        // Switch to Issues for the board.
        let issuesTab = app.buttons["tab-issues"]
        expect(issuesTab, within: Wait.sync, "Tab bar never appeared")
        issuesTab.tap()

        // The FIRST sync after sign-in: the one long wait of the run.
        let showcaseRowTitle = app.staticTexts[Self.showcaseTitle]
        expect(
            showcaseRowTitle,
            within: Wait.firstSync,
            "Issue list never synced (missing showcase issue \(Self.showcaseTitle))"
        )
        if !injected { dismissSavePasswordSheet(timeout: 3) }

        // ── sg_board-switcher: the server → team → board bottom sheet ────────
        // The trigger is the board-name control in the pinned nav row; its
        // accessibility LABEL is the same string as the sheet's headline, so
        // scope the tap to the button and the assertion to staticTexts.
        let switcherButton = app.buttons["Switch board"]
        expect(switcherButton, within: Wait.nav, "Board switcher trigger missing")
        switcherButton.tap()
        let switcherHeadline = app.staticTexts["Switch board"]
        expect(switcherHeadline, within: Wait.nav, "Board switcher sheet did not open")
        // The team block header is the real content — the sheet chrome alone
        // renders before the boards have been loaded off the synced rows.
        expect(
            app.staticTexts[Self.teamName].firstMatch,
            within: Wait.sync,
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
        expect(newTeamRow, within: Wait.nav, "Board switcher has no New team row")
        // The sheet is fitted to its content, so the row is on screen for the
        // seeded team — scroll it up only if a longer board list ever pushes it
        // under the fold.
        scrollUntilVisible(app, newTeamRow, attempts: 3)
        newTeamRow.tap()
        let teamSetupSheet = app.descendants(matching: .any)["team-setup-sheet"].firstMatch
        expect(
            teamSetupSheet,
            within: Wait.nav,
            "New team row did not open the team setup sheet"
        )
        // The sheet opens on the choice page (polish round ×4); its "Create
        // a team" button PUSHES the create form this view photographs. Gate
        // on real content, not the container.
        let createTeamChoice = app.buttons["team-setup-create"]
        expect(
            createTeamChoice,
            within: Wait.nav,
            "Team setup sheet never rendered its Create a team choice"
        )
        createTeamChoice.tap()
        expect(
            app.staticTexts["Team name"],
            within: Wait.nav,
            "Create a team never pushed its form"
        )
        snapshot("sg_onboarding-create-team", settle: 1)
        // Nothing was submitted, so no team is created and the next steps start
        // from the same board list. The switcher is already gone — this only
        // has to close the setup sheet.
        dismissSheet(app, whileVisible: app.staticTexts["Set up a team"].firstMatch)
        expect(
            showcaseRowTitle,
            within: Wait.nav,
            "Dismissing the team setup sheet did not return to the board list"
        )

        // ── sg_board-empty: a board with no issues on it ─────────────────────
        // The seed's second board ("Launch Marketing") is created empty for
        // exactly this shot, so nothing has to be deleted to reach the state.
        switchBoard(app, to: Self.emptyBoardName)
        expect(
            app.staticTexts["No issues yet"],
            within: Wait.sync,
            "\(Self.emptyBoardName) did not render its empty state"
        )
        snapshot("sg_board-empty", settle: 2)
        switchBoard(app, to: Self.seededBoardName)
        expect(
            showcaseRowTitle,
            within: Wait.sync,
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
        expect(
            bulkBar,
            within: Wait.nav,
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
        _ = bulkBar.waitForNonExistence(timeout: Wait.nav)

        // ── sg_issue-comments: APP-5 scrolled to its comment thread ──────────
        // Tap the row's TITLE text, never the `issue-row-*` element (EXP-348).
        expect(showcaseRowTitle, within: Wait.nav, "Did not return to the board")
        openIssue(app, title: Self.showcaseTitle)
        let commentsHeader = app.staticTexts["comment-thread-header"]
        expect(commentsHeader, within: Wait.nav, "Issue detail did not open")
        // The comments shape can lag the issues shape by tens of seconds right
        // after the first login — gate on a real comment body, not the header.
        expect(
            anyElement(app, containing: Self.showcaseCommentFragment),
            within: Wait.sync,
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
            expect(propertiesButton, within: Wait.nav, "Properties button missing on the issue detail")
            let propertiesHeadline = app.staticTexts["Properties"]
            // ONE tap, on purpose (EXP-1160): this is the first sheet the
            // page presents since the screen opened, the case the paged
            // `TabView` broke (it presented the sheet twice and tore both
            // down). A retry here would hide that regression.
            propertiesButton.tap()
            expect(
                propertiesHeadline,
                within: Wait.nav,
                "Properties sheet did not open on its first tap"
            )
            expect(
                anyElement(app, containing: "Priority"),
                within: Wait.nav,
                "Properties sheet never showed its property rows"
            )
            snapshot("sg_issue-properties", settle: 2)
            // EXP-687: no sheet has a close button any more. Let the sheet
            // finish animating out before the nav-bar back tap, or that tap
            // lands on the dismissing sheet.
            dismissSheet(app, whileVisible: propertiesHeadline)
            _ = propertiesHeadline.waitForNonExistence(timeout: Wait.nav)
            settle(1)
        }
        goBack(app)

        // ── sg_issue-create: the new-issue page ─────────────────────────────
        // The compose button is only mounted on board routes (AppNavigator
        // `composeTarget`), so come back to the issues tab first. The title
        // field takes focus on appear, so the page is captured with the
        // keyboard up — which is the state a user actually sees.
        app.buttons["tab-issues"].tap()
        expect(showcaseRowTitle, within: Wait.nav, "Board did not come back for the compose shot")
        let composeButton = app.buttons["compose-button"]
        expect(composeButton, within: Wait.nav, "Compose button missing on the board")
        composeButton.tap()
        let titleField = app.textFields["issue-title-field"]
        expect(titleField, within: Wait.nav, "Create-issue page did not open")
        focus(titleField)
        titleField.typeText("Prefetch avatars before the first board paint")
        snapshot("sg_issue-create", settle: 2)
        // EXP-1170: the page AUTOSAVES its draft while the title is typed, and
        // the styleguide run must not leave anything in the seed — clear the
        // title first: Back from an emptied page DELETES the transient row the
        // autosave wrote (and writes nothing when none was written yet).
        clearText(of: titleField)
        app.buttons["Back"].firstMatch.tap()
        // A clear that left a character behind (a slow keyboard) keeps
        // content on the page, so Back asks the EXP-1212 leave prompt:
        // Discard it — the run must leave nothing in the seed either way.
        let discard = app.buttons["Discard"].firstMatch
        if discard.waitForExistence(timeout: 2) { discard.tap() }
        _ = titleField.waitForNonExistence(timeout: Wait.nav)

        // ── sg_search: the search view with seeded results ───────────────────
        // EXP-686: Search lost its tab — it is a push off the board header.
        let searchButton = app.buttons["board-search"]
        expect(searchButton, within: Wait.nav, "Board search button missing")
        searchButton.tap()
        let searchField = app.textFields["search-field"]
        expect(searchField, within: Wait.nav, "Search field missing")
        focus(searchField)
        searchField.typeText(Self.searchQuery)
        // SLOP-28: the result is THE search-list row, labelled
        // "IDENT Title" — match the title inside it.
        expect(
            anyElement(app, containing: Self.showcaseTitle),
            within: Wait.sync,
            "Search never returned the seeded issue for \"\(Self.searchQuery)\""
        )
        snapshot("sg_search", settle: 2)
        goBack(app)

        // ── sg_my-issues: Inbox → the "My issues" segment ────────────────────
        // The segment is a GlassSegmentedControl button carrying only an
        // accessibility LABEL. Its choice is persisted in @AppStorage, so the
        // tap is deliberately unconditional (it is idempotent).
        let myWorkTab = app.buttons["tab-mywork"]
        expect(myWorkTab, within: Wait.nav, "Inbox tab missing")
        myWorkTab.tap()
        let myIssuesSegment = app.buttons["My issues"]
        expect(myIssuesSegment, within: Wait.nav, "My issues segment missing")
        myIssuesSegment.tap()
        expect(
            app.staticTexts[Self.myIssueTitle].firstMatch,
            within: Wait.sync,
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
        expect(devicesTab, within: Wait.nav, "Devices tab missing")
        devicesTab.tap()
        expect(
            app.navigationBars["Devices"],
            within: Wait.nav,
            "Devices surface never appeared"
        )
        expect(
            app.staticTexts[Self.demoDeviceName].firstMatch,
            within: Wait.network,
            "No \(Self.demoDeviceName) row — is `bun run screenshots:desktop` running?"
        )
        // EXP-944: device rows are COLLAPSED by default; the shot opens the
        // demo device so its logins and their usage are in frame.
        let deviceRow = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'device-row-'"))
            .firstMatch
        expect(deviceRow, within: Wait.nav, "No foldable device row")
        deviceRow.tap()
        // EXP-909: the machine's own logins sit under its row — the subject
        // the retired `sg_usage` shot used to have a section of its own for.
        expect(
            anyElement(app, containing: Self.demoAccountEmail),
            within: Wait.network,
            "No \(Self.demoAccountEmail) login row — is the stub device reporting agent accounts?"
        )
        snapshot("sg_agents", settle: 2)

        // ── sg_chat / sg_chat-issues: the home composer ─────────────────────
        // EXP-825: the ONE launcher. EXP-909: a device row starts nothing any
        // more (its one control is the settings gear), so the bar's Chat
        // arm opens the home on the default device: an empty composer
        // starts a free request; Implement issue › chips two issues.
        // Nothing is ever submitted — a session would land on a real machine.
        let chatButton = app.buttons["chat-button"].firstMatch
        expect(
            chatButton,
            within: Wait.sync,
            "The bar offers no Chat arm — is the demo team's device online with an agent?"
        )
        chatButton.tap()
        let composer = anyElement(app, identified: "agent-composer")
        expect(composer, within: Wait.nav, "Agent page did not open")
        // The submit label proves the composer resolved its subject (a chat).
        expect(
            app.buttons["Start chat"].firstMatch,
            within: Wait.sync,
            "The composer never settled on the chat subject"
        )
        snapshot("sg_chat", settle: 2)

        // EXP-1249: the "+" is the composer's ONLY tool — its sheet of the
        // `composer-menu.json` rows is `sg_composer-menu`. SLOP-28: Implement
        // issue › PUSHES its search-list page inside the same sheet, whose
        // `Implement N issues` button closes the menu.
        anyElement(app, identified: "agent-composer-plus-button").tap()
        let plusMenu = anyElement(app, identified: "agent-composer-menu")
        expect(plusMenu, within: Wait.nav, "The + menu did not open")
        snapshot("sg_composer-menu", settle: 2)
        anyElement(app, identified: "agent-composer-menu-implement-issue").tap()
        let issuePicker = anyElement(app, identified: "agent-composer-issues-picker")
        expect(issuePicker, within: Wait.nav, "Issue picker did not open")
        for title in [Self.bulkFirstTitle, Self.bulkSecondTitle] {
            // EXP-1030: a picker row reads `IDENT Title` in ONE label (the ×4
            // contract), so it is matched on the title as a fragment.
            let row = anyElement(app, containing: title)
            expect(row, within: Wait.sync, "Issue picker never listed \"\(title)\"")
            row.tap()
        }
        // SLOP-28: the page's footer button closes the whole menu (the picks
        // already are the subject).
        let implementButton = anyElement(app, identified: "agent-composer-implement")
        expect(implementButton, within: Wait.nav, "No Implement button after picking")
        implementButton.tap()
        _ = issuePicker.waitForNonExistence(timeout: Wait.nav)
        let issueChip = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "agent-composer-chip-issue-")
        ).firstMatch
        expect(issueChip, within: Wait.nav, "No issue chip after picking")
        snapshot("sg_chat-issues", settle: 2)

        // ── sg_search-list: the `#` typeahead's sheet (SLOP-28) ─────────────
        // Typing `#` in the composer opens THE search list as a bottom sheet,
        // its input focused and prefilled with the token after the `#`.
        let composerField = anyElement(app, identified: "agent-composer-field")
        composerField.tap()
        app.typeText("Explain #")
        let refSheet = anyElement(app, identified: "issue-ref-search-sheet")
        expect(refSheet, within: Wait.nav, "Typing # did not open the search-list sheet")
        app.typeText("cold")
        expect(
            anyElement(app, containing: Self.showcaseTitle),
            within: Wait.sync,
            "The # sheet never listed the seeded issue for \"cold\""
        )
        snapshot("sg_search-list", settle: 2)
        dismissSheet(app, whileVisible: refSheet)
        _ = refSheet.waitForNonExistence(timeout: Wait.nav)
        // The composer keeps focus, and the keyboard hides the floating bar:
        // empty it (nothing left behind) and drag the page to dismiss it.
        clearText(of: composerField)
        for _ in 0..<3 where app.keyboards.firstMatch.exists {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3))
                .press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.9)))
            usleep(600_000)
        }
        // The Agent page is a bar root now (the chat arm switches to it) —
        // back to Devices through the bar.
        let devicesTabAgain = app.buttons["tab-devices"]
        expect(devicesTabAgain, within: Wait.nav, "Devices tab missing")
        devicesTabAgain.tap()
        expect(
            app.navigationBars["Devices"],
            within: Wait.nav,
            "Did not return to the Devices surface"
        )
        settle(1)

        // ── sg_machine-settings: the device settings sheet ───────────────────
        // EXP-909: a device row carries ONE control, the settings gear, and
        // only on own registered machines — which is why the relay stub is a
        // prerequisite. One tap, no menu hop.
        let machineSettings = app.buttons["machine-settings"].firstMatch
        expect(
            machineSettings,
            within: Wait.sync,
            "No device settings gear — the stub device must be the demo user's OWN, registered machine"
        )
        machineSettings.tap()
        let deviceSheet = anyElement(app, identified: "device-settings-sheet")
        expect(deviceSheet, within: Wait.nav, "Device settings sheet did not open")
        snapshot("sg_machine-settings", settle: 2)
        // EXP-694: the sheet autosaves and has no Done button — swipe it away.
        dismissSheet(app, whileVisible: deviceSheet)
        _ = deviceSheet.waitForNonExistence(timeout: Wait.nav)
        settle(1)

        // ── sg_pr-row: THE pull-request row (EXP-1248/1295) ──────────────────
        // An issue page lists its pull requests as rows; APP-14 owns the
        // seed's real open PR.
        app.buttons["tab-issues"].tap()
        openIssue(app, title: Self.guideTitle)
        let prRow = anyElement(app, identified: "issue-pr-row")
        _ = scrollUntilVisible(app, prRow)
        expect(prRow, within: Wait.sync, "APP-14's page drew no pull-request row")
        snapshot("sg_pr-row", settle: 1)

        // ── sg_guide-section: a Guide section page (EXP-1251/1295) ──────────
        // The issue's Threads row opens the thread holding it on the Guide
        // tab: APP-14's real PR has no report, so its whole diff is ONE
        // Changes section; that row opens the section page.
        if ScreenshotShots.isWanted("sg_guide-section") {
            let threadChip = anyElement(app, identified: "issue-thread-chip")
            _ = scrollUntilVisible(app, threadChip)
            expect(threadChip, within: Wait.sync, "APP-14 sits in no thread — reseed with `bun run seed:screenshots`")
            threadChip.tap()
            openThreadGuide(app)
            let changesRow = anyElement(app, identified: "guide-changes-row")
            // Below the fold on a phone (the PR body comes first).
            revealGuideRow(app, [changesRow], deadline: Date().addingTimeInterval(Wait.network))
            XCTAssertTrue(
                changesRow.exists,
                "The Guide drew no Changes row — are the seeded PR's files reachable on GitHub?"
            )
            changesRow.tap()
            let sectionPage = anyElement(app, identified: "guide-section-page")
            expect(sectionPage, within: Wait.nav, "The Guide section page did not open")
            snapshot("sg_guide-section", settle: 2)
            anyElement(app, identified: "guide-section-back").tap()
            goBack(app)
        }
        // Issue → board.
        goBack(app)
        settle(1)

        // ── sg_guide: web's `guide` thread (EXP-1303) ────────────────────────
        // APP-15's thread, the one web/desktop photograph: its seeded Guide
        // (Summary + one section) under THE Merge stack capsule (its PR is
        // the bottom of APP-14's stack).
        if ScreenshotShots.isWanted("sg_guide") {
            openIssue(app, title: Self.batchGuideTitle)
            let threadChip = anyElement(app, identified: "issue-thread-chip")
            _ = scrollUntilVisible(app, threadChip)
            expect(threadChip, within: Wait.sync, "APP-15 sits in no thread — reseed with `bun run seed:screenshots`")
            threadChip.tap()
            openThreadGuide(app)
            expect(app.staticTexts[Self.batchGuideSection], within: Wait.sync, "APP-15's thread drew no seeded Guide")
            expect(app.buttons["Merge stack"], within: Wait.sync, "APP-15's Guide drew no Merge stack capsule")
            snapshot("sg_guide", settle: 2)
            // Thread → issue → board.
            goBack(app)
            goBack(app)
            settle(1)
        }

        // ── sg_thread-replies: the quick replies (EXP-1211) ──────────────────
        // APP-15's thread again, left on its Conversation: the seeded
        // `replies` card's row over the composer capsule.
        if ScreenshotShots.isWanted("sg_thread-replies") {
            openIssue(app, title: Self.batchGuideTitle)
            let threadChip = anyElement(app, identified: "issue-thread-chip")
            _ = scrollUntilVisible(app, threadChip)
            expect(threadChip, within: Wait.sync, "APP-15 sits in no thread — reseed with `bun run seed:screenshots`")
            threadChip.tap()
            let replies = anyElement(app, identified: "thread-replies")
            expect(replies, within: Wait.sync, "APP-15's thread drew no quick replies — reseed with `bun run seed:screenshots`")
            snapshot("sg_thread-replies", settle: 2)
            // Thread → issue → board.
            goBack(app)
            goBack(app)
            settle(1)
        }

        // ── sg_threads · sg_agent-channel · sg_thread · sg_session ──────────
        // EXP-1309: the Threads tab and the screens it opens, LIVE over the
        // seed web/desktop photograph (no specimen to drift). Taken this
        // late on purpose: every shape (issues for the chips, pull requests
        // for the row words) has synced by now, and an agent text parsed
        // before its issues landed would cache without its pills. A detour:
        // it leaves through the Issues tab it came from.
        let threadShots = ["sg_threads", "sg_agent-channel", "sg_thread", "sg_session"]
        if threadShots.map(ScreenshotShots.isWanted).contains(true) {
            walkThreads(app)
            app.buttons["tab-issues"].tap()
            settle(1)
        }

        // ── sg_settings-root: the top-level settings list ────────────────────
        // The gear only lives on the issues tab's nav bar. The root is the
        // Servers / Teams / General stack — the paired Android shot of the
        // SAME screen (EXP-566 split it out of the old sg_settings-personal).
        let settingsLink = app.buttons["nav-settings-link"]
        expect(settingsLink, within: Wait.nav, "Settings toolbar link missing")
        settingsLink.tap()
        expect(
            app.staticTexts["Teams"],
            within: Wait.nav,
            "Settings screen never appeared"
        )
        expect(
            app.staticTexts["Servers"],
            within: Wait.nav,
            "Settings screen never showed its Servers section"
        )
        // The team row aggregates an avatar + the name, so its own label is not
        // simply the team name — match the button by the staticText it contains.
        let teamRow = app.buttons.containing(.staticText, identifier: Self.teamName).firstMatch
        expect(teamRow, within: Wait.nav, "Team \"\(Self.teamName)\" missing from Settings")
        snapshot("sg_settings-root", settle: 2)

        // ── sg_getting-started: the block behind Settings' entry (EXP-1270) ─
        // The row shows while the demo team has open steps (the seed never
        // sets up a server), and opens the block in a bottom sheet.
        let gettingStartedEntry = anyElement(app, identified: "getting-started-entry")
        expect(
            gettingStartedEntry,
            within: Wait.sync,
            "Settings has no Getting started row — is every step of the demo team done?"
        )
        gettingStartedEntry.tap()
        let gettingStartedSheet = anyElement(app, identified: "getting-started-sheet")
        expect(
            anyElement(app, identified: "getting-started-row-board"),
            within: Wait.nav,
            "The Getting started sheet never drew its rows"
        )
        snapshot("sg_getting-started", settle: 2)
        dismissSheet(app, whileVisible: gettingStartedSheet)
        _ = gettingStartedSheet.waitForNonExistence(timeout: Wait.nav)

        // ── sg_settings-team: team settings ─────────────────────────────────
        teamRow.tap()
        // TeamSettingsView carries the nav title "Settings" too — anchor on the
        // seeded board listed in its Boards section instead.
        expect(
            app.staticTexts[Self.seededBoardName],
            within: Wait.sync,
            "Team settings never listed the seeded boards"
        )
        snapshot("sg_settings-team", settle: 2)

        // ── sg_settings-team-agent: Settings → Agents, one agent's page ─────
        // EXP-1293/1309: the Triage agent (DEMO_AGENT_ID) — its build
        // server, login, daily Schedule row and the server-only instructions
        // (`sessions.getAgent`, so gate on their text). A detour: nothing is
        // edited, Back returns to the team settings.
        if ScreenshotShots.isWanted("sg_settings-team-agent") {
            let agentRow = app.buttons
                .matching(identifier: "agent-settings-row")
                .matching(NSPredicate(format: "label CONTAINS %@", Self.agentName))
                .firstMatch
            expect(agentRow, within: Wait.sync, "Team settings listed no \(Self.agentName) agent")
            scrollUntilVisible(app, agentRow, attempts: 8)
            agentRow.tap()
            expect(anyElement(app, identified: "trigger-row"), within: Wait.sync, "\(Self.agentName)'s page drew no Schedule row")
            // The editor mounts only once `sessions.getAgent` answered
            // (a failure draws a retry instead), so it IS the content gate.
            let instructions = anyElement(app, identified: "agent-instructions")
            expect(instructions, within: Wait.network, "\(Self.agentName)'s instructions never loaded (sessions.getAgent)")
            snapshot("sg_settings-team-agent", settle: 2)
            goBack(app)
            expect(app.staticTexts[Self.seededBoardName], within: Wait.nav, "Did not return to the team settings")
        }

        // ── sg_settings-prompts · sg_settings-prompt: Settings → Prompts ────
        // NEW-7: the team settings page's Prompts section (the band with New
        // prompt over one row per prompt) scrolled into view, then New prompt
        // (the owner pill): the same page an existing prompt opens, in create
        // mode so the capture needs no seeded id. Nothing is saved.
        if ScreenshotShots.isWanted("sg_settings-prompts") || ScreenshotShots.isWanted("sg_settings-prompt") {
            let newPrompt = anyElement(app, identified: "prompts-new")
            scrollUntilVisible(app, newPrompt, attempts: 10)
            expect(newPrompt, within: Wait.sync, "Team settings drew no Prompts band with New prompt")
            snapshot("sg_settings-prompts", settle: 2)
            newPrompt.tap()
            expect(anyElement(app, identified: "prompt-form"), within: Wait.nav, "New prompt did not open its form")
            snapshot("sg_settings-prompt", settle: 2)
            goBack(app)
            expect(newPrompt, within: Wait.nav, "Did not return to the team settings")
        }
        goBack(app)

        // ── sg_settings-account: the account / server detail ──────────────────
        // There is no separate profile screen (EXP-311): the signed-in
        // identity, sign out and delete account live on the server row's
        // detail view. The row is titled by the SERVER, with the email below,
        // so match on the email.
        expect(
            app.staticTexts["Servers"],
            within: Wait.nav,
            "Did not return to the Settings screen"
        )
        let serverRow = app.buttons.containing(.staticText, identifier: ScreenshotSeed.demoEmail).firstMatch
        expect(
            serverRow,
            within: Wait.nav,
            "No server row for \(ScreenshotSeed.demoEmail) in Settings"
        )
        serverRow.tap()
        expect(
            app.buttons["Sign out"],
            within: Wait.nav,
            "Account settings did not open"
        )
        snapshot("sg_settings-account", settle: 2)

        // ── sg_onboarding: the first-run create-or-join wizard ───────────────
        // LAST on purpose: it switches the signed-in identity. EXP-1267: a
        // relaunch signed in as the newcomer (their reused session; the login
        // UI only as the fallback) replaces the old Sign out + login form.
        //
        // The newcomer (`newcomer@exponential.at`) is a member of nothing with
        // a null `onboardingCompletedAt`, so the app opens the wizard. NOTHING
        // is submitted: creating a team or accepting an invite would mutate the
        // seed and burn the invite the desktop/web lanes photograph.
        launchSignedIn(
            app,
            email: ScreenshotSeed.newcomerEmail,
            password: ScreenshotSeed.newcomerPassword
        )
        // The wizard opens on web's choice page: the mark over "Welcome to
        // Exponential" and the two outline buttons (polish round ×4).
        expect(
            app.staticTexts["Welcome to Exponential"],
            within: Wait.sync,
            "The onboarding wizard never appeared for \(ScreenshotSeed.newcomerEmail) — reseed with `bun run seed:screenshots`"
        )
        expect(
            app.buttons["team-setup-create"],
            within: Wait.sync,
            "The team step never rendered its Create a team choice"
        )
        snapshot("sg_onboarding", settle: 2)

        // ── sg_onboarding-invite / sg_onboarding-devices ─────────────────────
        // The wizard's last two steps (EXP-725) need a RESOLVED team, which
        // the newcomer has not got — creating one would mutate the seed. The
        // starter identity owns one and is still un-onboarded, so ONE
        // relaunch signed in as them (EXP-1267: reused session, the login UI
        // as the fallback) with `-uiTestingOnboardingStep invite` parks the
        // wizard on step 3. NOTHING is submitted here either: no invite is
        // minted, no board is created.
        launchSignedIn(
            app,
            email: ScreenshotSeed.starterEmail,
            password: ScreenshotSeed.starterPassword,
            extraArguments: ["-uiTestingOnboardingStep", "invite"]
        )

        let inviteStep = app.otherElements["onboarding-invite-step"]
        expect(
            inviteStep,
            within: Wait.firstSync,
            "The wizard never parked on its invite step — is -uiTestingOnboardingStep wired?"
        )
        // Gate on the real control, not just the container: at the seat cap
        // the creator renders NOTHING (App Store 3.1.1) and the shot would be
        // a blank styleguide page.
        expect(
            app.buttons["invite-generate"],
            within: Wait.sync,
            "The invite step never rendered its Generate button — is the starter team over its seat cap?"
        )
        snapshot("sg_onboarding-invite", settle: 2)

        app.buttons["Skip for now"].firstMatch.tap()
        expect(
            app.otherElements["onboarding-devices-step"],
            within: Wait.nav,
            "The wizard never reached its devices step"
        )
        snapshot("sg_onboarding-devices", settle: 2)

        // ── sg_feedback-states: the six feedback kinds (EXP-1275) ────────────
        // A capture-only screen over a signed-out relaunch: every kind drawn
        // by its real primitive, the error toast raised on appear.
        launchSignedOut(app, extraArguments: ["-uiTestingStyleguide", "feedback-states"])
        expect(
            anyElement(app, identified: "feedback-states-screen"),
            within: Wait.launch,
            "The Feedback states screen never opened — is -uiTestingStyleguide wired?"
        )
        snapshot("sg_feedback-states", settle: 2)

        // ── sg_work-card · sg_script-card · sg_comment-call-card: the run
        // thread's cards (EXP-1240/1279/1241) ─────────────────────────────────
        // Capture-only specimens over the real views, one relaunch each. The
        // names stay literal: views.test.ts reads them off the source.
        openThreadCards(app, "work-card")
        snapshot("sg_work-card", settle: 2)
        openThreadCards(app, "script-card")
        snapshot("sg_script-card", settle: 2)
        openThreadCards(app, "comment-call-card")
        snapshot("sg_comment-call-card", settle: 2)
        // ── sg_trace · sg_code-block: THE Trace + Queue and THE CodeBlock
        // (EXP-1324), capture-only specimens over the real ExpUI views.
        openThreadCards(app, "trace")
        snapshot("sg_trace", settle: 2)
        openThreadCards(app, "code-block")
        snapshot("sg_code-block", settle: 2)
        // ── sg_ask-dock: THE ask dock (fixtures/ask-dock.json): one plan,
        // a stack of three, a focused card with the back arrow.
        openThreadCards(app, "ask-dock")
        snapshot("sg_ask-dock", settle: 2)
        // ── sg_plan-card: THE rich plan / question card (EXP-1280,
        // fixtures/plan-card.json): a pending plan with a picture-tile
        // question, a multiple one and the inline surface; an answered
        // question with several picks; an approved plan with feedback.
        openThreadCards(app, "plan-card")
        snapshot("sg_plan-card", settle: 2)

        finished = true
    }

    // MARK: - Helpers

    /// EXP-1259: relaunch signed out onto one run-thread card specimen.
    @MainActor
    private func openThreadCards(_ app: XCUIApplication, _ card: String) {
        launchSignedOut(app, extraArguments: ["-uiTestingStyleguide", card])
        expect(
            anyElement(app, identified: "thread-cards-screen"),
            within: Wait.launch,
            "The \(card) screen never opened — is -uiTestingStyleguide wired?"
        )
    }

    /// EXP-1309: the Threads tab → the Triage agent's channel → back → the
    /// cold-start thread → its "Profile the launch path" session, each shot
    /// gated on its seeded content; ends back on the Threads list.
    @MainActor
    private func walkThreads(_ app: XCUIApplication) {
        let threadsTab = app.buttons["tab-threads"]
        expect(threadsTab, within: Wait.nav, "Threads tab missing")
        threadsTab.tap()
        // ── sg_threads: gate on the rows' quiet words (pull requests + the
        // pending question have synced), not on the list container.
        // EXP-1339: `merged` draws no word (only the violet lead).
        for word in ["question", "stack"] {
            expect(
                threadRow(app, containing: word),
                within: Wait.sync,
                "No Threads row says \"\(word)\" — reseed with `bun run seed:screenshots`"
            )
        }
        snapshot("sg_threads", settle: 2)

        // ── sg_agent-channel: the Triage agent's channel (DEMO_AGENT_ID) ──
        let agentRow = app.buttons
            .matching(identifier: "agents-row")
            .matching(NSPredicate(format: "label CONTAINS %@", Self.agentName))
            .firstMatch
        expect(agentRow, within: Wait.sync, "The Agents band has no \(Self.agentName) row")
        agentRow.tap()
        expect(
            anyElement(app, containing: Self.agentRequest),
            within: Wait.sync,
            "\(Self.agentName)'s channel never drew the seeded request"
        )
        snapshot("sg_agent-channel", settle: 2)
        goBack(app)
        // A subset run that wants neither of the next two stops here: the
        // session step needs the relay stub (`bun run screenshots:desktop`).
        guard ScreenshotShots.isWanted("sg_thread") || ScreenshotShots.isWanted("sg_session") else { return }

        // ── sg_thread: the cold-start thread (DEMO_THREAD_ID) ──────────────
        let coldStart = threadRow(app, containing: Self.coldStartThreadTitle)
        expect(coldStart, within: Wait.nav, "Did not return to the Threads list")
        coldStart.tap()
        // THE ask dock: the seeded question + the child's plan wait on the
        // viewer, stacked in the composer's slot.
        expect(
            anyElement(app, identified: "ask-dock-row"),
            within: Wait.sync,
            "The cold-start thread drew no ask dock stack"
        )
        // EXP-1334: the strip never scrolls — the "+" and Conversation pinned
        // left, Guide pinned right, the chips folded between them.
        for id in ["thread-strip-add", "thread-tab-conversation", "thread-tab-guide"] {
            let pinned = anyElement(app, identified: id)
            expect(pinned, within: Wait.sync, "The thread strip drew no \(id)")
            XCTAssertTrue(pinned.isHittable, "\(id) is off screen — the strip must never scroll")
        }
        snapshot("sg_thread", settle: 2)
        guard ScreenshotShots.isWanted("sg_session") else {
            goBack(app)
            settle(1)
            return
        }

        // ── sg_session: that session's page in the thread (DEMO_SESSION_ID) ─
        // The relay stub's feed ends on an UNANSWERED question, whose card
        // takes the composer's place (EXP-1306). Nothing is answered.
        // The session's strip chip in the thread header opens the SAME page
        // as its Conversation card, and it never scrolls: a card scrolled
        // under the floating header still reads hittable, and XCUITest then
        // taps the point it finds visible (the header's back button on a Pro
        // Max), popping the thread instead. The card is the fallback when
        // the strip folded the chip into a "+N".
        let sessionChip = anyElement(app, identified: "thread-tab-session-\(Self.sessionId)")
        if sessionChip.waitForExistence(timeout: Wait.sync), sessionChip.isHittable {
            sessionChip.tap()
        } else {
            let sessionCard = app.descendants(matching: .any)
                .matching(identifier: "session-card")
                .matching(NSPredicate(format: "label CONTAINS %@", Self.sessionTitle))
                .firstMatch
            for _ in 0..<4 where !(sessionCard.exists && sessionCard.isHittable) {
                app.swipeDown()
            }
            expect(sessionCard, within: Wait.nav, "The cold-start thread drew no \"\(Self.sessionTitle)\" chip or card")
            sessionCard.tap()
        }
        // EXP-1343: a session chip of THIS thread opens as its page under
        // the thread's ONE header (the thread title, Stop while live) — no
        // pushed session screen, no back row (Android's `sg_session` is the
        // same page). THE ask dock: the relay feed's pending question takes
        // the composer's place once the feed is live and answerable. Gate on
        // its first option row (`agent-question-option-1`, drawn only by the
        // relay feed's card; the Conversation's dock draws plan-card rows):
        // SwiftUI folds the dock's single-child containers into `ask-dock`,
        // so the card's own `agent-feed-question` id is not queryable here,
        // and the neighbouring Conversation page's dock is `ask-dock` too.
        expect(
            app.buttons["agent-question-option-1"].firstMatch,
            within: Wait.firstSync,
            "The session drew no pending question in its ask dock — is `bun run screenshots:desktop` publishing its feed?"
        )
        expect(
            anyElement(app, identified: "thread-tab-session-\(Self.sessionId)"),
            within: Wait.sync,
            "The session did not open as the thread's \"\(Self.sessionTitle)\" page"
        )
        expect(anyElement(app, identified: "session-stop"), within: Wait.sync, "The thread header drew no Stop for the session")
        snapshot("sg_session", settle: 2)
        // Back once (the session page → the Threads list) on the system
        // back button (`goBack`: by name, never the bar's Stop or `…`).
        goBack(app)
        settle(1)
    }

    /// A Threads row whose (combined) label contains `fragment`.
    @MainActor
    private func threadRow(_ app: XCUIApplication, containing fragment: String) -> XCUIElement {
        app.buttons
            .matching(identifier: "threads-row")
            .matching(NSPredicate(format: "label CONTAINS %@", fragment))
            .firstMatch
    }

    /// A thread's strip → its Guide tab.
    @MainActor
    private func openThreadGuide(_ app: XCUIApplication) {
        let guideTab = anyElement(app, identified: "thread-tab-guide")
        expect(guideTab, within: Wait.sync, "The thread never opened on its strip")
        guideTab.tap()
    }

    /// Board switcher → the named board.
    ///
    /// The sheet's rows are Buttons whose glyph + name + prefix SwiftUI merges
    /// into ONE element, so there is no contained staticText to address — match
    /// on the button's own concatenated label instead. The nav-row trigger
    /// overrides its label to "Switch board", so it can never be the match.
    @MainActor
    private func switchBoard(_ app: XCUIApplication, to name: String) {
        let switcherButton = app.buttons["Switch board"]
        expect(switcherButton, within: Wait.nav, "Board switcher trigger missing")
        switcherButton.tap()
        let headline = app.staticTexts["Switch board"]
        expect(headline, within: Wait.nav, "Board switcher sheet did not open")
        let row = app.buttons.matching(
            NSPredicate(format: "label CONTAINS %@", name)
        ).firstMatch
        expect(row, within: Wait.nav, "Board \"\(name)\" missing from the switcher")
        row.tap()
        _ = headline.waitForNonExistence(timeout: Wait.nav)
        settle(1)
    }
}
