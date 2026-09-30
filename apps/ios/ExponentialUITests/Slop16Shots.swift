import XCTest

/// THROWAWAY (SLOP-16 live check): photographs the work header badge and its
/// sheet on APP-16. Deleted before the PR.
final class Slop16Shots: XCTestCase {
    @MainActor
    func testBadgeAndSheet() throws {
        continueAfterFailure = false
        let app = launchScreenshotApp()
        signIn(app)
        let title = "Fix flaky scroll restore on the issue list"
        XCTAssertTrue(app.staticTexts[title].waitForExistence(timeout: 120), "board never synced")
        dismissSavePasswordSheet(timeout: 3)
        openIssue(app, title: title)
        let badge = app.buttons["pr-graph-badge"]
        XCTAssertTrue(badge.waitForExistence(timeout: 60), "badge never appeared")
        sleep(3)
        try XCUIScreen.main.screenshot().pngRepresentation.write(to: URL(fileURLWithPath: "/tmp/slop16/ios-r3-badge.png"))
        badge.tap()
        sleep(3)
        try XCUIScreen.main.screenshot().pngRepresentation.write(to: URL(fileURLWithPath: "/tmp/slop16/ios-r3-sheet.png"))
    }
}
