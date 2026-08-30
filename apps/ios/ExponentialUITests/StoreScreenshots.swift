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

        let app = launchScreenshotApp()

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
        snapshot("02_issue-detail", settle: 2, popRects: app)

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
        snapshot("04_steering", settle: 3, popRects: app)
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
        snapshot("03_start-coding", settle: 2, popRects: app)
        // EXP-687: sheets have no Cancel — swipe it away.
        dismissSheet(app, whileVisible: startSheet)
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
        snapshot("05_review", settle: 2, popRects: app)
        goBack(app)

        // ── 06: actions (EXP-253) — the seed inserts three team actions.
        // EXP-686: Actions is its own tab; no builtins in the list.
        let actionsTab = app.buttons["tab-actions"]
        XCTAssertTrue(actionsTab.waitForExistence(timeout: 15), "Actions tab missing")
        actionsTab.tap()
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
        snapshot("06_actions", settle: 2, popRects: app)

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
        snapshot("07_inbox", settle: 2, popRects: app)

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
        snapshot("08_support", settle: 2, popRects: app)

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
        snapshot("01_board", settle: 2, popRects: app)

        finished = true
    }
}
