import XCTest

/// Automated App Store screenshots (fastlane snapshot).
///
/// Drives the real app against a seeded local backend: sign in (a reused
/// session handed to the app, EXP-1267; the InstanceView/LoginView flow only as
/// the fallback), wait for Electric to sync the demo team, then
/// capture the seven store shots — board, issue detail, Start-coding dialog,
/// live steering, PR review, actions, inbox (the support inbox left with the
/// helpdesk (gone, SLOP-4)). EXP-393 replaced
/// the comments / board-switcher / agents-list / reviews-list / search shots
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
///   3. `bun run screenshots:desktop` left running for the whole capture
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

        let app = makeScreenshotApp()
        // EXP-1267: a reused session, no login UI; false = the UI fallback ran.
        let injected = launchSignedIn(app)

        // The app lands on the Agent tab — switch to Issues for the board.
        let issuesTab = app.buttons["tab-issues"]
        expect(issuesTab, within: Wait.sync, "Tab bar never appeared — did the sign-in land?")
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
        let commentsHeader = app.staticTexts["comment-thread-header"]
        var detailOpened = false
        for _ in 0..<2 {
            if showcaseRowTitle.exists && showcaseRowTitle.isHittable {
                showcaseRowTitle.tap()
            }
            // The comment thread renders once the comments shape has synced;
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
        // EXP-893/1150: the reader's OWN live run grows the Work screen's Run
        // TAB — that is what makes this shot say "an agent is coding on this
        // right now" rather than "live steering is unavailable on this
        // instance". Wait for the tab, never a caption.
        let runTab = app.descendants(matching: .any)
            .matching(identifier: "work-face-run").firstMatch
        expect(
            runTab,
            within: Wait.sync,
            "No live session on \(Self.showcaseIdentifier) — is screenshots:desktop running against the relay?"
        )
        snapshot("02_issue-detail", settle: 2, popRects: app)

        // ── 04: live steering ───────────────────────────────────────────────
        // The Run tab flips the SAME screen to its Run face (EXP-1150).
        // EXP-1175: the face opens as the THREAD (the status row over the
        // run's results, the pending question card still in place), so wait
        // for the status row, which both renderings wear.
        runTab.tap()
        let statusRow = app.descendants(matching: .any)
            .matching(identifier: "run-status-row").firstMatch
        expect(
            statusRow,
            within: Wait.nav,
            "The Run face never appeared — is screenshots:desktop publishing to the relay?"
        )
        // An EMPTY feed still renders the container (a dropped relay socket
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
            "The relay never replayed the transcript — is STEER_RELAY_URL reachable from the simulator?"
        )
        snapshot("04_steering", settle: 3, popRects: app)

        // ── 03: the Agent page composer (EXP-825, shot id unchanged) ────────
        // From a repo-backed issue the demo user is NOT already coding on, so
        // the circle offers the start action — which PUSHES the Agent page
        // with the issue chipped. The circle needs an online desktop: without
        // one it shows a notice instead of navigating. EXP-893: the run is a
        // face of the issue's screen, so ONE Back returns to the board.
        goBack(app)
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
            "The Agent page did not open — is a desktop online on the relay?"
        )
        snapshot("03_start-coding", settle: 2, popRects: app)
        // Agent page → issue → board.
        goBack(app)
        goBack(app)

        // ── 05: PR review (real diff + merge bar) ───────────────────────────
        // EXP-1251: a Reviews row opens the issue's Work screen on its Guide;
        // its "Show complete diff" row opens the whole diff page (the cards over
        // `[files][Merge PR]`). The file list comes from GitHub via
        // issues.prFiles — the seed points APP-14 at a real public PR so
        // there is an actual diff to show.
        let reviewsTab = app.buttons["tab-reviews"]
        expect(reviewsTab, within: Wait.nav, "Reviews tab missing")
        reviewsTab.tap()
        let reviewPr = reviewRow(app, titled: Self.reviewTitle)
        expect(reviewPr, within: Wait.sync, "Reviews tab never showed the seeded open PRs")
        reviewPr.tap()
        openGuideDiffPage(
            app,
            failure: "The PR diff never loaded — check SCREENSHOT_PR_URL is a reachable public PR"
        )
        // EXP-916: every file card starts OPEN (only a huge one folds itself
        // away), so the shot already shows patches rather than a filename list.
        snapshot("05_review", settle: 2, popRects: app)
        goBack(app)

        // ── 06: actions (EXP-253) — the seed inserts three team actions.
        // EXP-1187: Actions is a tab of its own; no builtins in the list.
        let actionsTab = app.buttons["tab-actions"]
        expect(actionsTab, within: Wait.nav, "Actions tab missing")
        actionsTab.tap()
        let actionRow = app.descendants(matching: .any)
            .matching(identifier: "action-row").firstMatch
        expect(actionRow, within: Wait.sync, "Actions list never showed the seeded actions")
        expect(
            app.staticTexts["Update dependencies"].firstMatch,
            within: Wait.sync,
            "Seeded team actions never synced"
        )
        snapshot("06_actions", settle: 2, popRects: app)

        // ── 07: inbox (the Inbox tab, Inbox segment — the default) ──────────
        // Wait for a real notification group — capturing the "You're all
        // caught up" empty state would silently ship an empty store shot.
        let inboxTab = app.buttons["tab-mywork"]
        expect(inboxTab, within: Wait.nav, "Inbox tab missing")
        inboxTab.tap()
        expect(
            app.staticTexts[Self.showcaseTitle].firstMatch,
            within: Wait.sync,
            "Inbox never showed the seeded notifications"
        )
        snapshot("07_inbox", settle: 2, popRects: app)

        // ── 01: home issue list (captured last, see above) ──────────────────
        app.buttons["tab-issues"].tap()
        expect(showcaseRowTitle, within: Wait.nav, "Board did not come back for the final capture")
        // Opening an issue earlier may have scrolled the list; the board shot
        // has to start at the top of the first group.
        for _ in 0..<3 { app.swipeDown() }
        if !injected { dismissSavePasswordSheet(timeout: 2) }
        snapshot("01_board", settle: 2, popRects: app)

        finished = true
    }

    /// A Reviews row by its title. EXP-1248: a row is a plain Button over
    /// `PrRow`, which COMBINES its texts into one element, so the title is no
    /// standalone staticText: match the button whose label contains it.
    @MainActor
    private func reviewRow(_ app: XCUIApplication, titled title: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label CONTAINS %@", title)).firstMatch
    }
}
