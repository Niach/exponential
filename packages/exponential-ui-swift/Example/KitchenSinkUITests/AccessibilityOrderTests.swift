import XCTest

/// The VoiceOver order of the kitchen sink. XCUITest's own element snapshot
/// ignores `accessibilitySortPriority` (the VAPP-4 finding), so the app
/// walks its UIAccessibility tree itself (`-a11yDump <path>`, the walk
/// VoiceOver consumes; SwiftUI materialises it only under an assistive
/// client, which a UI test is) and this test reads the file.
final class AccessibilityOrderTests: XCTestCase {
    func testKitchenSinkReadsInPreOrder() throws {
        let path = NSTemporaryDirectory() + "kitchen-sink-a11y.txt"
        try? FileManager.default.removeItem(atPath: path)
        let app = XCUIApplication()
        app.launchArguments = ["-shot", "exponential-ui-kitchen-sink", "-a11yDump", path]
        app.launch()
        let deadline = Date().addingTimeInterval(20)
        while !FileManager.default.fileExists(atPath: path), Date() < deadline { usleep(200_000) }
        let text = try String(contentsOfFile: path, encoding: .utf8)
        let labels = text.components(separatedBy: "\n---\n").first!.split(separator: "\n").map(String.init)
        print("[a11y-walk] \(labels.joined(separator: " | "))")
        let expectedPrefix = ["Alex Chen", "Reddit radar", "Kitchen sink · one catalog on every client", "3", "Scan now", "Sources"]
        XCTAssertEqual(Array(labels.prefix(expectedPrefix.count)), expectedPrefix, "walk: \(labels.prefix(20))")
        func position(_ label: String) -> Int { labels.firstIndex(where: { $0.hasPrefix(label) }) ?? Int.max }
        XCTAssertLessThan(position("r/selfhosted"), position("r/opensource"))
        XCTAssertLessThan(position("Auto-scan"), position("Drafts"))
        XCTAssertLessThan(position("Scanning"), position("Quota"))
        XCTAssertLessThan(position("All"), position("Archived"))
        XCTAssertLessThan(position("Title"), position("Posting guidelines"))
        XCTAssertGreaterThan(labels.count, 60)
        // Attach the walk for the report.
        let attachment = XCTAttachment(string: text)
        attachment.name = "a11y-walk"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}

/// The typing test: 40 characters into the host-owned Title field with the
/// example host's 150 ms echo; every character lands, in order, and the
/// echo line shows the same text. XCUITest paces its keys (the VAPP-4 burst
/// finding is covered by the model-level test in `swift test`); this proves
/// the UIKit field → model → host → echo path end to end on the device.
final class TypingTests: XCTestCase {
    func testFortyCharactersLandInOrder() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-shot", "exponential-ui-kitchen-sink"]
        app.launch()
        let field = app.textFields["Title"].firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.tap()
        let text = "The quick brown fox jumps over the lazy dog"
        field.typeText(String(text.prefix(40)))
        let echo = app.staticTexts["host-echo"]
        let expected = "host: \(text.prefix(40))"
        let deadline = Date().addingTimeInterval(5)
        while echo.label != expected, Date() < deadline { usleep(100_000) }
        XCTAssertEqual(field.value as? String, String(text.prefix(40)))
        XCTAssertEqual(echo.label, expected)
    }
}
