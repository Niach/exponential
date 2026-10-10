import XCTest
import SwiftUI
import ExponentialUICore
@testable import ExponentialUI
#if canImport(UIKit)
import UIKit
#endif

/// VAPP-103: the platform Slider keeps the author's range and step, an
/// Input `type` drives keyboard / autofill / autocorrection, an extension
/// the core refuses reaches the host, unnamed media carry no English label,
/// and the list bench's per-step cost.
@MainActor
final class NativeHardeningTests: XCTestCase {
    // MARK: - Slider

    func testSliderKeepsARangeNarrowerThanOne() {
        let g = SurfaceModel.sliderRange(min: 0, max: 0.5, step: 0.1)
        XCTAssertEqual(g.range, 0...0.5)
        XCTAssertEqual(g.step, 0.1)
    }

    func testSliderWithoutAStepIsContinuous() {
        XCTAssertNil(SurfaceModel.sliderRange(min: 0, max: 1, step: 0).step)
        XCTAssertNil(SurfaceModel.sliderRange(min: 0, max: 1, step: -2).step)
        // ... and the drag snaps nothing.
        XCTAssertEqual(SurfaceModel.snap(0.3337, min: 0, max: 1, step: 0), 0.3337, accuracy: 1e-9)
    }

    func testAStepThatDoesNotDivideTheRangeStaysContinuousAndReachesMax() {
        // 0...10 by 3: no platform step (it would stop at 9), the drag snaps.
        XCTAssertNil(SurfaceModel.sliderRange(min: 0, max: 10, step: 3).step)
        XCTAssertEqual(SurfaceModel.sliderRange(min: 0, max: 10, step: 2.5).step, 2.5)
        XCTAssertEqual(SurfaceModel.snap(10, min: 0, max: 10, step: 3), 10)
        XCTAssertEqual(SurfaceModel.snap(12, min: 0, max: 10, step: 3), 10)
        XCTAssertEqual(SurfaceModel.snap(9.6, min: 0, max: 10, step: 3), 10)
        XCTAssertEqual(SurfaceModel.snap(9.4, min: 0, max: 10, step: 3), 9)
        XCTAssertEqual(SurfaceModel.snap(5, min: 0, max: 10, step: 2.5), 5)
        XCTAssertEqual(SurfaceModel.snap(-1, min: 0, max: 10, step: 3), 0)
    }

    func testOnlyAnEmptyOrInvertedRangeWidens() {
        XCTAssertEqual(SurfaceModel.sliderRange(min: 5, max: 5, step: 1).range, 5...6)
        XCTAssertEqual(SurfaceModel.sliderRange(min: 5, max: 2, step: 1).range, 5...6)
        XCTAssertEqual(SurfaceModel.sliderRange(min: -1, max: -0.5, step: 0).range, -1 ... -0.5)
    }

    // MARK: - Example themes

    /// Every theme file an example or sample host ships loads strictly (a
    /// missing `$schema` or a bad token fails here, not on a device).
    func testEveryExampleAndSampleThemeLoads() throws {
        let pkg = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let repo = pkg.deletingLastPathComponent().deletingLastPathComponent()
        let files = [
            pkg.appendingPathComponent("Example/KitchenSink/Resources/brand.theme.json"),
            repo.appendingPathComponent("samples/exponential-ui/shared/sample.theme.json"),
        ]
        for file in files {
            let json = try String(contentsOf: file, encoding: .utf8)
            XCTAssertNoThrow(try ThemeHandle.load(json: json), file.lastPathComponent)
            var issues: [ThemeHandle.ThemeIssue] = []
            _ = ThemeHandle.loadOrDefault(json: json) { issues = $0 }
            XCTAssertEqual(issues.count, 0, "\(file.lastPathComponent): \(issues)")
        }
        // A file without `$schema` is refused, and loadOrDefault says why.
        var issues: [ThemeHandle.ThemeIssue] = []
        let fallback = ThemeHandle.loadOrDefault(json: #"{"id":"x","name":"X","extends":"neutral"}"#) { issues = $0 }
        XCTAssertFalse(issues.isEmpty)
        XCTAssertNotNil(fallback)
    }

    // MARK: - Input types

    func testInputTypesMapToKeyboardAutofillAndAutocorrection() {
        let email = InputTraits(type: "email", kind: .input)
        XCTAssertEqual(email.keyboard, .email)
        XCTAssertEqual(email.content, .email)
        XCTAssertFalse(email.autocorrect)
        XCTAssertFalse(email.secure)
        let url = InputTraits(type: "url", kind: .input)
        XCTAssertEqual(url.keyboard, .url)
        XCTAssertEqual(url.content, .url)
        XCTAssertFalse(url.autocorrect)
        let tel = InputTraits(type: "tel", kind: .input)
        XCTAssertEqual(tel.keyboard, .phone)
        XCTAssertEqual(tel.content, .phone)
        XCTAssertFalse(tel.autocorrect)
        let password = InputTraits(type: "password", kind: .input)
        XCTAssertTrue(password.secure)
        XCTAssertEqual(password.content, .password)
        XCTAssertFalse(password.autocorrect)
        let text = InputTraits(type: "text", kind: .input)
        XCTAssertEqual(text, InputTraits(type: "", kind: nil))
        XCTAssertTrue(text.autocorrect)
        XCTAssertNil(text.content)
        // The inline fields of a native keep their own keyboards.
        XCTAssertEqual(InputTraits(type: "", kind: .number).keyboard, .number)
        XCTAssertEqual(InputTraits(type: "", kind: .search).keyboard, .search)
    }

    #if canImport(UIKit)
    func testUIKitFieldReadsTheTraits() {
        let field = UITextField()
        OwnedTextField.apply(InputTraits(type: "email", kind: .input), to: field)
        XCTAssertEqual(field.keyboardType, .emailAddress)
        XCTAssertEqual(field.textContentType, .emailAddress)
        XCTAssertEqual(field.autocapitalizationType, .none)
        XCTAssertEqual(field.autocorrectionType, .no)
        OwnedTextField.apply(InputTraits(type: "password", kind: .input), to: field)
        XCTAssertTrue(field.isSecureTextEntry)
        XCTAssertEqual(field.textContentType, .password)
        OwnedTextField.apply(InputTraits(type: "tel", kind: .input), to: field)
        XCTAssertFalse(field.isSecureTextEntry)
        XCTAssertEqual(field.keyboardType, .phonePad)
        XCTAssertEqual(field.textContentType, .telephoneNumber)
        OwnedTextField.apply(InputTraits(type: "url", kind: .input), to: field)
        XCTAssertEqual(field.keyboardType, .URL)
        XCTAssertEqual(field.textContentType, .URL)
        OwnedTextField.apply(InputTraits(type: "text", kind: .input), to: field)
        XCTAssertEqual(field.keyboardType, .default)
        XCTAssertNil(field.textContentType)
        XCTAssertEqual(field.autocorrectionType, .default)
    }
    #endif

    // MARK: - Extensions

    func testAnInvalidExtensionIsRefusedAtRegistration() {
        let registry = ExtensionRegistry.shared
        registry.reset()
        defer { registry.reset() }
        let bad = #"{"id":"https://example.com/bad/v1","extends":"https://ui.exponential.at/catalogs/core/v1","components":{"Button":{"kind":"native","props":{}}}}"#
        XCTAssertThrowsError(try ExponentialUI.register(extension: bad, painters: [:]))
        XCTAssertTrue(registry.definitions.isEmpty, "a refused extension is never registered")
    }

    func testASurfaceReportsAnExtensionTheCoreRefuses() {
        let registry = ExtensionRegistry.shared
        registry.reset()
        defer { registry.reset() }
        // Bypass the checked entry point: the surface itself must not swallow it.
        registry.register(definition: "{ not json")
        XCTAssertThrowsError(try makeModel("ext"))
    }

    // MARK: - Accessibility

    func testUnnamedMediaCarryNoEnglishLabel() throws {
        let m = try makeModel("media", theme: "neutral", mode: .light, width: 400, fixed: true)
        try m.setNested(json: JSONValue.object([
            "id": .string("root"), "component": .string("Stack"),
            "children": .array([
                .object(["id": .string("img"), "component": .string("Image"), "props": .object(["src": .string("x.png")])]),
                .object(["id": .string("av"), "component": .string("Avatar"), "props": .object([:])]),
            ]),
        ]).json)
        func info(_ id: String) -> A11yInfo {
            let i = m.index(of: id)!
            return A11yInfo.of(m.node(i)!, model: m, style: m.style(i))
        }
        XCTAssertTrue(info("img").hidden, "an unnamed Image is decorative")
        XCTAssertNotEqual(info("img").label, "image")
        let av = m.nodes.first { $0.component == "Avatar" && !(A11yInfo.of($0, model: m, style: m.style($0.index)).hidden) }
        if let av {
            let a = A11yInfo.of(av, model: m, style: m.style(av.index))
            XCTAssertNotEqual(a.label, "image")
            XCTAssertTrue(a.traits.contains(.isImage), "the platform speaks the image trait in the user's language")
        }
    }

    // MARK: - The list bench (honest per-step numbers)

    /// `fixtures/bench-list.json` (100,000 rows, 390 × 800, neutral) with
    /// the REAL TextKit measure on the main actor: `scrollStepMs` = the
    /// mean wall time of 100 one-viewport `scrollTo` steps (core + measure +
    /// node read-back); `coreMs` = the core's own share. Printed for the
    /// README; asserted only loosely (machines differ).
    func testListBenchScrollStep() throws {
        guard Fixtures.available() else { throw XCTSkip("no fixtures") }
        let f = try Fixtures.json("bench-list.json")
        let count = Int(f["rows"]!["count"]!.number!)
        let rows = JSONValue.array((0..<count).map { i in
            .object(["id": .string("r\(i)"), "title": .string("Row \(i)"), "meta": .string(String(i % 97))])
        })
        let vw = CGFloat(f["viewport"]!["width"]!.number!)
        let vh = CGFloat(f["viewport"]!["height"]!.number!)
        func run() throws -> [String: Double] {
            var options = SurfaceOptions()
            options.theme = ThemeHandle.builtin(f["theme"]!.string!)
            options.mode = .light
            let m = try SurfaceModel(id: "bench", options: options, host: NoHost())
            let t0 = ContinuousClock.now
            _ = try m.setComponents(json: f["components"]!.json)
            m.setData(path: "/rows", value: rows)
            m.setViewport(width: vw, height: vh)
            let first = ms(ContinuousClock.now - t0)
            let steps = 100
            var coreNs: UInt64 = 0
            var measureCalls = 0
            let t1 = ContinuousClock.now
            for i in 1...steps {
                m.scrollTo(id: "root", offset: CGPoint(x: 0, y: CGFloat(i) * vh))
                coreNs += m.stats.layoutNs
                measureCalls += m.stats.measureCalls
            }
            let step = ms(ContinuousClock.now - t1) / Double(steps)
            let t2 = ContinuousClock.now
            m.scrollToIndex(id: "root", index: 50000, align: "start")
            let jump = ms(ContinuousClock.now - t2)
            return ["firstPaintMs": first, "scrollStepMs": step, "coreMs": Double(coreNs) / 1e6 / Double(steps), "measureCallsPerStep": Double(measureCalls) / Double(steps), "scrollToIndexMs": jump]
        }
        _ = try run() // warm the font and width caches
        let r = try run()
        let line = r.keys.sorted().map { "\($0) \(String(format: "%.2f", r[$0]!))" }.joined(separator: ", ")
        print("bench-list (Swift, TextKit measure, \(ProcessInfo.processInfo.activeProcessorCount) cpus): \(line)")
        XCTAssertLessThan(r["scrollStepMs"]!, 250, "a scroll step stays interactive")
    }

    private func ms(_ d: Duration) -> Double {
        Double(d.components.seconds) * 1000 + Double(d.components.attoseconds) / 1e15
    }
}
