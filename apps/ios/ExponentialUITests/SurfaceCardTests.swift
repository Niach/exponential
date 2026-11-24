import XCTest

/// Session 58cb32df (chats.json SURFACE CARD): the inline preview on the
/// styleguide's chat-transcript specimen — the real renderer paints it, a
/// `reply` press hands its text out.
final class SurfaceCardTests: XCTestCase {
    @MainActor
    func testSurfaceReply() throws {
        continueAfterFailure = false
        let app = makeScreenshotApp()
        launchSignedOut(app, extraArguments: ["-uiTestingStyleguide", "chat-transcript"])
        let ship = app.buttons["Ship snapshot"].firstMatch
        XCTAssertTrue(ship.waitForExistence(timeout: 20), "The surface card's renderer drew no Ship snapshot button")
        // The live renderer lays its hit areas out after its first frame.
        Thread.sleep(forTimeInterval: 1.5)
        ship.tap()
        let echo = app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "reply → Ship the snapshot")).firstMatch
        XCTAssertTrue(echo.waitForExistence(timeout: 5), "The reply press posted nothing")
    }

    /// #EXP-2076 + #EXP-2074 (chat-express.json DRAW): the chat-transcript
    /// specimen's call view turn and subagent lane narration (the ×4 sample:
    /// "Checked the three lanes." · one block · "All green.") draw the
    /// block as a surface in place, never its raw. Each frame is kept as an
    /// attachment (the PR pictures: `xcresulttool export attachments`).
    @MainActor
    func testExpressBlocksInCallViewAndSubagentLane() throws {
        continueAfterFailure = false
        let app = makeScreenshotApp()
        launchSignedOut(app, extraArguments: ["-uiTestingStyleguide", "chat-transcript"])
        XCTAssertTrue(
            anyElement(app, identified: "chat-transcript-specimen").waitForExistence(timeout: 20),
            "The chat-transcript specimen never opened"
        )
        for (specimen, picture) in [("call-view-specimen", "ios call view express"), ("subagent-lane-express-specimen", "ios subagent lane express")] {
            let root = anyElement(app, identified: specimen)
            XCTAssertTrue(root.waitForExistence(timeout: 10), "\(specimen) is missing")
            let last = root.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", "All green.")).firstMatch
            // Slow drags without momentum until the turn's last prose run
            // sits in the window's lower half.
            for _ in 0..<24 where !(last.exists && last.isHittable && last.frame.maxY < app.frame.height * 0.7) {
                let from = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.8))
                from.press(
                    forDuration: 0.05, thenDragTo: from.withOffset(CGVector(dx: 0, dy: -app.frame.height * 0.3)),
                    withVelocity: .slow, thenHoldForDuration: 0.3
                )
            }
            XCTAssertTrue(last.isHittable, "\(specimen): no \"All green.\" bubble on screen")
            XCTAssertTrue(
                root.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", "Checked the three lanes.")).firstMatch.exists,
                "\(specimen): the first prose run is missing"
            )
            let block = root.descendants(matching: .any).matching(identifier: "express-block").firstMatch
            XCTAssertTrue(block.waitForExistence(timeout: 10), "\(specimen): the block drew no surface")
            XCTAssertTrue(
                root.buttons["Ship it"].firstMatch.waitForExistence(timeout: 10),
                "\(specimen): the block's surface drew no Ship it button"
            )
            XCTAssertTrue(
                root.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", "Build 1482 is green")).firstMatch.exists
                    && !root.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", "root = Stack")).firstMatch.exists,
                "\(specimen): the block's surface is not drawn, or its raw is drawn as text"
            )
            Thread.sleep(forTimeInterval: 1.5)
            let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
            shot.name = picture
            shot.lifetime = .keepAlways
            add(shot)
        }
    }
}
