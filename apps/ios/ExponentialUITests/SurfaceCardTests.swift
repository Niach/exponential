import XCTest

/// Thread 58cb32df (chats.json SURFACE CARD): the inline preview on the
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
}
