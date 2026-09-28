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

    /// Real key events through the Mac keyboard path (Simulator > I/O >
    /// Keyboard > Connect Hardware Keyboard). The test only opens the field
    /// and waits; a host shell types with System Events between the markers
    /// `/tmp/vapp4-ios-type-ready` and `/tmp/vapp4-ios-type-done`.
    @MainActor
    func testTypingHardwareKeyboard() throws {
        let ready = "/tmp/vapp4-ios-type-ready"
        let done = "/tmp/vapp4-ios-type-done"
        let files = FileManager.default
        try? files.removeItem(atPath: done)
        try? files.removeItem(atPath: ready)
        let app = launch("kitchen-sink")
        let field = app.textFields["echo-field"]
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        if !field.isHittable { app.swipeUp() }
        field.tap()
        Thread.sleep(forTimeInterval: 0.5)
        XCTAssertTrue(files.createFile(atPath: ready, contents: Data("\(Date())".utf8)), "cannot write the ready marker")
        let deadline = Date().addingTimeInterval(20)
        while !files.fileExists(atPath: done), Date() < deadline {
            Thread.sleep(forTimeInterval: 0.05)
        }
        try? files.removeItem(atPath: ready)
        XCTAssertTrue(files.fileExists(atPath: done), "the host never typed (no done marker in 20 s)")
        Thread.sleep(forTimeInterval: 0.5)
        let value = (field.value as? String) ?? ""
        let host = app.staticTexts["echo-host"].label
        let line = "VAPP hwtyping pass=\(value == Self.typed && host == "host: " + Self.typed) value=\(value) host=\(host)"
        print(line)
        attach(line, name: "hw-typing")
        // Diagnostic only: where the field and the echo settle 2 s later.
        Thread.sleep(forTimeInterval: 1.5)
        let echoHost = app.staticTexts["echo-host"]
        let late = "VAPP hwtyping late value=\((field.value as? String) ?? "") host=\(echoHost.label) \((echoHost.value as? String) ?? "")"
        print(late)
        attach(late, name: "hw-typing-late")
        try? files.removeItem(atPath: done)
        XCTAssertEqual(value, Self.typed, "field value")
        XCTAssertEqual(host, "host: " + Self.typed, "host echo")
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
        for screen in ["kitchen-sink", "kitchen-sink-bench"] {
            let app = launch(screen)
            let caption = app.staticTexts["vapp-bench-caption"]
            XCTAssertTrue(caption.waitForExistence(timeout: 10))
            Thread.sleep(forTimeInterval: 0.6)
            // The caption's value carries the live Surface's build time.
            func reading(_ tag: String) -> String {
                "\(tag) \(caption.label) · \((caption.value as? String) ?? "build ? µs")"
            }
            var readings: [String] = [reading("first")]
            for mode in ["vapp-relayout", "vapp-relayout-warm"] {
                for _ in 0..<5 {
                    // relayout = a fresh Surface (cold, every leaf measured);
                    // warm = same Surface, forced pass (taffy cache hits).
                    let before = caption.label
                    app.buttons[mode].tap()
                    Thread.sleep(forTimeInterval: 0.6)
                    if caption.label == before { Thread.sleep(forTimeInterval: 0.6) }
                    readings.append(reading(mode == "vapp-relayout" ? "cold" : "warm"))
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
