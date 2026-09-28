import XCTest

/// VAPP-4 spike: the taffy kitchen sink. Opened DIRECTLY with
/// `-uiTesting -uiTestingScreen kitchen-sink[-bench]`: pushed onto the
/// signed-in navigator, or shown at the root when nobody is signed in, so
/// these tests need no backend.
final class VappSpikeTests: XCTestCase {

    static let typed = "abcdefghijklmnopqrstuvwxyz0123456789ABCD"

    /// The fixture's leaves in pre-order (LANES.md: a11y order = frame order).
    static let expectedPrefix = [
        "Reddit radar",
        "Kitchen sink · one taffy layout on every client",
        "3",
        "Scan now",
        "Sources",
        "r/selfhosted",
    ]

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    private func launch(_ screen: String, extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments += ["-uiTesting", "-uiTestingScreen", screen] + extra
        addUIInterruptionMonitor(withDescription: "System dialog") { alert in
            for label in ["Allow", "OK", "Don't Allow", "Not Now", "Cancel"] where alert.buttons[label].exists {
                alert.buttons[label].tap()
                return true
            }
            return false
        }
        app.launch()
        XCTAssertTrue(
            app.otherElements["vapp-surface"].waitForExistence(timeout: 60),
            "The kitchen sink never opened — is -uiTestingScreen wired?"
        )
        return app
    }

    // MARK: - Typing

    @MainActor
    func testTypingFortyCharsFast() throws {
        let app = launch("kitchen-sink")
        var results: [String] = []
        for run in 1...3 {
            let field = app.textFields["echo-field"]
            XCTAssertTrue(field.waitForExistence(timeout: 10))
            if !field.isHittable { app.swipeUp() }
            field.tap()
            if run > 1 {
                // Clear the previous run's text.
                let current = (field.value as? String) ?? ""
                field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: current.count))
            }
            let start = Date()
            field.typeText(Self.typed)
            let typing = Date().timeIntervalSince(start)
            Thread.sleep(forTimeInterval: 0.5)
            let value = (field.value as? String) ?? ""
            let host = app.staticTexts["echo-host"].label
            let pass = value == Self.typed && host == "host: " + Self.typed
            let line = "VAPP typing run=\(run) pass=\(pass) typingSeconds=\(String(format: "%.2f", typing)) value=\(value) host=\(host)"
            print(line)
            results.append(line)
            XCTAssertEqual(value, Self.typed, "run \(run): field value")
            XCTAssertEqual(host, "host: " + Self.typed, "run \(run): host echo")
        }
        attach(results.joined(separator: "\n"), name: "typing")
    }

    // MARK: - Accessibility order

    @MainActor
    func testAccessibilityOrder() throws {
        let app = launch("kitchen-sink")
        let surface = app.otherElements["vapp-surface"]
        let labels = surface.descendants(matching: .any).allElementsBoundByIndex
            .map(\.label)
            .filter { !$0.isEmpty }
        // Collapse consecutive duplicates (a Button + its Text child).
        var ordered: [String] = []
        for label in labels where ordered.last != label { ordered.append(label) }
        let json = (try? JSONSerialization.data(withJSONObject: ordered, options: []))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
        print("VAPP a11y order \(json)")
        attach(json, name: "a11y-order")
        // XCTest's snapshot ignores accessibilityHidden/SortPriority, so the
        // assertion runs on the app's own VoiceOver-style walk.
        let dumpButton = app.buttons["vapp-a11y-dump"]
        dumpButton.tap()
        Thread.sleep(forTimeInterval: 0.5)
        let dump = (dumpButton.value as? String) ?? "[]"
        print("VAPP a11y walk \(dump)")
        attach(dump, name: "a11y-walk")
        let walked = (try? JSONSerialization.jsonObject(with: Data(dump.utf8))) as? [String] ?? []
        guard let first = walked.firstIndex(of: Self.expectedPrefix[0]) else {
            return XCTFail("The header title never appeared in the walk: \(dump)")
        }
        XCTAssertEqual(Array(walked[first...].prefix(Self.expectedPrefix.count)), Self.expectedPrefix)
    }

    // MARK: - Bench

    @MainActor
    func testBenchTiming() throws {
        for screen in ["kitchen-sink-bench", "kitchen-sink"] {
            let app = launch(screen)
            let caption = app.staticTexts["vapp-bench-caption"]
            XCTAssertTrue(caption.waitForExistence(timeout: 10))
            var readings: [String] = ["first " + caption.label]
            for mode in ["vapp-relayout", "vapp-relayout-warm"] {
                for _ in 0..<5 {
                    // relayout = a fresh Surface (cold, every leaf measured);
                    // warm = same Surface, forced pass (taffy cache hits).
                    app.buttons[mode].tap()
                    Thread.sleep(forTimeInterval: 0.3)
                    readings.append((mode == "vapp-relayout" ? "cold " : "warm ") + caption.label)
                }
            }
            for reading in readings { print("VAPP bench \(screen) \(reading)") }
            attach(readings.joined(separator: "\n"), name: "bench-\(screen)")
            app.terminate()
        }
    }

    // MARK: - Styleguide hook

    /// The styleguide walk reaches the screen through the invisible
    /// `open-vapp-kitchen-sink` hook on the signed-in navigator. Needs the
    /// seeded screenshot backend; skipped when it is unreachable.
    @MainActor
    func testStyleguideHookOpensKitchenSink() throws {
        let app = XCUIApplication()
        app.launchArguments += ["-uiTesting"]
        app.launch()
        // Same sign-in as the capture suites (seeded demo backend,
        // SNAPSHOT_INSTANCE_URL or http://localhost:5173).
        signIn(app)
        // Let the post-login navigator settle (an account activation
        // recreates MainNavigator and would drop a path pushed too early).
        _ = app.buttons["tab-issues"].waitForExistence(timeout: 60)
        Thread.sleep(forTimeInterval: 3)
        let hook = app.buttons["open-vapp-kitchen-sink"]
        guard hook.waitForExistence(timeout: 60) else {
            throw XCTSkip("Could not reach the signed-in navigator (backend down or unseeded?)")
        }
        hook.tap()
        XCTAssertTrue(app.otherElements["vapp-surface"].waitForExistence(timeout: 20))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.buttons["tab-issues"].waitForExistence(timeout: 20), "Back did not return to the app")
    }

    private func attach(_ text: String, name: String) {
        let attachment = XCTAttachment(string: text)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
