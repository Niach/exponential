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
///   sg_sign-in ·
///   sg_board-empty ·
///   sg_board-bulk-edit · sg_issue-comments · sg_issue-reporter-comments · sg_issue-properties ·
///   sg_issue-create · sg_search · sg_my-issues · sg_agents ·
///   sg_chat · sg_composer-menu · sg_chat-issues ·
///   sg_machine-settings · sg_settings-agents · sg_settings-team-agent · sg_settings-assistant · sg_chats ·
///   sg_session-row · sg_pr-row ·
///   sg_settings-root · sg_settings-team ·
///   sg_settings-account · sg_onboarding · sg_onboarding-create-team ·
///   sg_invite-accept · sg_onboarding-invite ·
///   sg_onboarding-devices · sg_agent-home · sg_search-list ·
///   sg_getting-started · sg_feedback-states · sg_work-card ·
///   sg_script-card · sg_comment-call-card · sg_chat-screen · sg_agent-channel ·
///   sg_session · sg_trace · sg_code-block · sg_ask-dock · sg_dock-card · sg_record-form-field-states · sg_plan-card ·
///   sg_chat-list · sg_phone-tab-bar · sg_phone-board-menu · sg_phone-page-header · sg_chat-header ·
///   sg_chat-transcript · sg_sealed-locked · sg_chat-menus · sg_chat-selection · sg_schedule-picker · sg_mention-pill ·
///   sg_app-export · sg_app-import · sg_app-draft · sg_app-versions · sg_app-settings · sg_app-data · sg_voice-call · sg_call-controls · sg_call-pill · sg_device-readiness ·
///   sg_plan-page · sg_team-avatar ·
///   sg_share-targets · sg_share-composer · sg_composer-add-file ·
///   sg_add-server · sg_drafts · sg_composer-slash-menu · sg_session-slash-menu ·
///   sg_settings-assistant-meeting · sg_settings-general · sg_settings-members ·
///   sg_settings-labels · sg_settings-repositories · sg_onboarding-join (EXP-1558) ·
///   sg_app-run (VAPP-242) ·
///   sg_app-record-table · sg_app-record-form · sg_app-calendar · sg_app-timeline · sg_app-connect ·
///   sg_app-publish · sg_app-versions-sweep · sg_app-version-preview ·
///   sg_app-settings-links · sg_app-settings-automations ·
///   sg_app-csv-import · sg_app-record-history · sg_app-record-undo ·
///   sg_team-avatar · sg_device-attention ·
///   sg_os-live-activity · sg_os-live-activity-island · sg_os-notification-actions
///
/// EXP-1295 (Chats v2) repointed the shots whose screens left (Reviews,
/// the work faces, the Recent sheet, the builtin actions, the action page)
/// and retired `sg_chat-action`, `sg_action-runs`, `sg_action-create`,
/// `sg_run-changes` and `sg_trigger-editor`. EXP-1309 then moved every
/// shot off its hand-written specimen onto the LIVE app over the
/// seed web/desktop photograph, so they cannot drift from it again:
///   * `sg_agent-home` = the dawn home the app lands on (`agent-page`)
///     (`sg_session-row` is a specimen since session 20b7fdc2: the island's
///     rows, `walkSpecimens`; it used to photograph the Chats tab);
///   * `sg_chats` = the Chats tab (Agents band, Needs you · Running ·
///     Recent, each row's quiet word off its pull requests);
///     `sg_agent-channel` = the Triage agent's channel off its Agents row;
///     `sg_chat-screen` = the cold-start chat (DEMO_CHAT_ID); `sg_session` =
///     its "Profile the launch path" session card's page in that chat
///     (EXP-1343; DEMO_SESSION_ID, fed by the relay stub), its pending
///     question in the composer's place;
///   * `sg_settings-team-agent` = Settings → team → Agents → Triage;
///   * `sg_pr-row` = the issue page's pull-request row (S-3 retired
///     `sg_guide` + `sg_guide-section` with the Guide tab).
///
/// EXP-1284 added `sg_chat-screen` + `sg_agent-channel` (the CHAT screen).
///
/// EXP-1259 added the last four: the Agent HOME the app lands on (the
/// greeting over the composer, nothing beneath it), the `#` typeahead's
/// search-list sheet, Getting started's block (Settings' entry sheet) and the
/// Feedback states screen (`FeedbackStatesScreen`, opened by
/// `-uiTestingStyleguide feedback-states` on a signed-out relaunch). The run chat's three cards
/// (`sg_work-card`, `sg_script-card`, `sg_comment-call-card`, EXP-1240/1279/
/// 1241) ride the same seam (`ChatCardsScreen`).
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
/// EXP-698 r5 added `sg_onboarding-create-team`: the create-or-join team
/// form. It used to be photographed in the New team sheet; it is now the
/// newcomer's wizard page (Create a team pushed off the choice page), the
/// full-screen step card web draws, beside `sg_onboarding-join`.
/// #EXP-1384 retired `sg_board-switcher` with the sheet it
/// photographed (the board menu is `sg_phone-board-menu`).
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
/// chats holding them, the team agents, actions) PLUS, since EXP-642, the
/// relay stub: `bun run screenshots:desktop` (apps/web) registers the demo
/// user's OWN device row, which is what `sg_machine-settings` (gated `isMine
/// && registered`) and the `sg_chat*` shots photograph, and (EXP-1309)
/// serves the scripted feed `sg_session` watches over eNet (VAPP-168).
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
    /// APP-5 — the showcase issue: the only one the seed gives comment replies
    /// (four comments, incl. an `@mention` + `#APP-2` issue ref).
    private static let showcaseTitle = "Reduce cold start below 800 ms"
    /// EXP-1341: the cold-start chat (DEMO_CHAT_ID) is titled off its
    /// linked issues (APP-5, APP-10, FB-3), session-title.json `titles.issues`:
    /// `#APP-5 Reduce cold start below 800 ms + 2`; matched on the issue title.
    private static let coldStartChatTitle = "Reduce cold start below 800 ms"
    /// First comment on APP-5, plain text before any markdown decoration.
    private static let showcaseCommentFragment = "Profiled on a mid-range device"
    /// #EXP-1754: the widget-filed issue (APP-19) and its deleted member's note.
    private static let widgetTitle = "Can't sign in on the iPad app"
    private static let widgetCommentFragment = "Same spinner on an iPad Air here"
    /// Assigned to the demo user, so it is on My Issues.
    private static let myIssueTitle = "Dark mode contrast pass across settings"
    /// APP-14: the seed's realPr issue (its open PR row).
    private static let guideTitle = "Group board issues by assignee"
    /// APP-15: its chat carries the seeded quick replies.
    private static let batchGuideTitle = "Batch-edit labels from the board"
    private static let searchQuery = "by:alex"
    /// A seeded board — the anchor that says we are on TEAM settings rather
    /// than the outer Settings screen (both carry the nav title "Settings").
    private static let seededBoardName = "Mobile App"
    /// The seed's SECOND board: created empty on purpose, so the "no issues
    /// yet" state is photographable without deleting anything (EXP-642).
    private static let emptyBoardName = "Launch Marketing"
    /// EXP-1558: the bulk-edit selection = the board's first three rows
    /// (APP-6, APP-7, APP-9 at the top of Backlog), the ones web's
    /// `openBoardBulkEdit` checks, so every client photographs one selection.
    private static let bulkTitles = [
        "Push notification deep links open the wrong tab",
        "Add drag-and-drop reordering on the board",
        "Offline queue for issue edits",
    ]
    /// EXP-1551: the two issues every client's `chat-issues` chips, in order
    /// (web `demo-ids.ts` `CHAT_ISSUE_IDENTIFIERS`: `$issueA` APP-3, `$issueB`
    /// APP-6).
    private static let chatIssueTitles = [
        "Dark mode contrast pass across settings",
        "Push notification deep links open the wrong tab",
    ]
    /// The device `bun run screenshots:desktop` registers for the demo user.
    private static let demoDeviceName = "Alex's MacBook Pro"
    /// The login that device announces for its agents (`screenshot-demo.ts`
    /// `DEMO_AGENT_STATUS`) — EXP-909: the first login row UNDER the machine
    /// row names it.
    private static let demoAccountEmail = "demo@exponential.at"
    /// The Triage agent (`DEMO_AGENT_ID`): its channel and settings page.
    private static let agentName = "Triage"
    /// #EXP-1735: the seeded lead Assistant's member (the demo user).
    private static let leadName = "Alex Carter"
    /// `DEMO_AGENT_REQUEST`, the person's ask in Triage's channel.
    private static let agentRequest = "Label the Android ones too"
    /// The cold-start chat's child session (`DEMO_SESSION_ID`).
    private static let sessionTitle = "Profile the launch path"
    /// Its id (`DEMO_SESSION_ID`): the strip chip of its chat page.
    private static let sessionId = "7c1f0a52-6f3c-4e19-9a2b-2d5d0f7c8e41"
    /// VAPP-122: the seeded Working hours app (`DEMO_APP_SLUG`, with
    /// records), whose editor's Preview tab is the `app` shot.
    private static let appSlug = "working-hours"
    private static let appName = "Working hours"
    /// VAPP-176: the seeded Release checklist app (`DEMO_APP_EMPTY_ID`): no
    /// version and no editor session = a DRAFT (Jonas's, so leaving its
    /// editor never discards it).
    private static let draftAppSlug = "release-checklist"
    private static let draftAppName = "Release checklist"
    /// EXP-1558: the demo user's title-less seeded draft's row, which only
    /// the Drafts list draws (its other draft shares APP-9's title).
    private static let draftTitle = "Untitled draft"

    @MainActor
    func testCaptureStyleguideScreenshots() throws {
        continueAfterFailure = false

        let app = makeScreenshotApp()

        if Self.specimensOnly {
            walkSpecimens(app)
            return
        }

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

        // ── sg_apps · sg_app: the team's Apps off the Chats header, then
        // the Team dashboard editor (VAPP-110, DEMO_APP_SLUG) on its Chat tab.
        // Gated: opening the editor and leaving it ENDS the seeded editor
        // session, so only a run that wants these shots visits.
        // VAPP-121: sg_app-export · sg_app-import ride the same walk.
        // VAPP-176: sg_app-draft too (the Release checklist draft's editor).
        // VAPP-177: sg_app-versions · sg_app-settings (the editor's … menu).
        // VAPP-178: sg_app-data (the editor's Data segment).
        // VAPP-242: sg_app-run (a row tap RUNS the app; the editor = … › Edit app).
        if ["sg_apps", "sg_app-run", "sg_app", "sg_app-export", "sg_app-import", "sg_app-draft", "sg_app-versions", "sg_app-settings", "sg_app-data"]
            .map(ScreenshotShots.isWanted).contains(true) || Self.richAppShots.map(ScreenshotShots.isWanted).contains(true) {
            walkApps(app)
        }
        // The rich demo apps (CRM, Leave planner, Ops desk) + Team dashboard's
        // publish states: walkRichApps ends on the Apps list.
        if Self.richAppShots.map(ScreenshotShots.isWanted).contains(true) {
            walkRichApps(app)
        }

        // Switch to the Board tab for the board.
        let issuesTab = app.buttons["tab-board"]
        expect(issuesTab, within: Wait.sync, "Tab bar never appeared")
        issuesTab.tap()

        // The FIRST sync after sign-in: the one long wait of the run.
        let showcaseRowTitle = app.staticTexts[Self.showcaseTitle]
        // The board tab opens the viewer's LAST board (server-side): another
        // lane's walk may have left it on another board; land on the seeded one.
        if !showcaseRowTitle.waitForExistence(timeout: Wait.sync),
           !app.buttons["tab-board"].label.contains(Self.seededBoardName) {
            switchBoard(app, to: Self.seededBoardName)
        }
        expect(
            showcaseRowTitle,
            within: Wait.firstSync,
            "Issue list never synced (missing showcase issue \(Self.showcaseTitle))"
        )
        if !injected { dismissSavePasswordSheet(timeout: 3) }

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
        // each further row then adds it. EXP-1558: the first three rows, at
        // the top of the list as the board opens (web's selection).
        let bulkRows = Self.bulkTitles.map { app.staticTexts[$0].firstMatch }
        for (title, row) in zip(Self.bulkTitles, bulkRows) {
            XCTAssertTrue(row.waitForExistence(timeout: Wait.sync) && row.isHittable, "\(title) is not on screen as the board opens")
        }
        bulkRows[0].press(forDuration: 1.0)
        let bulkBar = anyElement(app, identified: "bulk-selection-bar")
        expect(
            bulkBar,
            within: Wait.nav,
            "Long-press did not enter multi-select"
        )
        for row in bulkRows.dropFirst() {
            row.tap()
        }
        expect(
            bulkBar.staticTexts["\(Self.bulkTitles.count)"],
            within: Wait.nav,
            "The bulk bar does not count \(Self.bulkTitles.count) selected rows"
        )
        snapshot("sg_board-bulk-edit", settle: 2)
        // Leave selection mode — every later shot assumes the plain list.
        app.buttons["Clear selection"].firstMatch.tap()
        _ = bulkBar.waitForNonExistence(timeout: Wait.nav)

        // ── sg_issue-comments: APP-5 scrolled to its comments ──────────
        // Tap the row's TITLE text, never the `issue-row-*` element (EXP-348).
        expect(showcaseRowTitle, within: Wait.nav, "Did not return to the board")
        openIssue(app, title: Self.showcaseTitle)
        let commentsHeader = app.staticTexts["comment-replies-header"]
        expect(commentsHeader, within: Wait.nav, "Issue detail did not open")
        // The comments shape can lag the issues shape by tens of seconds right
        // after the first login — gate on a real comment body, not the header.
        expect(
            anyElement(app, containing: Self.showcaseCommentFragment),
            within: Wait.sync,
            "The comments on APP-5 never synced"
        )
        // ── sg_issue-properties: the combined properties sheet ───────────────
        // EXP-1676: BEFORE the comments scroll: the page at its top behind the
        // sheet, the backdrop web-mobile photographs.
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
        // The detail is one long ScrollView with no scroll-to anchor; walk down
        // until the "Activity" header is on screen, then two more swipes so the
        // chat — not the header — fills the frame.
        scrollUntilVisible(app, commentsHeader, attempts: 14)
        // EXP-1676: web's and Android's frame = the END of the page's scroller.
        for _ in 0..<6 { app.swipeUp() }
        settle(1)
        snapshot("sg_issue-comments", settle: 2)

        goBack(app)

        // ── sg_issue-reporter-comments: APP-19, the widget-filed issue, at
        // its comments (#EXP-1754): the reporter's reply in the hue of their
        // name, a deleted member's note grey. A self-contained detour, skipped
        // whole when a scoped `shots:` run did not ask for it.
        if ScreenshotShots.isWanted("sg_issue-reporter-comments") {
            expect(showcaseRowTitle, within: Wait.nav, "Did not return to the board")
            openIssue(app, title: Self.widgetTitle)
            let widgetComments = app.staticTexts["comment-replies-header"]
            expect(widgetComments, within: Wait.nav, "The widget issue did not open")
            expect(
                anyElement(app, containing: Self.widgetCommentFragment),
                within: Wait.sync,
                "The comments on APP-19 never synced"
            )
            scrollUntilVisible(app, widgetComments, attempts: 14)
            for _ in 0..<6 { app.swipeUp() }
            settle(1)
            snapshot("sg_issue-reporter-comments", settle: 2)
            goBack(app)
        }

        // ── sg_issue-create: the new-issue page ─────────────────────────────
        // The compose button is only mounted on board routes (AppNavigator
        // `composeTarget`), so come back to the issues tab first. The title
        // field takes focus on appear; EXP-1558: the shot lowers the keyboard,
        // the state the other clients photograph. EXP-1654: nothing is typed
        // — every other platform shows the EMPTY form (placeholder title,
        // Create disabled), and an untyped page autosaves no draft.
        showBoard(app)
        expect(showcaseRowTitle, within: Wait.nav, "Board did not come back for the compose shot")
        let composeButton = app.buttons["compose-button"]
        expect(composeButton, within: Wait.nav, "Compose button missing on the board")
        composeButton.tap()
        let titleField = app.textFields["issue-title-field"]
        expect(titleField, within: Wait.nav, "Create-issue page did not open")
        dismissKeyboard(app)
        snapshot("sg_issue-create", settle: 2)
        // EXP-1170: the run must not leave anything in the seed; Back from
        // an empty page writes nothing (and deletes a transient row if one
        // was ever written).
        app.buttons["Back"].firstMatch.tap()
        // A clear that left a character behind (a slow keyboard) keeps
        // content on the page, so Back asks the EXP-1212 leave prompt:
        // Discard it — the run must leave nothing in the seed either way.
        let discard = app.buttons["Discard"].firstMatch
        if discard.waitForExistence(timeout: 2) { discard.tap() }
        _ = titleField.waitForNonExistence(timeout: Wait.nav)

        // ── sg_search: the search view with seeded results ───────────────────
        // EXP-686: Search lost its tab — it is a push off the board header.
        let searchButton = app.buttons["header-search"]
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
        // EXP-1676: the keyboard down, the state the other clients photograph.
        dismissKeyboard(app)
        snapshot("sg_search", settle: 2)
        goBack(app)

        // ── sg_my-issues: the board's "My issues" chip (EXP-1373) ────────────
        showBoard(app)
        let myIssuesChip = app.buttons["board-chip-mine"]
        expect(myIssuesChip, within: Wait.nav, "My issues chip missing")
        myIssuesChip.tap()
        expect(
            app.staticTexts[Self.myIssueTitle].firstMatch,
            within: Wait.sync,
            "My Issues never showed the issues assigned to the demo user"
        )
        snapshot("sg_my-issues", settle: 2)
        // ── sg_drafts: the board's Drafts chip (EXP-1558; web-mobile's
        // `?chip=drafts`): the demo user's seeded drafts. A detour.
        if ScreenshotShots.isWanted("sg_drafts") {
            let draftsChip = app.buttons["board-chip-drafts"]
            expect(draftsChip, within: Wait.nav, "The board has no Drafts chip — reseed with `bun run seed:screenshots` (the demo user's drafts)")
            draftsChip.tap()
            // EXP-1647: a draft row is ONE combined button (title + age).
            expect(
                anyElement(app, containing: Self.draftTitle),
                within: Wait.sync,
                "The Drafts chip never listed the seeded draft \"\(Self.draftTitle)\""
            )
            snapshot("sg_drafts", settle: 2)
        }
        app.buttons["board-chip-all"].tap()

        // ── sg_agents: the machines / command centre ─────────────────────────
        // Since EXP-642 this lane needs the relay stub (`screenshots:desktop`):
        // the demo user's own device row is what the next three shots are
        // taken from, and an empty machines list is not a useful reference
        // shot either. EXP-686 renamed the surface to Devices (the shot id
        // stays sg_agents).
        // #EXP-1384: Devices is the Team page's last settings row.
        openDevices(app)
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
        // EXP-1560 (`devices-list.json`): a device is ONE card in a grid,
        // its logins as brand-mark icons (the email rides the icon's
        // accessibility label), so the demo login is in frame straight away.
        expect(
            anyElement(app, containing: Self.demoAccountEmail),
            within: Wait.network,
            "No \(Self.demoAccountEmail) login icon — is the stub device reporting agent accounts?"
        )
        // EXP-1560 (the "Device card" entry): a login with an attention item
        // wears the amber run-mark badge; soft — the demo logins may all be
        // healthy. The outdated demo desktop's update line lives in its `⋯`.
        _ = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'device-login-badge-'"))
            .firstMatch.waitForExistence(timeout: Wait.sync)
        // FEED-149: the demo user OWNS Acme, so the teammate's shared build
        // server's card carries a `⋯` (the owner's Remove from this team) —
        // beside the demo user's own card's `⋯`.
        let deviceActions = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'device-actions-'")
        )
        expect(deviceActions.element(boundBy: 1), within: Wait.sync,
               "No ⋯ on the shared team device — is the demo user Acme's owner?")
        snapshot("sg_agents", settle: 2)

        // ── sg_add-server: the band's Add device sheet (EXP-1169; web's
        // `openAddServer`): the desktop download and the CLI one-liner. A
        // detour: the sheet is swiped away again.
        if ScreenshotShots.isWanted("sg_add-server") {
            let addDevice = app.buttons["Add device"].firstMatch
            expect(addDevice, within: Wait.nav, "The Devices band has no Add device pill")
            addDevice.tap()
            let addSheet = anyElement(app, identified: "add-device-sheet")
            expect(addSheet, within: Wait.nav, "Add device opened no sheet")
            expect(anyElement(app, identified: "install-snippet"), within: Wait.sync, "The Add device sheet drew no install one-liner")
            snapshot("sg_add-server", settle: 2)
            dismissSheet(app, whileVisible: app.staticTexts["Add device"].firstMatch)
            _ = addSheet.waitForNonExistence(timeout: Wait.nav)
            settle(1)
        }

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
        // S-3: the agent row stands over the card, the viewer's Assistant
        // preselected (composer-home.json `agentRow`): that is `sg_chat`.
        let assistantLabel = anyElement(app, identified: "composer-agent-label")
        expect(assistantLabel, within: Wait.sync, "The composer's agent row never appeared")
        expect(
            app.staticTexts["To Assistant"].firstMatch,
            within: Wait.sync,
            "The agent row never preselected the Assistant — does the demo user have a desktop?"
        )
        snapshot("sg_chat", settle: 2)
        // ── sg_composer-slash-menu: `/` in the home composer (NEW-7; web's
        // `typeSlashInComposer`): the team's seeded prompts under the card.
        // EXP-1654: HERE, on the clean composer (the Assistant preselected,
        // no subject) the other platforms photograph — after sg_chat-issues
        // it carried the Implement chips and the Temporary pick. EXP-1558:
        // the field is a block editor whose empty first line sits at the
        // top, so tap the first line until the keyboard is up. EXP-1676: the
        // menu follows the draft (web, Android), so the keyboard is put away
        // like web's frame and the menu stays. Nothing is picked.
        if ScreenshotShots.isWanted("sg_composer-slash-menu") {
            let slashField = anyElement(app, identified: "agent-composer-field")
            expect(slashField, within: Wait.nav, "The home composer has no field")
            for dy in [0.25, 0.5, 0.15, 0.25, 0.5] where !app.keyboards.firstMatch.exists {
                slashField.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: dy)).tap()
                _ = app.keyboards.firstMatch.waitForExistence(timeout: Wait.nav)
            }
            expect(app.keyboards.firstMatch, within: Wait.nav, "The home composer never took focus for the / shot")
            app.typeText("/")
            expect(
                anyElement(app, identified: "agent-composer-slash-menu"),
                within: Wait.sync,
                "Typing / opened no prompts menu — does the seed insert the team's prompts?"
            )
            dismissKeyboard(app)
            expect(
                anyElement(app, identified: "agent-composer-slash-menu"),
                within: Wait.nav,
                "The / menu closed with the keyboard — it follows the draft"
            )
            snapshot("sg_composer-slash-menu", settle: 2)
            clearSlashDraft(app, field: slashField)
        }
        // The dotted circle = Temporary session: today's launch, whose "+"
        // carries the runner and work rows.
        anyElement(app, identified: "composer-agent-temporary").tap()
        // The submit label proves the composer resolved its subject (a chat).
        expect(
            app.buttons["Start chat"].firstMatch,
            within: Wait.sync,
            "The composer never settled on the chat subject"
        )

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
        // EXP-1551: the same two issues web chips (APP-3 then APP-6), each
        // filtered first so its row is on screen whatever the list's order.
        let issueSearch = anyElement(app, identified: "agent-composer-submenu-search")
        expect(issueSearch, within: Wait.nav, "The issue picker has no search input")
        for title in Self.chatIssueTitles {
            // Tap the trailing edge: the caret lands at the END, so the
            // deletes clear the previous title whole (a centre tap leaves
            // its head in the field).
            issueSearch.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: 0.5)).tap()
            clearText(of: issueSearch)
            issueSearch.typeText(title)
            // EXP-1030: a picker row reads `IDENT Title` in ONE label (the ×4
            // contract), so it is matched on the title as a fragment, among
            // the list's rows (never the input holding the same text).
            let row = app.descendants(matching: .any)
                .matching(identifier: "search-list-row")
                .matching(NSPredicate(format: "label CONTAINS %@", title))
                .firstMatch
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
        // EXP-1558: the field is a block editor in a scroll view; its empty
        // first line sits at the top, so a centre tap can miss it. Tap the
        // first line and wait for the keyboard before typing.
        for dy in [0.25, 0.5, 0.15] where !app.keyboards.firstMatch.exists {
            composerField.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: dy)).tap()
            _ = app.keyboards.firstMatch.waitForExistence(timeout: Wait.nav)
        }
        app.typeText("Explain #")
        let refSheet = anyElement(app, identified: "issue-ref-search-sheet")
        expect(refSheet, within: Wait.nav, "Typing # did not open the search-list sheet")
        // EXP-1558: `APP`, the seeded board's key, as web types it
        // (view-recipes `typeIssueRef`): every issue matches, the list's
        // uncapped face.
        app.typeText("APP")
        expect(
            app.descendants(matching: .any).matching(identifier: "search-list-row").element(boundBy: 2),
            within: Wait.sync,
            "The # sheet never listed the seeded issues for \"APP\""
        )
        // EXP-1676: the keyboard down, the state the other clients photograph.
        dismissKeyboard(app)
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
        // back to Devices through the Team tab (its pushes are parked).
        app.buttons["tab-team"].tap()
        expect(
            app.navigationBars["Devices"],
            within: Wait.nav,
            "Did not return to the Devices surface"
        )
        settle(1)

        // ── sg_machine-settings: the device settings sheet ───────────────────
        // EXP-1560: Device settings is the own card's `⋯` item, and only on
        // own registered machines — which is why the relay stub is a
        // prerequisite. The first card is the demo user's own device.
        let ownActions = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'device-actions-'")
        ).firstMatch
        expect(
            ownActions,
            within: Wait.sync,
            "No ⋯ on the device card — the stub device must be the demo user's OWN, registered machine"
        )
        ownActions.tap()
        let machineSettings = app.buttons["machine-settings"].firstMatch
        expect(machineSettings, within: Wait.nav, "No Device settings in the card's ⋯ menu")
        machineSettings.tap()
        let deviceSheet = anyElement(app, identified: "device-settings-sheet")
        expect(deviceSheet, within: Wait.nav, "Device settings sheet did not open")
        snapshot("sg_machine-settings", settle: 2)
        // EXP-694: the sheet autosaves and has no Done button — swipe it away.
        dismissSheet(app, whileVisible: deviceSheet)
        _ = deviceSheet.waitForNonExistence(timeout: Wait.nav)
        settle(1)

        // EXP-1553: sg_pr-row is a specimen now (THE PrRow: ring lead, tree
        // guides, the stack rail; `walkSpecimens`). Back on the board for the
        // issue walk below.
        showBoard(app)
        settle(1)

        // ── sg_chat-replies: the quick replies (EXP-1211) ──────────────────
        // APP-15's chat again, left on its Conversation: the seeded
        // `replies` card's row over the composer capsule.
        if ScreenshotShots.isWanted("sg_chat-replies") || ScreenshotShots.isWanted("sg_session-slash-menu") {
            openIssue(app, title: Self.batchGuideTitle)
            let chatChip = anyElement(app, identified: "issue-chat-chip")
            _ = scrollUntilVisible(app, chatChip)
            expect(chatChip, within: Wait.sync, "APP-15 sits in no chat — reseed with `bun run seed:screenshots`")
            chatChip.tap()
            let replies = anyElement(app, identified: "chat-replies")
            expect(replies, within: Wait.sync, "APP-15's chat drew no quick replies — reseed with `bun run seed:screenshots`")
            snapshot("sg_chat-replies", settle: 2)
            // ── sg_session-slash-menu: `/` in the same chat's composer
            // (NEW-7; web's `typeSlashInSessionComposer` on APP-15's chat):
            // the builtins, then the team's prompts. Nothing is sent.
            if ScreenshotShots.isWanted("sg_session-slash-menu") {
                let capsule = anyElement(app, identified: "chat-composer-collapsed")
                expect(capsule, within: Wait.nav, "APP-15's chat has no composer capsule")
                capsule.tap()
                _ = app.keyboards.firstMatch.waitForExistence(timeout: Wait.nav)
                app.typeText("/")
                expect(
                    app.descendants(matching: .any)
                        .matching(NSPredicate(format: "identifier BEGINSWITH 'slash-prompt-'"))
                        .firstMatch,
                    within: Wait.sync,
                    "Typing / in the chat composer listed no prompts — does the seed insert the team's prompts?"
                )
                // EXP-1676: web's frame has no keyboard; the menu follows the draft.
                dismissKeyboard(app)
                expect(
                    app.descendants(matching: .any)
                        .matching(NSPredicate(format: "identifier BEGINSWITH 'slash-prompt-'"))
                        .firstMatch,
                    within: Wait.nav,
                    "The / menu closed with the keyboard — it follows the draft"
                )
                snapshot("sg_session-slash-menu", settle: 2)
                clearSlashDraft(app, field: anyElement(app, identified: "chat-composer-field"))
            }
            // Chat → issue → board.
            goBack(app)
            goBack(app)
            settle(1)
        }

        // ── sg_chats · sg_agent-channel · sg_chat-screen · sg_session ──────────
        // EXP-1309: the Chats tab and the screens it opens, LIVE over the
        // seed web/desktop photograph (no specimen to drift). Taken this
        // late on purpose: every shape (issues for the chips, pull requests
        // for the row words) has synced by now, and an agent text parsed
        // before its issues landed would cache without its pills. A detour:
        // it leaves through the Issues tab it came from.
        let chatShots = ["sg_chats", "sg_agent-channel", "sg_chat-screen", "sg_session", "sg_script-card", "sg_work-card"]
        if chatShots.map(ScreenshotShots.isWanted).contains(true) {
            walkChats(app)
            showBoard(app)
            settle(1)
        }

        // ── sg_chat-results: APP-14's chat on its Results tab ──────────
        // results-tab.json, LIVE: from Chats, APP-14's chat (the stack top
        // over APP-15, the seeded PR with a real GitHub diff), then the
        // strip's last tab (icon-only, pinned right). Gate on the Changes row
        // (the PR's files loaded) and a result.
        if ScreenshotShots.isWanted("sg_chat-results") {
            walkChatResults(app)
            showBoard(app)
            settle(1)
        }

        // ── sg_settings-root: the TEAM tab (EXP-1373: the settings root moved
        // here) — the active team's header, the teams group, the team's
        // settings rows, the account rows.
        let teamTab = app.buttons["tab-team"]
        expect(teamTab, within: Wait.nav, "Team tab missing")
        teamTab.tap()
        // #EXP-1384: the Team tab parked the Devices page pushed earlier; a
        // re-tap pops back to its root.
        if !anyElement(app, identified: "team-tab-header").waitForExistence(timeout: 2) {
            teamTab.tap()
        }
        expect(
            anyElement(app, identified: "team-tab-header"),
            within: Wait.nav,
            "The Team tab never drew its team header"
        )
        // The teams group has no band (web-mobile): gate on its New team row.
        expect(
            anyElement(app, identified: "board-switcher-new-team"),
            within: Wait.nav,
            "The Team tab never showed its teams group"
        )
        let teamRow = teamSettingsRow(app, "Boards")
        expect(teamRow, within: Wait.nav, "The Team tab has no Boards row")
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
        // #EXP-1381: every Team row opens its OWN page; the Boards page,
        // anchored on the seeded board it lists.
        teamRow.tap()
        expect(
            app.staticTexts[Self.seededBoardName],
            within: Wait.sync,
            "Team settings never listed the seeded boards"
        )
        snapshot("sg_settings-team", settle: 2)
        goBack(app)

        // ── sg_settings-general · -members · -labels · -repositories: the
        // phone's own pages of web's General / Members / Labels /
        // Repositories (settings-nav.json `phone`), each a detour off the
        // Team tab gated on its seeded content (EXP-1558).
        // EXP-1676: from the page's top: the walk left it scrolled, General sat
        // under the glass header (it "exists", so scrollUntilVisible never
        // moved, and the tap landed on the header).
        if ScreenshotShots.isWanted("sg_settings-general") {
            for _ in 0..<2 { app.swipeDown() }
            settle(1)
        }
        for (label, shot, gate) in [
            ("General", "sg_settings-general", "team-settings-identity"),
            ("Members", "sg_settings-members", ScreenshotSeed.demoEmail),
            ("Labels", "sg_settings-labels", "Bug"),
            ("Repositories", "sg_settings-repositories", "acme/mobile-app"),
        ] where ScreenshotShots.isWanted(shot) {
            let row = teamSettingsRow(app, label)
            scrollUntilVisible(app, row, attempts: 4)
            expect(row, within: Wait.nav, "The Team tab has no \(label) row")
            row.tap()
            expect(anyElement(app, identified: "team-settings-page"), within: Wait.nav, "\(label) opened no settings page")
            let content = label == "General" ? anyElement(app, identified: gate) : anyElement(app, containing: gate)
            expect(content, within: Wait.network, "The \(label) page never showed \"\(gate)\"")
            switch shot {
            case "sg_settings-general": snapshot("sg_settings-general", settle: 2)
            case "sg_settings-members": snapshot("sg_settings-members", settle: 2)
            case "sg_settings-labels": snapshot("sg_settings-labels", settle: 2)
            default: snapshot("sg_settings-repositories", settle: 2)
            }
            goBack(app)
            expect(teamSettingsRow(app, "Boards"), within: Wait.nav, "Did not return to the Team tab")
        }

        // ── sg_settings-team-agent: Settings → Agents, one agent's page ─────
        // EXP-1293/1309: the Triage agent (DEMO_AGENT_ID) — its build
        // server, login, daily Schedule row and the server-only instructions
        // (`sessions.getAgent`, so gate on their text). A detour: nothing is
        // edited, Back returns to the team settings.
        // #EXP-1735: on the way, sg_settings-agents = the Agents page's top:
        // the Lead Assistant row naming the seeded lead (the demo user, read
        // through `agents.lead`) over the agents list.
        let wantsAgentsPage = ScreenshotShots.isWanted("sg_settings-agents")
        if ScreenshotShots.isWanted("sg_settings-team-agent") || wantsAgentsPage {
            let agentsRow = teamSettingsRow(app, "Agents")
            scrollUntilVisible(app, agentsRow, attempts: 4)
            agentsRow.tap()
            if wantsAgentsPage {
                let lead = anyElement(app, identified: "settings-agents-lead")
                expect(lead, within: Wait.sync, "The Agents page drew no Lead Assistant row")
                let named = app.descendants(matching: .any).matching(
                    NSPredicate(format: "identifier == %@ AND label CONTAINS %@", "settings-agents-lead", Self.leadName)
                ).firstMatch
                let nested = lead.descendants(matching: .any).matching(
                    NSPredicate(format: "label CONTAINS %@", Self.leadName)
                ).firstMatch
                let deadline = Date().addingTimeInterval(Wait.network)
                while !named.exists && !nested.exists && Date() < deadline { settle(1) }
                XCTAssertTrue(named.exists || nested.exists, "The Lead Assistant row never named \(Self.leadName) (agents.lead)")
                snapshot("sg_settings-agents", settle: 2)
            }
        }
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
            expect(agentRow, within: Wait.nav, "Did not return to the Agents page")
            goBack(app)
        } else if wantsAgentsPage {
            goBack(app)
        }

        // ── sg_settings-assistant: Settings → Agents → Assistant (S-3) ─────
        // The viewer's OWN Assistant (the `@assistant` row, brand mark; the
        // Agents page mounting ensures it): identity, Runs on, Check in, the
        // Voice block and the server-only instructions (gate on them). A
        // detour: nothing is edited.
        if ScreenshotShots.isWanted("sg_settings-assistant") || ScreenshotShots.isWanted("sg_settings-assistant-meeting") {
            let agentsRow = teamSettingsRow(app, "Agents")
            scrollUntilVisible(app, agentsRow, attempts: 4)
            agentsRow.tap()
            let assistantRow = app.buttons["assistant-settings-row"]
            expect(assistantRow, within: Wait.sync, "The Agents page has no Assistant row")
            assistantRow.tap()
            expect(
                anyElement(app, identified: "assistant-check-in-row"),
                within: Wait.network,
                "The Assistant page drew no Check in row (no Assistant synced: does the demo member own a device?)"
            )
            snapshot("sg_settings-assistant", settle: 2)
            // ── sg_settings-assistant-meeting: the same page scrolled to its
            // Meeting mode rows (#NEW-57; web's `scrollToMeetingMode`).
            if ScreenshotShots.isWanted("sg_settings-assistant-meeting") {
                // The page is a List: the row joins the tree only once
                // scrolled near, so scroll first, then gate.
                let autoStop = anyElement(app, identified: "settings-voice-auto-stop")
                for _ in 0..<8 where !(autoStop.exists && autoStop.isHittable) {
                    app.swipeUp()
                    _ = autoStop.waitForExistence(timeout: 1)
                }
                expect(autoStop, within: Wait.network, "The Assistant page drew no Meeting mode Stop after row (voice settings)")
                XCTAssertTrue(autoStop.isHittable, "Stop after never scrolled into view")
                snapshot("sg_settings-assistant-meeting", settle: 2)
            }
            goBack(app)
            expect(assistantRow, within: Wait.nav, "Did not return to the Agents page")
            goBack(app)
        }

        // ── sg_settings-prompts · sg_settings-prompt: Settings → Prompts ────
        // NEW-7: the team settings page's Prompts section (the band with New
        // prompt over one row per prompt) scrolled into view, then New prompt
        // (the owner pill): the same page an existing prompt opens, in create
        // mode so the capture needs no seeded id. Nothing is saved.
        if ScreenshotShots.isWanted("sg_settings-prompts") || ScreenshotShots.isWanted("sg_settings-prompt") {
            let promptsRow = teamSettingsRow(app, "Prompts")
            scrollUntilVisible(app, promptsRow, attempts: 4)
            promptsRow.tap()
            let newPrompt = anyElement(app, identified: "prompts-new")
            scrollUntilVisible(app, newPrompt, attempts: 10)
            expect(newPrompt, within: Wait.sync, "Team settings drew no Prompts band with New prompt")
            snapshot("sg_settings-prompts", settle: 2)
            newPrompt.tap()
            expect(anyElement(app, identified: "prompt-form"), within: Wait.nav, "New prompt did not open its form")
            snapshot("sg_settings-prompt", settle: 2)
            goBack(app)
            expect(newPrompt, within: Wait.nav, "Did not return to the Prompts page")
            goBack(app)
        }

        // ── sg_settings-skills · sg_settings-skill: Settings → Skills ───────
        // #EXP-1661: the read-only list (skills.json `list`), then the first
        // row's read-only page when the seed has a skill. Nothing is saved.
        if ScreenshotShots.isWanted("sg_settings-skills") || ScreenshotShots.isWanted("sg_settings-skill") {
            let skillsRow = teamSettingsRow(app, "Skills")
            scrollUntilVisible(app, skillsRow, attempts: 4)
            skillsRow.tap()
            expect(anyElement(app, identified: "settings-skills-page"), within: Wait.sync, "Settings › Skills did not open")
            snapshot("sg_settings-skills", settle: 2)
            let firstSkill = anyElement(app, identified: "settings-skills-row")
            if ScreenshotShots.isWanted("sg_settings-skill"), firstSkill.waitForExistence(timeout: 2) {
                firstSkill.tap()
                expect(anyElement(app, identified: "skill-form"), within: Wait.nav, "A skill row did not open its page")
                snapshot("sg_settings-skill", settle: 2)
                goBack(app)
            }
            goBack(app)
        }

        // ── sg_settings-account: the account / server detail ──────────────────
        // There is no separate profile screen (EXP-311): the signed-in
        // identity, sign out and delete account live on the server row's
        // detail view. The row is titled by the SERVER, with the email below,
        // so match on the email.
        expect(
            app.staticTexts["Account"],
            within: Wait.nav,
            "Did not return to the Team tab"
        )
        let serverRow = app.buttons.containing(.staticText, identifier: ScreenshotSeed.demoEmail).firstMatch
        scrollUntilVisible(app, serverRow, attempts: 4)
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

        // ── sg_onboarding-create-team: the choice page's Create a team (web's
        // wizard CreateTeamStep), the newcomer's full-screen step card, never
        // the New team sheet. Nothing is typed or submitted; Back returns to
        // the choice page for the join shot.
        if ScreenshotShots.isWanted("sg_onboarding-create-team") {
            app.buttons["team-setup-create"].tap()
            expect(app.staticTexts["Team name"].firstMatch, within: Wait.nav, "Create a team never pushed its form")
            dismissKeyboard(app)
            snapshot("sg_onboarding-create-team", settle: 2)
            app.buttons["Back"].firstMatch.tap()
            expect(app.buttons["team-setup-join"], within: Wait.nav, "Back never returned to the choice page")
        }

        // ── sg_onboarding-join: the choice page's Join a team (web's
        // `openOnboardingJoin`): the invite-link form. Nothing is pasted or
        // submitted; the relaunch below leaves it.
        if ScreenshotShots.isWanted("sg_onboarding-join") {
            app.buttons["team-setup-join"].tap()
            expect(app.staticTexts["Invite link"].firstMatch, within: Wait.nav, "Join a team never pushed its invite-link form")
            dismissKeyboard(app)
            snapshot("sg_onboarding-join", settle: 2)
        }

        // ── sg_invite-accept (#EXP-1632): the newcomer opening the seeded
        // invite link (`screenshots-demo-invite`, the token web and desktop
        // photograph) mid-wizard: the landing card, read and shown, NEVER
        // accepted (Accept would burn the token). The capture hook
        // `-uiTestingOpenInvite` routes the token through the deep-link bus
        // exactly as a tapped `exponential://invite/<token>` would.
        if ScreenshotShots.isWanted("sg_invite-accept") {
            launchSignedIn(
                app,
                email: ScreenshotSeed.newcomerEmail,
                password: ScreenshotSeed.newcomerPassword,
                extraArguments: ["-uiTestingOpenInvite", "screenshots-demo-invite"]
            )
            expect(
                app.buttons["invite-accept"],
                within: Wait.sync,
                "The invite landing card never rendered its Accept invite button — reseed with `bun run seed:screenshots`"
            )
            snapshot("sg_invite-accept", settle: 2)
        }

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
        // by its real primitive in web's six numbered cards (EXP-1554).
        launchSignedOut(app, extraArguments: ["-uiTestingStyleguide", "feedback-states"])
        expect(
            anyElement(app, identified: "feedback-states-screen"),
            within: Wait.launch,
            "The Feedback states screen never opened — is -uiTestingStyleguide wired?"
        )
        snapshot("sg_feedback-states", settle: 2)

        // ── sg_work-card · sg_script-card · sg_comment-call-card: the run
        // chat's cards (EXP-1240/1279/1241) ─────────────────────────────────
        // Capture-only specimens over the real views, one relaunch each. The
        // names stay literal: views.test.ts reads them off the source.
        // EXP-1654: sg_work-card + sg_script-card are the live session
        // walk's (walkChats, in the transcript like web and desktop).
        openChatCards(app, "comment-call-card")
        snapshot("sg_comment-call-card", settle: 2)
        // ── sg_trace · sg_code-block: THE Trace + Queue and THE CodeBlock
        // (EXP-1324), capture-only specimens over the real ExpUI views.
        openChatCards(app, "trace")
        snapshot("sg_trace", settle: 2)
        openChatCards(app, "code-block")
        snapshot("sg_code-block", settle: 2)
        // ── sg_ask-dock: THE Dock (fixtures/dock.json + ask-dock.json): one
        // plan card, the pile, the pile fanned, the plan's sheet.
        openChatCards(app, "ask-dock")
        snapshot("sg_ask-dock", settle: 2)
        // ── sg_dock-card: THE DockCard primitive (fixtures/dock.json): one
        // card, a banner with its action, the pile, the fan, the sheet frame.
        openChatCards(app, "dock-card")
        snapshot("sg_dock-card", settle: 2)
        // ── sg_plan-card: THE rich plan / question card (EXP-1280,
        // fixtures/plan-card.json): a pending plan with a picture-tile
        // question, a multiple one and the inline surface; an answered
        // question with several picks; an approved plan with feedback.
        openChatCards(app, "plan-card")
        snapshot("sg_plan-card", settle: 2)
        // ── sg_chat-list · sg_phone-tab-bar · sg_phone-board-menu ·
        // sg_phone-page-header · sg_chat-header · sg_chat-transcript ·
        // sg_chat-menus · sg_chat-selection (#EXP-1388) · sg_schedule-picker · sg_mention-pill (session 58cb32df / e4a04d22, chats.json +
        // schedule-picker.json) · sg_voice-call (NEW-69, voice-call.json) ·
        // sg_call-controls (S-3, the round call buttons) · sg_call-pill (EXP-1681) ·
        // sg_device-readiness (EXP-1375) · sg_pr-checks (#EXP-1611) ·
        // sg_needs-you · sg_results-tab (#FEED-191): the
        // styleguide entries' specimens.
        walkSpecimens(app)
        // ── sg_app-consent (VAPP-120, app-actions.json): the consent card an
        // app raises for an ungranted typed action, with the operation's own
        // words, drawn by the real card over a stand-in surface (the painted
        // surface's rows are not reachable from XCUITest, so no live walk).
        if ScreenshotShots.isWanted("sg_app-consent") {
            openChatCards(app, "app-consent")
            expect(anyElement(app, containing: "Allow this surface to comment on issue APP-15?"), within: Wait.nav, "The app-consent specimen raised no consent card")
            snapshot("sg_app-consent", settle: 2)
        }

        finished = true
    }

    // MARK: - Helpers

    /// #EXP-1381: a Team page row (`team-settings-row`, settings-nav.json)
    /// by its label.
    @MainActor
    private func teamSettingsRow(_ app: XCUIApplication, _ label: String) -> XCUIElement {
        app.buttons
            .matching(identifier: "team-settings-row")
            .matching(NSPredicate(format: "label BEGINSWITH %@", label))
            .firstMatch
    }

    /// #EXP-1384: the Team tab → its Devices row (the page it pushes).
    @MainActor
    private func openDevices(_ app: XCUIApplication) {
        let teamTab = app.buttons["tab-team"]
        expect(teamTab, within: Wait.nav, "Team tab missing")
        teamTab.tap()
        let devicesRow = teamSettingsRow(app, "Devices")
        scrollUntilVisible(app, devicesRow, attempts: 4)
        expect(devicesRow, within: Wait.nav, "The Team page has no Devices row")
        // The last row can sit under the floating tab bar, where a tap lands
        // on the bar (the board tab) instead: lift it clear first.
        app.swipeUp()
        devicesRow.tap()
    }

    /// The styleguide entries photographed as capture-only specimens
    /// (`-uiTestingStyleguide <entry>`, signed out): no account, no seed.
    static let specimenEntries = [
        "chat-list", "phone-tab-bar", "phone-board-menu", "phone-page-header", "chat-header", "chat-transcript",
        "chat-menus", "chat-selection", "schedule-picker", "mention-pill", "voice-call",
        "call-controls", "call-pill", "device-readiness",
        "sealed-locked", "plan-page", "pr-row", "pr-checks", "session-row",
        "team-avatar", "device-attention", "share-targets", "share-composer", "composer-add-file",
        "needs-you", "results-tab", "record-form-field-states",
        "os-live-activity", "os-live-activity-island", "os-notification-actions",
    ]

    /// EXP-1635: the entries whose capture-only screen has another name.
    static let specimenScreens = ["share-targets": "share-to", "composer-add-file": "composer-attach-menu"]

    /// A scoped run that wants only specimens (`--views sealed-locked`)
    /// needs none of the seeded walk before them.
    static var specimensOnly: Bool {
        guard let requested = ScreenshotShots.requested, !requested.isEmpty else { return false }
        let specimens = Set(specimenEntries.map { "sg_\($0)" })
        return requested.isSubset(of: specimens)
    }

    @MainActor
    private func walkSpecimens(_ app: XCUIApplication) {
        for entry in Self.specimenEntries {
            guard ScreenshotShots.isWanted("sg_\(entry)") else { continue }
            // A simulator has no camera: the add-file menu forces its row.
            openChatCards(
                app,
                Self.specimenScreens[entry] ?? entry,
                extraArguments: entry == "composer-add-file" ? ["-uiTestingCameraRow"] : []
            )
            switch entry {
            case "chat-list": snapshot("sg_chat-list", settle: 2)
            case "phone-tab-bar": snapshot("sg_phone-tab-bar", settle: 2)
            case "phone-board-menu": snapshot("sg_phone-board-menu", settle: 2)
            case "phone-page-header": snapshot("sg_phone-page-header", settle: 2)
            case "chat-header": snapshot("sg_chat-header", settle: 2)
            case "chat-transcript": snapshot("sg_chat-transcript", settle: 2)
            case "chat-menus": snapshot("sg_chat-menus", settle: 2)
            case "chat-selection": snapshot("sg_chat-selection", settle: 2)
            case "mention-pill": snapshot("sg_mention-pill", settle: 2)
            case "voice-call": snapshot("sg_voice-call", settle: 2)
            case "call-controls": snapshot("sg_call-controls", settle: 2)
            case "call-pill": snapshot("sg_call-pill", settle: 2)
            case "device-readiness": snapshot("sg_device-readiness", settle: 2)
            case "sealed-locked": snapshot("sg_sealed-locked", settle: 2)
            case "record-form-field-states": snapshot("sg_record-form-field-states", settle: 2)
            case "plan-page": snapshot("sg_plan-page", settle: 2)
            // #FEED-191: THE entry row + THE list, THE Results tab body.
            case "needs-you": snapshot("sg_needs-you", settle: 2)
            case "results-tab": snapshot("sg_results-tab", settle: 2)
            case "pr-row": snapshot("sg_pr-row", settle: 2)
            // Session 20b7fdc2: THE session row (web's `session-row` island).
            case "session-row": snapshot("sg_session-row", settle: 2)
            // #EXP-1611: the four Merge pills + the open Checks submenu.
            case "pr-checks": snapshot("sg_pr-checks", settle: 2)
            // #EXP-1645 / #EXP-1643: the specimens of states no seed draws.
            case "team-avatar": snapshot("sg_team-avatar", settle: 2)
            case "device-attention": snapshot("sg_device-attention", settle: 2)
            // EXP-1635: the share flow's two steps + the + menu's sub-choice.
            case "share-targets": snapshot("sg_share-targets", settle: 2)
            case "share-composer": snapshot("sg_share-composer", settle: 2)
            case "composer-add-file": snapshot("sg_composer-add-file", settle: 2)
            // #EXP-1639 / #EXP-1641: the OS-drawn surfaces (the Lock Screen
            // card, the Dynamic Island, the expanded notification's actions).
            case "os-live-activity": snapshot("sg_os-live-activity", settle: 2)
            case "os-live-activity-island": snapshot("sg_os-live-activity-island", settle: 2)
            case "os-notification-actions": snapshot("sg_os-notification-actions", settle: 2)
            default: snapshot("sg_schedule-picker", settle: 2)
            }
        }
    }

    /// EXP-1259: relaunch signed out onto one chat cards specimen.
    @MainActor
    private func openChatCards(_ app: XCUIApplication, _ card: String, extraArguments: [String] = []) {
        launchSignedOut(app, extraArguments: ["-uiTestingStyleguide", card] + extraArguments)
        expect(
            anyElement(app, identified: "chat-cards-screen"),
            within: Wait.launch,
            "The \(card) screen never opened — is -uiTestingStyleguide wired?"
        )
    }

    /// The rich demo apps' shots (`demo-sample-apps.ts`, walkRichApps).
    private static let richAppShots = [
        "sg_app-record-table", "sg_app-record-form", "sg_app-calendar", "sg_app-timeline", "sg_app-connect",
        "sg_app-settings-links", "sg_app-settings-automations",
        "sg_app-publish", "sg_app-versions-sweep", "sg_app-version-preview",
        "sg_app-csv-import", "sg_app-record-history", "sg_app-record-undo",
    ]

    /// From the Apps list, back to it: CRM's RecordTable + RecordForm, Leave
    /// planner's Calendar + Timeline, Ops desk's Connect banner + Settings,
    /// Team dashboard's Publish + sweep + version preview (Publish is never
    /// pressed: it would supersede the seeded sweep), then CRM's Data tab:
    /// the CSV import (cancelled), a record's History, and LAST a delete's
    /// Undo toast, whose Undo brings the record back. The surface's own
    /// controls carry no test ids, only their labels.
    @MainActor
    private func walkRichApps(_ app: XCUIApplication) {
        let wanted = { (names: [String]) in names.map(ScreenshotShots.isWanted).contains(true) }

        if wanted(["sg_app-record-table", "sg_app-record-form"]) {
            openAppRun(app, slug: "crm")
            openAppScreen(app, "contacts")
            expect(app.staticTexts["Ada Novak"], within: Wait.sync, "CRM Contacts drew no seeded contact")
            snapshot("sg_app-record-table", settle: 2)
            if ScreenshotShots.isWanted("sg_app-record-form") {
                let add = app.buttons["Add"].firstMatch
                expect(add, within: Wait.nav, "CRM Contacts has no Add")
                add.tap()
                let save = app.buttons["Save"].firstMatch
                expect(save, within: Wait.nav, "Add opened no record form")
                snapshot("sg_app-record-form", settle: 2)
                app.buttons["Cancel"].firstMatch.tap()
                _ = save.waitForNonExistence(timeout: Wait.nav)
            }
            goBack(app)
            settle(1)
        }

        if wanted(["sg_app-calendar", "sg_app-timeline"]) {
            openAppRun(app, slug: "leave-planner")
            // The Page's subtitle: the shown month (the seed moves the sample's leave into it).
            let month = DateFormatter()
            month.locale = Locale(identifier: "en_US")
            month.dateFormat = "MMMM yyyy"
            expect(anyElement(app, containing: month.string(from: Date())), within: Wait.sync, "Leave planner drew no Calendar")
            snapshot("sg_app-calendar", settle: 2)
            if ScreenshotShots.isWanted("sg_app-timeline") {
                openAppScreen(app, "timeline")
                expect(app.staticTexts["Who is out"], within: Wait.sync, "Leave planner drew no Timeline")
                snapshot("sg_app-timeline", settle: 2)
            }
            goBack(app)
            settle(1)
        }

        if wanted(["sg_app-connect", "sg_app-settings-links", "sg_app-settings-automations"]) {
            openAppRun(app, slug: "ops-desk")
            expect(anyElement(app, identified: "exponential-ui-upstream-connect"), within: Wait.network, "Ops desk raised no Connect banner")
            snapshot("sg_app-connect", settle: 2)
            if wanted(["sg_app-settings-links", "sg_app-settings-automations"]) {
                let runMenu = anyElement(app, identified: "app-run-menu")
                expect(runMenu, within: Wait.nav, "Ops desk's run view has no … menu")
                runMenu.tap()
                let editItem = anyElement(app, identified: "app-menu-edit")
                expect(editItem, within: Wait.nav, "Ops desk's menu has no Edit app")
                editItem.tap()
                let menu = anyElement(app, identified: "app-editor-menu")
                expect(menu, within: Wait.sync, "Ops desk's editor has no … menu")
                menu.tap()
                let settingsItem = anyElement(app, identified: "app-menu-settings")
                expect(settingsItem, within: Wait.nav, "Ops desk's editor menu has no Settings")
                settingsItem.tap()
                let sheet = anyElement(app, identified: "app-editor-settings")
                expect(sheet, within: Wait.sync, "Ops desk's Settings sheet never opened")
                expect(anyElement(app, identified: "app-settings-share-links"), within: Wait.network, "Ops desk's Settings drew no Public links")
                snapshot("sg_app-settings-links", settle: 2)
                if ScreenshotShots.isWanted("sg_app-settings-automations") {
                    // Inside the sheet's scroller, down to its last groups.
                    let from = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.9))
                    from.press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)))
                    expect(anyElement(app, identified: "app-settings-webhooks"), within: Wait.nav, "Ops desk's Settings drew no Webhooks")
                    snapshot("sg_app-settings-automations", settle: 2)
                }
                dismissSheet(app, whileVisible: sheet)
                goBack(app)
                settle(1)
            }
            goBack(app)
            settle(1)
        }

        if wanted(["sg_app-publish", "sg_app-versions-sweep", "sg_app-version-preview"]) {
            openAppEditor(app, slug: "team-dashboard")
            let menu = anyElement(app, identified: "app-editor-menu")
            expect(menu, within: Wait.sync, "Team dashboard's editor has no … menu")
            menu.tap()
            let publish = anyElement(app, identified: "app-editor-publish")
            expect(publish, within: Wait.nav, "Team dashboard's menu has no Publish: is its head ahead of the published version?")
            snapshot("sg_app-publish", settle: 2)
            let versionsItem = anyElement(app, identified: "app-menu-versions")
            expect(versionsItem, within: Wait.nav, "Team dashboard's menu has no Versions")
            versionsItem.tap()
            let versions = anyElement(app, identified: "app-editor-versions")
            expect(versions, within: Wait.sync, "Team dashboard's Versions never opened")
            expect(anyElement(app, containing: "Migrating records"), within: Wait.sync, "Versions drew no sweep line: reseed (a Publish supersedes it)")
            snapshot("sg_app-versions-sweep", settle: 2)
            if ScreenshotShots.isWanted("sg_app-version-preview") {
                let preview = anyElement(app, identified: "app-version-preview")
                expect(preview, within: Wait.nav, "Versions has no Preview row")
                preview.tap()
                expect(anyElement(app, identified: "app-editor-viewing"), within: Wait.sync, "Preview opened no viewing banner")
                snapshot("sg_app-version-preview", settle: 3)
            } else {
                dismissSheet(app, whileVisible: versions)
            }
            goBack(app)
            settle(1)
        }

        if wanted(["sg_app-csv-import", "sg_app-record-history", "sg_app-record-undo"]) {
            openAppEditor(app, slug: "crm")
            let dataTab = anyElement(app, identified: "app-editor-tabs-data")
            expect(dataTab, within: Wait.sync, "CRM's editor has no Data tab")
            dataTab.tap()
            expect(anyElement(app, identified: "app-data-row"), within: Wait.sync, "CRM's Data tab drew no record")
            if ScreenshotShots.isWanted("sg_app-csv-import") {
                let importButton = anyElement(app, identified: "app-data-import")
                expect(importButton, within: Wait.nav, "The Data tab has no Import…")
                importButton.tap()
                let field = anyElement(app, identified: "app-csv-text")
                expect(field, within: Wait.nav, "Import… opened no CSV field")
                focus(field)
                field.typeText("name,stage,domain\nInitech,lead,initech.example\nUmbrella,active,umbrella.example\nHooli,nosuch,hooli.example")
                dismissKeyboard(app)
                expect(anyElement(app, identified: "app-csv-preview"), within: Wait.nav, "The pasted CSV drew no preview")
                snapshot("sg_app-csv-import", settle: 2)
                anyElement(app, identified: "app-csv-cancel").tap()
                _ = field.waitForNonExistence(timeout: Wait.nav)
            }
            let rowMenus = app.buttons.matching(identifier: "app-data-row-menu")
            if ScreenshotShots.isWanted("sg_app-record-history") {
                rowMenus.firstMatch.tap()
                let edit = anyElement(app, identified: "app-data-menu-edit")
                expect(edit, within: Wait.nav, "A Data row's menu has no Edit")
                edit.tap()
                expect(anyElement(app, identified: "app-data-history-row"), within: Wait.sync, "The record form drew no History row")
                snapshot("sg_app-record-history", settle: 2)
                let cancel = anyElement(app, identified: "app-data-cancel")
                cancel.tap()
                _ = cancel.waitForNonExistence(timeout: Wait.nav)
            }
            if ScreenshotShots.isWanted("sg_app-record-undo") {
                let rows = rowMenus.count
                rowMenus.element(boundBy: rows - 1).tap()
                let delete = anyElement(app, identified: "app-data-menu-delete")
                expect(delete, within: Wait.nav, "A Data row's menu has no Delete")
                delete.tap()
                let confirm = anyElement(app, identified: "glass-alert-app-data-delete-confirm")
                expect(confirm, within: Wait.nav, "Delete raised no confirm prompt")
                confirm.tap()
                let undo = app.buttons["Undo"].firstMatch
                expect(undo, within: Wait.nav, "The delete raised no Undo toast")
                snapshot("sg_app-record-undo", settle: 1)
                undo.tap()
                let restored = NSPredicate(format: "count == %d", rows)
                let back = expectation(for: restored, evaluatedWith: rowMenus)
                if XCTWaiter().wait(for: [back], timeout: Wait.sync) != .completed {
                    XCTFail("Undo did not bring the deleted record back: \(rowMenus.count) rows of \(rows)")
                }
            }
            goBack(app)
            settle(1)
        }
    }

    /// An Apps list row's tap: the app's run view.
    @MainActor
    private func openAppRun(_ app: XCUIApplication, slug: String) {
        let row = anyElement(app, identified: "apps-row-\(slug)")
        expect(row, within: Wait.sync, "The Apps list has no \(slug) row: reseed with `bun run seed:screenshots`")
        row.tap()
        expect(anyElement(app, identified: "app-run"), within: Wait.sync, "The \(slug) row opened no run view")
    }

    /// An Apps list row's … › Edit app: the app's editor.
    @MainActor
    private func openAppEditor(_ app: XCUIApplication, slug: String) {
        let menu = anyElement(app, identified: "apps-row-menu-\(slug)")
        expect(menu, within: Wait.sync, "The Apps list has no \(slug) row: reseed with `bun run seed:screenshots`")
        menu.tap()
        let edit = anyElement(app, identified: "app-menu-edit")
        expect(edit, within: Wait.nav, "The \(slug) row's menu has no Edit app")
        edit.tap()
        expect(anyElement(app, identified: "app-editor-tabs"), within: Wait.sync, "The \(slug) editor drew no tabs")
    }

    /// The run view's tab bar (`previewScreens`): one screen.
    @MainActor
    private func openAppScreen(_ app: XCUIApplication, _ screen: String) {
        let tab = anyElement(app, identified: "app-editor-preview-screens-\(screen)")
        expect(tab, within: Wait.sync, "The run view has no \(screen) screen")
        tab.tap()
    }

    /// VAPP-110: the Apps TAB (#EXP-1384) → the list (gated on the seeded
    /// Working hours row) → VAPP-242: the row RUNS it (sg_app-run, gated on
    /// the run view) → its … › Edit app → the editor → the Preview tab (VAPP-122: the app
    /// rendered through the host over its records, gated on the preview
    /// frame) → VAPP-178: the Data tab (the records list) → back on the
    /// Chat segment (#EXP-1650: web-mobile opens the menu sheets over Chat)
    /// → VAPP-177: the header's … → Versions, then … → Settings (each a
    /// sheet, dismissed) → … → Export (VAPP-121) → back to the run view → back
    /// to the Apps list.
    @MainActor
    private func walkApps(_ app: XCUIApplication) {
        let appsTab = app.buttons["tab-apps"]
        expect(appsTab, within: Wait.nav, "Apps tab missing")
        appsTab.tap()
        let appRow = anyElement(app, identified: "apps-row-\(Self.appSlug)")
        expect(appRow, within: Wait.sync, "The Apps list has no \(Self.appName) row — reseed with `bun run seed:screenshots`")
        snapshot("sg_apps", settle: 2)
        if ScreenshotShots.isWanted("sg_app-import") {
            walkAppImport(app)
        }
        if ScreenshotShots.isWanted("sg_app-draft") {
            walkAppDraft(app)
        }
        guard ["sg_app-run", "sg_app", "sg_app-versions", "sg_app-settings", "sg_app-data", "sg_app-export"].map(ScreenshotShots.isWanted).contains(true) else { return }
        appRow.tap()
        expect(anyElement(app, identified: "app-run"), within: Wait.sync, "The \(Self.appName) row opened no run view")
        if ScreenshotShots.isWanted("sg_app-run") {
            snapshot("sg_app-run", settle: 3)
        }
        guard ["sg_app", "sg_app-versions", "sg_app-settings", "sg_app-data", "sg_app-export"].map(ScreenshotShots.isWanted).contains(true) else {
            goBack(app)
            settle(1)
            return
        }
        let runMenu = anyElement(app, identified: "app-run-menu")
        expect(runMenu, within: Wait.nav, "The run view has no … menu")
        runMenu.tap()
        let editItem = anyElement(app, identified: "app-menu-edit")
        expect(editItem, within: Wait.nav, "The run view's menu has no Edit app")
        editItem.tap()
        expect(anyElement(app, identified: "app-editor-tabs"), within: Wait.sync, "The editor drew no Chat | Preview control")
        if ScreenshotShots.isWanted("sg_app") {
            let previewTab = anyElement(app, identified: "app-editor-tabs-preview")
            expect(previewTab, within: Wait.nav, "The editor's control has no Preview segment")
            previewTab.tap()
            expect(
                anyElement(app, identified: "app-editor-preview"),
                within: Wait.sync,
                "The \(Self.appName) editor never drew its preview"
            )
            snapshot("sg_app", settle: 3)
        }
        if ScreenshotShots.isWanted("sg_app-data") {
            // VAPP-178 (app-records.json `dataTab`, PHONES): the Data
            // segment = the kind picker + Add record over the seeded
            // records' grouped list.
            let dataTab = anyElement(app, identified: "app-editor-tabs-data")
            expect(dataTab, within: Wait.nav, "The editor's control has no Data segment")
            dataTab.tap()
            expect(
                anyElement(app, identified: "app-data-kind"),
                within: Wait.sync,
                "The \(Self.appName) Data tab drew no kind picker (the head package declares no records?)"
            )
            expect(
                anyElement(app, identified: "app-data-row"),
                within: Wait.sync,
                "The \(Self.appName) Data tab listed no record — reseed with `bun run seed:screenshots`"
            )
            snapshot("sg_app-data", settle: 2)
        }
        // #EXP-1650: the menu sheets open over the Chat segment (web-mobile).
        if ["sg_app-versions", "sg_app-settings", "sg_app-export"].map(ScreenshotShots.isWanted).contains(true) {
            let chatTab = anyElement(app, identified: "app-editor-tabs-chat")
            expect(chatTab, within: Wait.nav, "The editor's control has no Chat segment")
            chatTab.tap()
            settle(1)
        }
        if ScreenshotShots.isWanted("sg_app-versions") {
            walkAppMenuSheet(app, item: "app-menu-versions", sheet: "app-editor-versions", shot: "sg_app-versions")
        }
        if ScreenshotShots.isWanted("sg_app-settings") {
            walkAppMenuSheet(app, item: "app-menu-settings", sheet: "app-editor-settings", shot: "sg_app-settings")
        }
        if ScreenshotShots.isWanted("sg_app-export") {
            walkAppExport(app)
        }
        // The editor back to the run view, the run view back to the list.
        goBack(app)
        settle(1)
        goBack(app)
        settle(1)
    }

    /// VAPP-177 (apps.json PHONES): the editor header's … (`app-editor-menu`)
    /// → `item` → its sheet (`sheet`, gated on the container) → `shot` →
    /// dismissed; ends on the editor.
    @MainActor
    private func walkAppMenuSheet(_ app: XCUIApplication, item: String, sheet: String, shot: String) {
        let menu = anyElement(app, identified: "app-editor-menu")
        expect(menu, within: Wait.nav, "The \(Self.appName) editor has no … menu")
        menu.tap()
        let entry = anyElement(app, identified: item)
        expect(entry, within: Wait.nav, "The editor's … menu has no \(item)")
        entry.tap()
        let container = anyElement(app, identified: sheet)
        expect(container, within: Wait.sync, "\(item) opened no \(sheet) sheet")
        if shot == "sg_app-versions" {
            snapshot("sg_app-versions", settle: 2)
        } else {
            snapshot("sg_app-settings", settle: 2)
        }
        // The fitted Versions sheet scrolls under a drag: key the dismissal
        // on its title; the Settings sheet is full height (its own top).
        dismissSheet(app, whileVisible: shot == "sg_app-versions" ? app.staticTexts["Versions"] : container)
        settle(1)
    }

    /// VAPP-121 (app-export.json `capture`): the editor header's `…` →
    /// Export → Include records ON → sg_app-export → dismissed; ends on the
    /// editor (#EXP-1650: over the Chat segment, like web-mobile).
    @MainActor
    private func walkAppExport(_ app: XCUIApplication) {
        let menu = anyElement(app, identified: "app-editor-menu")
        expect(menu, within: Wait.nav, "The \(Self.appName) editor has no … menu")
        menu.tap()
        let exportItem = anyElement(app, identified: "app-menu-export")
        expect(exportItem, within: Wait.nav, "The editor's … menu has no Export (the app has no version?)")
        exportItem.tap()
        let dialog = anyElement(app, identified: "app-export-dialog")
        expect(dialog, within: Wait.nav, "Export opened no sheet")
        let records = anyElement(app, identified: "app-export-records")
        expect(records, within: Wait.sync, "The Export sheet has no Include records row — reseed with `bun run seed:screenshots`")
        for _ in 0..<3 where (records.value as? String) != "On" {
            records.tap()
            usleep(400_000)
        }
        snapshot("sg_app-export", settle: 2)
        dismissSheet(app, whileVisible: app.staticTexts["Export \(Self.appName)"])
        settle(1)
    }

    /// VAPP-176 (apps.json NEW APP): Apps list → the seeded Release checklist
    /// row's … › Edit app (VAPP-242; a draft: no version, no editor session) → its editor on the Chat
    /// tab: the Draft badge, `What should this app do?` with the starter
    /// pills, the composer → sg_app-draft → back to the Apps list.
    @MainActor
    private func walkAppDraft(_ app: XCUIApplication) {
        let menu = anyElement(app, identified: "apps-row-menu-\(Self.draftAppSlug)")
        expect(menu, within: Wait.sync, "The Apps list has no \(Self.draftAppName) row — reseed with `bun run seed:screenshots`")
        menu.tap()
        let editItem = anyElement(app, identified: "app-menu-edit")
        expect(editItem, within: Wait.nav, "The row's menu has no Edit app")
        editItem.tap()
        expect(
            anyElement(app, identified: "app-editor-starter"),
            within: Wait.sync,
            "The \(Self.draftAppName) draft's editor drew no starter pills (has it a version or an editor session?)"
        )
        // EXP-1558: the composer takes focus on open; the other clients
        // photograph the page without a keyboard.
        dismissKeyboard(app)
        snapshot("sg_app-draft", settle: 2)
        goBack(app)
        settle(1)
    }

    /// VAPP-121 + VAPP-242: Apps list → the band's New app pill → THE NEW
    /// MENU's Import… → the capture
    /// text typed → the server's `/package: is required` row → sg_app-import
    /// → dismissed.
    @MainActor
    private func walkAppImport(_ app: XCUIApplication) {
        let newPill = anyElement(app, identified: "apps-new")
        expect(newPill, within: Wait.nav, "The Apps band has no New app pill")
        newPill.tap()
        let importItem = anyElement(app, identified: "apps-import")
        expect(importItem, within: Wait.nav, "THE NEW MENU has no Import…")
        importItem.tap()
        let field = anyElement(app, identified: "app-import-text")
        expect(field, within: Wait.nav, "The import step has no text field")
        focus(field)
        field.typeText(#"{"format":"exponential-app/1","app":{"name":"Todos"}}"#)
        expect(
            anyElement(app, containing: "/package: is required"),
            within: Wait.network,
            "The import step never listed the server's /package issue"
        )
        dismissKeyboard(app)
        snapshot("sg_app-import", settle: 2)
        dismissSheet(app, whileVisible: app.staticTexts["Import an app"])
        settle(1)
    }

    /// results-tab.json: Chats → APP-14's chat → its Results tab
    /// (`sg_chat-results`); ends back on the Chats list.
    @MainActor
    private func walkChatResults(_ app: XCUIApplication) {
        let chatsTab = app.buttons["tab-chats"]
        expect(chatsTab, within: Wait.nav, "Chats tab missing")
        chatsTab.tap()
        let row = chatRow(app, containing: Self.guideTitle)
        expect(row, within: Wait.sync, "The Chats list has no \"\(Self.guideTitle)\" chat — reseed with `bun run seed:screenshots`")
        row.tap()
        let tab = anyElement(app, identified: "chat-results-tab")
        expect(tab, within: Wait.sync, "The chat strip drew no Results tab")
        XCTAssertTrue(tab.isHittable, "The Results tab is off screen — the strip must never scroll")
        tab.tap()
        expect(
            anyElement(app, identified: "chat-results-changes"),
            within: Wait.network,
            "The Results tab drew no Changes row — are the seeded PR's files reachable on GitHub?"
        )
        expect(anyElement(app, identified: "result-section"), within: Wait.sync, "The Results tab drew no result")
        snapshot("sg_chat-results", settle: 3)
        goBack(app)
        settle(1)
    }

    /// EXP-1309: the Chats tab → the Triage agent's channel → back → the
    /// cold-start chat → its "Profile the launch path" session, each shot
    /// gated on its seeded content; ends back on the Chats list.
    @MainActor
    private func walkChats(_ app: XCUIApplication) {
        let chatsTab = app.buttons["tab-chats"]
        expect(chatsTab, within: Wait.nav, "Chats tab missing")
        chatsTab.tap()
        // ── sg_chats: gate on the rows' quiet words (pull requests + the
        // pending question have synced), not on the list container.
        // EXP-1339: `merged` draws no word (only the violet lead).
        for word in ["Question", "stack"] {
            expect(
                chatRow(app, containing: word),
                within: Wait.sync,
                "No chat row says \"\(word)\" — reseed with `bun run seed:screenshots`"
            )
        }
        snapshot("sg_chats", settle: 2)

        // ── sg_agent-channel: the Triage agent's chat (DEMO_AGENT_ID) off its
        // pinned agent row (the agent rows sit above every
        // chat row, so the first `chat-row` named Triage is its own). Like Android, a scoped run that
        // does not ask for this shot skips the walk.
        if ScreenshotShots.isWanted("sg_agent-channel") {
            let agentRow = app.descendants(matching: .any)
                .matching(identifier: "chat-row")
                .matching(NSPredicate(format: "label CONTAINS %@", Self.agentName))
                .firstMatch
            expect(agentRow, within: Wait.sync, "The Chats list has no \(Self.agentName) agent row")
            agentRow.tap()
            expect(
                anyElement(app, containing: Self.agentRequest),
                within: Wait.sync,
                "\(Self.agentName)'s channel never drew the seeded request"
            )
            snapshot("sg_agent-channel", settle: 2)
            goBack(app)
        }
        // A subset run that wants neither of the next two stops here: the
        // session step needs the relay stub (`bun run screenshots:desktop`).
        let sessionShots = ["sg_session", "sg_script-card", "sg_work-card"]
        guard ScreenshotShots.isWanted("sg_chat-screen") || sessionShots.contains(where: ScreenshotShots.isWanted) else { return }

        // ── sg_chat-screen: the cold-start chat (DEMO_CHAT_ID) ──────────────
        let coldStart = chatRow(app, containing: Self.coldStartChatTitle)
        expect(coldStart, within: Wait.nav, "Did not return to the Chats list")
        coldStart.tap()
        // EXP-1334: the strip never scrolls — the "+" and Conversation pinned
        // left, the chips folded beside them (S-3: no Guide tab; the seeded
        // question + the child's plan stay inline, not in the Dock).
        for id in ["chat-strip-add", "chat-tab-conversation"] {
            let pinned = anyElement(app, identified: id)
            expect(pinned, within: Wait.sync, "The chat strip drew no \(id)")
            XCTAssertTrue(pinned.isHittable, "\(id) is off screen — the strip must never scroll")
        }
        // EXP-1558: the Conversation at its newest rows, as every other
        // client photographs it: the pending question and the child's plan
        // row in view (#EXP-1662: a plan is ONE row here, its buttons live on
        // the Plan tab). The jump-to-bottom button re-pins a list that opened
        // scrolled up.
        let jump = anyElement(app, identified: "jump-to-bottom")
        if jump.waitForExistence(timeout: 2), jump.isHittable { jump.tap() }
        expect(
            anyElement(app, containing: "Plan: lazy-load"),
            within: Wait.sync,
            "The cold-start chat drew no plan row — reseed with `bun run seed:screenshots`"
        )
        expect(
            anyElement(app, containing: "Cold start is at 740"),
            within: Wait.nav,
            "The cold-start chat drew no pending question"
        )
        snapshot("sg_chat-screen", settle: 2)
        guard sessionShots.contains(where: ScreenshotShots.isWanted) else {
            goBack(app)
            settle(1)
            return
        }

        // ── sg_session: that session's page in the chat (DEMO_SESSION_ID) ─
        // The relay stub's feed ends on an UNANSWERED question, whose card
        // waits in THE Dock (N-12). Nothing is answered.
        // #EXP-1433: the strip holds no session chips (issues + sessions fold
        // into ONE group tab), so its Conversation card opens the page.
        let sessionCard = app.descendants(matching: .any)
            .matching(identifier: "session-card")
            .matching(NSPredicate(format: "label CONTAINS %@", Self.sessionTitle))
            .firstMatch
        // NEW-16: scroll the card BELOW the strip, then tap its centre: a
        // card scrolled under the floating header still reads hittable, and
        // XCUITest would tap the header's back button instead.
        revealAndTap(
            app,
            sessionCard,
            below: anyElement(app, identified: "chat-tab-conversation"),
            "The cold-start chat drew no \"\(Self.sessionTitle)\" card"
        )
        // NEW-16: a tap that missed the card lands on the header's back
        // button and pops to the Chats list; say so instead of waiting a
        // minute for a Dock that can never draw.
        settle(1)
        if !anyElement(app, identified: "chat-tab-conversation").exists,
           chatRow(app, containing: Self.coldStartChatTitle).exists {
            XCTFail("Opening \"\(Self.sessionTitle)\" popped the chat back to the Chats list: the tap missed the card (header or status bar)")
            return
        }
        // EXP-1343: a session chip of THIS chat opens as its page under
        // the chat's ONE header (the chat title, Stop while live) — no
        // pushed session screen, no back row (Android's `sg_session` is the
        // same page). S-3: the relay feed's pending question stays inline
        // (`feed-question`, dock.json) once the feed is live and answerable;
        // the neighbouring Conversation page's cards are off screen, so gate
        // on a HITTABLE one.
        let relayCard = app.descendants(matching: .any).matching(identifier: "feed-question")
        let deadline = Date().addingTimeInterval(Wait.firstSync)
        while Date() < deadline, !relayCard.allElementsBoundByIndex.contains(where: \.isHittable) {
            settle(1)
        }
        XCTAssertTrue(
            relayCard.allElementsBoundByIndex.contains(where: \.isHittable),
            "The session drew no pending question inline — is `bun run screenshots:desktop` publishing its feed?"
        )
        // #EXP-1433: the group tab reads the open item's name.
        expect(
            app.descendants(matching: .any)
                .matching(identifier: "chat-strip-group-label")
                .matching(NSPredicate(format: "label == %@", Self.sessionTitle))
                .firstMatch,
            within: Wait.sync,
            "The session did not open as the chat's \"\(Self.sessionTitle)\" page"
        )
        // S-3: no header Stop (the composer's stop-turn is the one).
        snapshot("sg_session", settle: 2)
        // ── sg_script-card · sg_work-card (EXP-1654): the SAME relay feed
        // web and desktop photograph, dragged up from its tail to the
        // code-mode call's Script card, then the `cold-start-review`
        // workflow's Work card above it: in the session, never isolated.
        // A card under the floating strip still reads hittable, so it must
        // sit below the strip's bottom edge.
        // The names stay literal: views.test.ts reads them off the source.
        let strip = anyElement(app, identified: "chat-tab-conversation")
        // The Work card is found by its workflow's title: the feed holds more
        // than one `work-card` (firstMatch named one far above the strip).
        let revealFeedCard = { (id: String) in
            let card = id == "work-card"
                ? app.staticTexts["cold-start-review"].firstMatch
                : self.anyElement(app, identified: id)
            var drags = 0
            while drags < 14, !(card.exists && card.isHittable && card.frame.minY > strip.frame.maxY) {
                app.swipeDown(velocity: .slow)
                drags += 1
            }
            XCTAssertTrue(
                card.exists && card.isHittable,
                "The session feed drew no \(id) — is `bun run screenshots:desktop` publishing its feed?"
            )
        }
        if ScreenshotShots.isWanted("sg_script-card") || ScreenshotShots.isWanted("sg_work-card") {
            revealFeedCard("script-card")
            snapshot("sg_script-card", settle: 2)
            revealFeedCard("work-card")
            snapshot("sg_work-card", settle: 2)
        }
        // Back once (the session page → the Chats list) on the system
        // back button (`goBack`: by name, never the bar's `…`).
        goBack(app)
        settle(1)
    }

    /// A Chats row whose (combined) label contains `fragment`.
    @MainActor
    private func chatRow(_ app: XCUIApplication, containing fragment: String) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(identifier: "chat-row")
            .matching(NSPredicate(format: "label CONTAINS %@", fragment))
            .firstMatch
    }

    /// #EXP-1384: the board menu → the named board (on the board view the
    /// Board tab opens the menu; its rows are labelled with the board names).
    @MainActor
    private func switchBoard(_ app: XCUIApplication, to name: String) {
        showBoard(app)
        app.buttons["tab-board"].tap()
        let menu = anyElement(app, identified: "board-menu")
        expect(menu, within: Wait.nav, "The board menu did not open")
        let row = app.buttons.matching(identifier: "board-menu-row")
            .matching(NSPredicate(format: "label CONTAINS %@", name)).firstMatch
        expect(row, within: Wait.nav, "Board \"\(name)\" missing from the board menu")
        row.tap()
        _ = menu.waitForNonExistence(timeout: Wait.nav)
        settle(1)
    }
}
