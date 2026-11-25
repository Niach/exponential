import XCTest

/// Automated App Store screenshots (fastlane snapshot).
///
/// Drives the real app against a seeded local backend: sign in (a reused
/// session handed to the app, EXP-1267; the InstanceView/LoginView flow only as
/// the fallback), wait for Electric to sync the demo team, then
/// capture the seven store shots — board, issue detail, Start-coding dialog,
/// live steering, PR review, actions, inbox (the support inbox left with the
/// helpdesk (gone, SLOP-4)). EXP-1295 walks the issue's chat for the
/// steering and review shots (Reviews and the work faces are gone). EXP-393 replaced
/// the comments / board-picker / agents-list / reviews-list / search shots
/// with the three that actually differentiate the product (start coding,
/// steering, diff + merge), and kept the set identical to Android's, where
/// Play caps phone screenshots at 8.
///
/// Run via `fastlane screenshots` (apps/ios). Prerequisites, in order:
///   1. seeded dev server — `apps/web/scripts/seed-screenshots.ts`
///      (demo@exponential.at / screenshots-demo, team "Acme", board
///      "Mobile App", showcase issue APP-5, open PRs, actions)
///   2. a steer relay, with STEER_RELAY_URL + STEER_RELAY_SECRET exported for
///      the web server (`docker compose --profile steer up -d`)
///   3. `bun run screenshots:desktop` left running for the whole capture: the
///      relay only holds its presence, the steering feed itself is served
///      over eNet by its feed stub (VAPP-168)
/// Without 2+3 the instance reports steering disabled: the Start-coding and
/// steering shots are unreachable and the issue detail renders "Live steering
/// is unavailable on this instance." — which is what EXP-393 set out to fix.
///
/// The instance URL defaults to http://localhost:5173 and can be overridden
/// with the SNAPSHOT_INSTANCE_URL environment variable.
///
/// The launch/sign-in/tap helpers live in ScreenshotFlow.swift, shared with
/// StyleguideScreenshots — including the `snapshot(_:settle:popRects:)`
/// overload every capture below goes through. `popRects: app` additionally
/// writes the pop-out rect sidecar the store compositor crops from (EXP-627,
/// see PopRects.swift), and the overload honours the lane's optional `shots:`
/// allowlist (EXP-642).
final class StoreScreenshots: XCTestCase {

    /// Set at the very end of the capture walk, so the `shots:` typo check in
    /// tearDown never piles a second failure onto a run that already broke.
    private var finished = false

    override func tearDown() {
        assertRequestedShotsWereReached(suiteFinished: finished)
        super.tearDown()
    }

    private static let showcaseTitle = "Reduce cold start below 800 ms"
    private static let showcaseIdentifier = "APP-5"
    /// APP-3: repo-backed, member-assigned, and deliberately WITHOUT a coding
    /// session of its own — an issue the demo user is already coding on turns
    /// the bottom-bar circle into the session link instead of the start action.
    private static let startCodingTitle = "Dark mode contrast pass across settings"
    /// A fragment of the question screenshot-desktop.ts ends its transcript on
    /// (DEMO_FEED_QUESTION) — proof that the eNet feed actually arrived. It has
    /// to be the LAST event: the feed is bottom-anchored and lazy, so anything
    /// above the fold is never rendered and no query can find it.
    private static let feedQuestionFragment = "Lazy-load the markdown editor too"
    /// The chat's child session the stand-in desktop serves over eNet
    /// (DEMO_SESSION_ID); the chat root's own card comes first and has no
    /// feed of its own.
    private static let steeredSessionTitle = "Profile the launch path"

    @MainActor
    func testCaptureAppStoreScreenshots() throws {
        continueAfterFailure = false

        let app = makeScreenshotApp()
        // EXP-1267: a reused session, no login UI; false = the UI fallback ran.
        let injected = launchSignedIn(app)

        // The app lands on the Agent tab — switch to the Board tab (#NEW-12:
        // the iPad's sidebar row).
        let issuesTab = shellEntry(app, "tab-board", within: Wait.sync)
        expect(issuesTab, within: 1, "Tab bar never appeared — did the sign-in land?")
        issuesTab.tap()

        // Wait for the board: the FIRST sync after sign-in is the one long
        // wait of the run. 01_board itself is captured LAST — after a UI
        // login the save-password sheet pops at an unpredictable moment
        // several seconds later (it photobombed the iPad board shot twice);
        // by the end of the run it has provably appeared and been dismissed.
        let showcaseRowTitle = app.staticTexts[Self.showcaseTitle]
        expect(
            showcaseRowTitle,
            within: Wait.firstSync,
            "Issue list never synced (missing showcase issue \(Self.showcaseIdentifier))"
        )
        if !injected { dismissSavePasswordSheet(timeout: 3) }

        // ── 02: issue detail (APP-5) ────────────────────────────────────────
        // The detail ScrollView renders its whole content tree, so the comment
        // header existing (even offscreen) means the detail is fully loaded.
        //
        // Tap the row's TITLE text, never the `issue-row-*` button element:
        // since the glass chip rework the row element's accessibility
        // activation point lands on the leading priority control, so an
        // element tap opens the priority picker sheet instead of navigating
        // (EXP-348 — it silently killed all three snapshot retries).
        let commentsHeader = app.staticTexts["comment-replies-header"]
        var detailOpened = false
        for _ in 0..<2 {
            if showcaseRowTitle.exists && showcaseRowTitle.isHittable {
                showcaseRowTitle.tap()
            }
            // The comments render once the comments shape has synced;
            // a second tap only helps when the first one was swallowed (a
            // save-password sheet after a UI login).
            if commentsHeader.waitForExistence(timeout: Wait.sync) {
                detailOpened = true
                break
            }
            if !injected { dismissSavePasswordSheet(timeout: 2) }
        }
        if !detailOpened {
            print("EXP-DEBUG hierarchy after failed detail open:\n\(app.debugDescription)")
        }
        XCTAssertTrue(detailOpened, "Issue detail did not open — missing: \(commentsHeader.description)")
        // EXP-1295: the chat the reader's OWN live session works in shows
        // as the page's Chats chip — that is what makes this shot say "an
        // agent is coding on this right now". Wait for the chip, never a
        // caption.
        let chatChip = app.descendants(matching: .any)
            .matching(identifier: "issue-chat-chip").firstMatch
        expect(
            chatChip,
            within: Wait.sync,
            "No chat on \(Self.showcaseIdentifier) — is screenshots:desktop running against the relay?"
        )
        snapshot("02_issue-detail", settle: 2, popRects: app)

        // ── 04: live steering ───────────────────────────────────────────────
        // EXP-1295 / #EXP-1732: the chip opens the chat; its Sessions tab's
        // row opens the child's session (its raw work, the pending question
        // card still in place) as an item pushed over the tab.
        chatChip.tap()
        let sessionsTab = anyElement(app, identified: "session-tabs-sessions")
        expect(sessionsTab, within: Wait.sync, "The chat drew no Sessions tab")
        sessionsTab.tap()
        let sessionRow = app.descendants(matching: .any)
            .matching(NSPredicate(
                format: "identifier BEGINSWITH %@ AND label CONTAINS %@", "session-sessions-row-", Self.steeredSessionTitle
            ))
            .firstMatch
        expect(sessionRow, within: Wait.sync, "The Sessions tab listed no steered session")
        sessionRow.tap()
        expect(
            app.descendants(matching: .any).matching(identifier: "agent-feed").firstMatch,
            within: Wait.nav,
            "The session view never appeared — is screenshots:desktop running?"
        )
        // An EMPTY feed still renders the container (a dropped eNet link
        // leaves the view "Reconnecting…" with nothing in it), so the tag alone
        // would happily photograph a blank screen. Gate on real content —
        // matched across every element type, since markdown-rendered feed rows
        // surface as TextViews rather than StaticTexts.
        let feedQuestion = app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@", Self.feedQuestionFragment)
        ).firstMatch
        expect(
            feedQuestion,
            within: Wait.network,
            "The eNet feed stub never replayed the transcript — is screenshots:desktop's feed stub up?"
        )
        snapshot("04_steering", settle: 3, popRects: app)

        // ── 03: the Agent page composer (EXP-825, shot id unchanged) ────────
        // From a repo-backed issue the demo user is NOT already coding on, so
        // the circle offers the start action — which PUSHES the home
        // composer with the issue chipped. The circle needs an online desktop:
        // without one it shows a notice instead of navigating. Session →
        // chat → issue → board. #EXP-1732: the session is an item pushed
        // over the chat's Sessions tab, so it takes its own Back; one more
        // than that would land on the board header's switcher sheet.
        // #NEW-12: the iPad's issue is the detail's root, the board beside it.
        goBack(app)
        goBack(app)
        if !isSplitLayout { goBack(app) }
        expect(showcaseRowTitle, within: Wait.nav, "Did not return to the board")
        openIssue(app, title: Self.startCodingTitle)
        let startButton = app.buttons["Start coding"]
        // The circle offers the start action once the readiness inputs (the
        // stand-in desktop's device row) have loaded.
        expect(startButton, within: Wait.sync, "Start-coding control missing")
        startButton.tap()
        let composer = app.descendants(matching: .any)
            .matching(identifier: "agent-composer").firstMatch
        expect(
            composer,
            within: Wait.nav,
            "The Agent page did not open — is the demo desktop online?"
        )
        snapshot("03_start-coding", settle: 2, popRects: app)
        // Agent page → issue → board.
        goBack(app)
        if !isSplitLayout { goBack(app) }

        // ── 05: retired (S-3) — the Guide tab and its diff page entry are
        // gone; results land on the Results tab.

        // ── 06: retired (NEW-7) — the Actions tab is gone: prompts live in
        // the composer's `/` menu and Settings → Prompts.

        // ── 07: inbox (EXP-1373: the page the Chats header's inbox circle
        // pushes). Wait for a real notification group — capturing the
        // "You're all caught up" empty state would silently ship an empty
        // store shot.
        let chatsTab = shellEntry(app, "tab-chats", within: Wait.nav)
        expect(chatsTab, within: 1, "Chats tab missing")
        chatsTab.tap()
        let inboxButton = app.buttons["chats-inbox"]
        expect(inboxButton, within: Wait.nav, "The Chats header has no inbox button")
        inboxButton.tap()
        expect(
            app.staticTexts[Self.showcaseTitle].firstMatch,
            within: Wait.sync,
            "Inbox never showed the seeded notifications"
        )
        snapshot("07_inbox", settle: 2, popRects: app)
        if !isSplitLayout { goBack(app) }

        // ── 01: home issue list (captured last, see above) ──────────────────
        showBoard(app)
        expect(showcaseRowTitle, within: Wait.nav, "Board did not come back for the final capture")
        // Opening an issue earlier may have scrolled the list; the board shot
        // has to start at the top of the first group.
        if isSplitLayout {
            // #NEW-12: the board beside an open issue, the desktop's shape.
            let list = app.coordinate(withNormalizedOffset: CGVector(dx: 0.15, dy: 0.4))
            for _ in 0..<3 {
                list.press(forDuration: 0.05, thenDragTo: list.withOffset(CGVector(dx: 0, dy: 500)))
            }
            openIssue(app, title: Self.startCodingTitle)
            expect(app.buttons["Start coding"], within: Wait.sync, "The board's issue never opened beside it")
        } else {
            for _ in 0..<3 { app.swipeDown() }
        }
        if !injected { dismissSavePasswordSheet(timeout: 2) }
        snapshot("01_board", settle: 2, popRects: app)

        finished = true
    }
}
