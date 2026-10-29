import XCTest
import ExponentialUICore
import ExponentialUIPrimitives
import SwiftUI
@testable import ExponentialUI
#if canImport(AppKit)
import AppKit
#endif

/// VAPP-162 (docs/round-5-contract.md, the SwiftUI checklist): the QR
/// matrix and the bars meter through the facade, QrCode / QrScanner paint
/// and the host camera hook, the Segmented rail + count badge, Button hold
/// (`interactions-round1.json buttonHold` through the model, the keys,
/// focus loss) and the serif family.
@MainActor
final class Round5Tests: XCTestCase {
    // MARK: - QrCode

    /// `fixtures/qr-code.json` bit for bit through `QrMatrix` (the facade's
    /// `qrMatrixJson`); a value with no matrix is nil.
    func testQrMatrixReplaysTheFixture() throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        let cases = try Fixtures.json("qr-code.json")["cases"]?.array ?? []
        XCTAssertEqual(cases.count, 13)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let got = QrMatrix.of(c["value"]?.string ?? "", level: c["level"]?.string ?? "")
            guard let e = c["expected"], !e.isNull else {
                XCTAssertNil(got, name)
                continue
            }
            let m = try XCTUnwrap(got, name)
            XCTAssertEqual(m.size, Int(e["size"]?.number ?? -1), name)
            XCTAssertEqual(m.version, Int(e["version"]?.number ?? -1), name)
            XCTAssertEqual(m.mask, Int(e["mask"]?.number ?? -1), name)
            XCTAssertEqual(m.rows, e["rows"]?.array?.compactMap(\.string), name)
        }
    }

    /// The plate `size` square plus the label `labelGap` below; the lighter
    /// recipe colour on the plate in BOTH modes; module edges on device
    /// pixels inside the 4-module quiet zone; the img named by the label.
    func testQrCodeMeasuresSwapsThePlateAndSnapsModules() throws {
        for mode in [Mode.light, .dark] {
            let m = try makeModel(mode: mode)
            try m.setNested(json: #"{"id":"col","component":"Stack","props":{"direction":"vertical","align":"start"},"children":[{"id":"q","component":"QrCode","props":{"value":"https://exponential.at","label":"Scan to pair","size":160}},{"id":"bare","component":"QrCode","props":{"value":"HELLO","size":96}}]}"#)
            let q = try XCTUnwrap(m.index(of: "q"))
            let f = m.frame(q)
            XCTAssertEqual(f.height, 160 + 8 + 16, accuracy: 0.5, "\(mode)")
            XCTAssertGreaterThanOrEqual(f.width, 160)
            XCTAssertEqual(m.frame(try XCTUnwrap(m.index(of: "bare"))).size, CGSize(width: 96, height: 96))
            let cx = LeafContext(model: m, node: m.node(q)!, size: f.size, style: m.style(q), ink: m.ink(q), textStyle: m.textStyle(q))
            let (plate, module) = QrCodeLeaf.colours(plate: cx.part("QrCode", "plate").style.background ?? .white, module: cx.part("QrCode", "module").color ?? .black)
            XCTAssertGreaterThan(plate.relativeLuminance, module.relativeLuminance, "\(mode): dark modules on a light plate")
            let a = A11yInfo.of(m.node(q)!, model: m, style: m.style(q))
            XCTAssertEqual(a.label, "Scan to pair")
            XCTAssertTrue(a.traits.contains(.isImage))
            XCTAssertTrue(A11yInfo.of(m.node(try XCTUnwrap(m.index(of: "bare")))!, model: m, style: m.style(try XCTUnwrap(m.index(of: "bare")))).hidden)
        }
        let matrix = try XCTUnwrap(QrMatrix.of("HELLO", level: "L"))
        let path = try XCTUnwrap(QrCodeView.modules(rows: matrix.rows, size: 100, quietZone: 4, scale: 3))
        let bounds = path.boundingRect
        // 21 modules + 8 quiet = 29 → 3.448… pt; the finder patterns touch
        // the matrix corners, so the dark bounds = the quiet zone inset.
        let module = 100.0 / 29
        XCTAssertEqual(bounds.minX, (4 * module * 3).rounded() / 3, accuracy: 1e-4)
        XCTAssertEqual(bounds.maxX, (25 * module * 3).rounded() / 3, accuracy: 1e-4)
        for edge in [bounds.minX, bounds.maxX, bounds.minY, bounds.maxY] {
            XCTAssertEqual((edge * 3).rounded(), edge * 3, accuracy: 1e-4, "module edges sit on device pixels")
        }
        XCTAssertNil(QrCodeView.modules(rows: nil, size: 100, quietZone: 4, scale: 2))
        XCTAssertNil(QrMatrix.of("", level: "M"))
        #if canImport(AppKit)
        let m = try makeModel(mode: .dark)
        try m.setNested(json: #"{"id":"q","component":"QrCode","props":{"value":"HELLO","size":120}}"#)
        let host = NSHostingView(rootView: ExponentialSurface(model: m).frame(width: 390, height: 200, alignment: .topLeading))
        host.frame = CGRect(x: 0, y: 0, width: 390, height: 200)
        host.layoutSubtreeIfNeeded()
        let rep = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        if let dir = ProcessInfo.processInfo.environment["R5_DUMP"] { try rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: dir).appendingPathComponent("qr.png")) }
        let k = CGFloat(rep.pixelsWide) / 390
        // The plate is centred in the leaf's box.
        let x0 = m.frame(try XCTUnwrap(m.index(of: "q"))).midX - 60
        func lum(_ x: CGFloat, _ y: CGFloat) -> CGFloat {
            let c = rep.colorAt(x: Int((x * k).rounded()), y: Int((y * k).rounded()))?.usingColorSpace(.sRGB)
            return (c?.redComponent ?? 0) + (c?.greenComponent ?? 0) + (c?.blueComponent ?? 0)
        }
        // The quiet zone is plate (light) even in dark mode; the finder's
        // top-left module dark.
        let mod = 120.0 / 29
        XCTAssertGreaterThan(lum(x0 + mod * 2, mod * 2), 2.4)
        XCTAssertLessThan(lum(x0 + mod * 4.5, mod * 4.5), 0.6)
        #endif
    }

    // MARK: - QrScanner

    /// The viewfinder square (≤ 280) + the hint; a decoded value fires
    /// `scanned {value}` once per distinct value, announced politely;
    /// disabled reports nothing.
    func testQrScannerMeasuresAndDispatchesScannedOncePerValue() async throws {
        let host = Round1Host()
        let m = try makeModel(mode: .dark)
        m.host = host
        try m.setNested(json: #"{"id":"col","component":"Stack","props":{"direction":"vertical"},"children":[{"id":"s","component":"QrScanner","props":{"hint":"Point at the code"},"on":{"scanned":{"event":{"name":"scanned"}}}}]}"#)
        let s = try XCTUnwrap(m.index(of: "s"))
        let f = m.frame(s)
        XCTAssertEqual(f.width, 390, accuracy: 0.5)
        XCTAssertGreaterThan(f.height, 280 + 12)
        XCTAssertLessThan(f.height, 280 + 12 + 40)
        XCTAssertEqual(QrScannerGeometry.viewfinder(width: 200), 200)
        XCTAssertEqual(QrScannerGeometry.viewfinder(width: 390), 280)
        XCTAssertEqual(QrScannerLeaf.formats([:]), ["qr"])
        #if canImport(AppKit)
        var started: [[String]] = []
        var report: ((String) -> Void)?
        var stopped = false
        let scanner = ExponentialQrScanner { formats in
            started.append(formats)
            let (feed, r) = QrScannerFeed.reporting(Color.green, onStop: { stopped = true })
            report = r
            return feed
        }
        let view = NSHostingView(rootView: ExponentialSurface(model: m).exponentialQrScanner(scanner).frame(width: 390, height: 400))
        view.frame = CGRect(x: 0, y: 0, width: 390, height: 400)
        view.layoutSubtreeIfNeeded()
        try await eventually { report != nil }
        XCTAssertEqual(started, [["qr"]])
        report?("pair:abc")
        report?("pair:abc")
        report?("pair:xyz")
        try await eventually { host.actions.count >= 2 }
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(host.actions.map(\.name), ["scanned", "scanned"])
        XCTAssertEqual(host.actions.map { $0.payload?["value"]?.string ?? $0.context["value"]?.string }, ["pair:abc", "pair:xyz"])
        XCTAssertTrue(host.announced.contains { $0 == ("pair:abc", "polite") })
        _ = stopped
        #endif
        // A disabled scanner reports nothing.
        try m.setNested(json: #"{"id":"s","component":"QrScanner","props":{"disabled":true},"on":{"scanned":{"event":{"name":"scanned"}}}}"#)
        let before = host.actions.count
        m.scanned(try XCTUnwrap(m.index(of: "s")), value: "late")
        XCTAssertEqual(host.actions.count, before)
    }

    // MARK: - VoiceWave bars

    /// `fixtures/voice-wave.json` `bars` through `VoiceWaveBars` (the
    /// facade's `voiceWaveBarsJson`): every rect within 0.01.
    func testVoiceWaveBarsReplayTheFixture() throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        let cases = try Fixtures.json("voice-wave.json")["bars"]?.array ?? []
        XCTAssertEqual(cases.count, 8)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let i = c["input"]!
            let e = c["expected"]!
            let got = try XCTUnwrap(VoiceWaveBars.compute(size: i["size"]?.string ?? "", activity: VoiceWaveFrame.activity(i["activity"]?.string ?? ""), level: i["level"]?.number ?? 0, bands: i["bands"]?.array?.compactMap(\.number), elapsed: i["elapsed"]?.number ?? 0, reducedMotion: i["reducedMotion"]?.bool ?? false), name)
            XCTAssertEqual(Double(got.width), e["width"]?.number ?? -1, name)
            XCTAssertEqual(Double(got.height), e["height"]?.number ?? -1, name)
            XCTAssertEqual(got.phase, e["phase"]?.number ?? -1, accuracy: 1e-9, name)
            let want = e["bars"]?.array ?? []
            XCTAssertEqual(got.bars.count, want.count, name)
            for (k, (g, w)) in zip(got.bars, want).enumerated() {
                let wr = [w["x"], w["y"], w["width"], w["height"], w["radius"]].map { $0?.number ?? .nan }
                let gr = [g.rect.minX, g.rect.minY, g.rect.width, g.rect.height, g.radius].map(Double.init)
                XCTAssertEqual(Round2Suites.near(gr, wr, 0.01), nil, "\(name) bar \(k)")
            }
        }
    }

    /// The root recipe's meter box per size; a child stays in the tree,
    /// never painted; the img name keeps the activity.
    func testVoiceWaveBarsSizeTheNodeAndHideTheChild() throws {
        let m = try makeModel()
        try m.setNested(json: #"{"id":"col","component":"Stack","props":{"direction":"vertical","align":"start"},"children":[{"id":"b","component":"VoiceWave","props":{"variant":"bars","size":"lg","activity":"speaking","level":0.5,"label":"Level"}},{"id":"s","component":"VoiceWave","props":{"variant":"bars","size":"sm"},"children":[{"id":"c","component":"Icon","props":{"name":"mic"}}]}]}"#)
        XCTAssertEqual(m.frame(try XCTUnwrap(m.index(of: "b"))).size, CGSize(width: 156, height: 40))
        XCTAssertEqual(m.frame(try XCTUnwrap(m.index(of: "s"))).size, CGSize(width: 78, height: 16))
        XCTAssertEqual(SurfaceMeasurer.voiceWaveBarsBox("xl"), CGSize(width: 195, height: 56))
        XCTAssertEqual(SurfaceMeasurer.voiceWaveBarsBox("nope"), CGSize(width: 117, height: 24))
        let b = try XCTUnwrap(m.index(of: "b"))
        XCTAssertEqual(A11yInfo.of(m.node(b)!, model: m, style: m.style(b)).label, "Level, speaking")
        let cx = LeafContext(model: m, node: m.node(b)!, size: m.frame(b).size, style: m.style(b), ink: m.ink(b), textStyle: m.textStyle(b))
        XCTAssertNotNil(cx.part("VoiceWave", "bar").color)
        #if canImport(AppKit)
        let host = NSHostingView(rootView: ExponentialSurface(model: m).frame(width: 390, height: 200).environment(\.xuiReducedMotion, true))
        host.frame = CGRect(x: 0, y: 0, width: 390, height: 200)
        host.layoutSubtreeIfNeeded()
        XCTAssertNotNil(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        #endif
    }

    // MARK: - Segmented rail + badge

    /// A vertical `bar` = 72 × (items × 56); a vertical `segmented` stacks
    /// its items; the badge text is the core's (`99+`, none at 0).
    func testSegmentedRailLaysOutAndBadges() throws {
        let m = try makeModel()
        try m.setNested(json: #"{"id":"row","component":"Stack","props":{"direction":"horizontal","align":"start"},"children":[{"id":"rail","component":"Segmented","props":{"variant":"bar","orientation":"vertical","value":"chats","items":[{"label":"Chats","value":"chats","icon":"message-circle","count":3},{"label":"People","value":"people","icon":"users"},{"label":"Settings","value":"settings","icon":"settings","count":120}]}},{"id":"col","component":"Segmented","props":{"orientation":"vertical","value":"a","items":[{"label":"Alpha","value":"a"},{"label":"Beta","value":"b"}]}}]}"#)
        let rail = m.frame(try XCTUnwrap(m.index(of: "rail")))
        XCTAssertEqual(rail.width, 72, accuracy: 0.5)
        XCTAssertEqual(rail.height, 3 * 56, accuracy: 0.5)
        let col = m.frame(try XCTUnwrap(m.index(of: "col")))
        XCTAssertGreaterThan(col.height, 40, "two stacked segments")
        XCTAssertLessThan(col.width, 200)
        XCTAssertEqual(SegmentedLeaf.badge(.number(3)), "3")
        XCTAssertEqual(SegmentedLeaf.badge(.number(120)), "99+")
        XCTAssertNil(SegmentedLeaf.badge(.number(0)))
        XCTAssertNil(SegmentedLeaf.badge(nil))
        // ArrowDown moves the rail's roving focus.
        m.requestFocus("rail", keyboard: true)
        XCTAssertTrue(m.handleKey(key: "down"))
        #if canImport(AppKit)
        let host = NSHostingView(rootView: ExponentialSurface(model: m).frame(width: 390, height: 300))
        host.frame = CGRect(x: 0, y: 0, width: 390, height: 300)
        host.layoutSubtreeIfNeeded()
        XCTAssertNotNil(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        #endif
    }

    // MARK: - Button hold

    /// `interactions-round1.json` `buttonHold` through the model: a 120 × 40
    /// Button, each step's pointer / key input, then the events it fired,
    /// `pressed` and the painted label. `hold: false` = the plain press.
    func testButtonHoldReplaysTheFixture() throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        let cases = try Fixtures.json("interactions-round1.json")["buttonHold"]?.array ?? []
        XCTAssertEqual(cases.count, 10)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            var props: [String: JSONValue] = ["label": .string("Hold to talk"), "holdingLabel": .string("Holding…"), "hold": .bool(true)]
            for (k, v) in c["props"]?.object ?? [:] {
                if v.isNull { props[k] = nil } else { props[k] = v }
            }
            let plain = props["hold"]?.bool != true
            let on: JSONValue = .object(["press", "holdStart", "holdEnd", "holdCancel"].reduce(into: [:]) { $0[$1] = .object(["event": .object(["name": .string($1)])]) })
            let host = Round1Host()
            let m = try makeModel(fixed: true)
            m.host = host
            let tree: JSONValue = .object(["id": .string("root"), "component": .string("Box"), "children": .array([.object(["id": .string("b"), "component": .string("Button"), "props": .object(props), "on": on, "style": .object(["width": .number(120), "height": .number(40)])])])])
            try m.setNested(json: tree.json)
            let i = try XCTUnwrap(m.index(of: "b"), name)
            XCTAssertEqual(m.frame(i).size, CGSize(width: 120, height: 40), name)
            XCTAssertEqual(m.isHoldButton(m.node(i)!), !plain, name)
            var hostPressed = false
            for (k, step) in (c["steps"]?.array ?? []).enumerated() {
                let before = host.actions.count
                let x = CGFloat(step["x"]?.number ?? 0), y = CGFloat(step["y"]?.number ?? 0)
                let op = step["do"]?.string ?? ""
                if plain {
                    switch op {
                    case "pointerdown":
                        m.pressDown("b")
                        hostPressed = true
                    case "pointerup":
                        m.pressUp("b")
                        hostPressed = false
                        if (0...120).contains(x), (0...40).contains(y) { m.press(i) }
                    default: break
                    }
                } else {
                    switch op {
                    case "pointerdown", "pointermove", "pointerup": m.holdPointer(i, op, at: CGPoint(x: x, y: y))
                    case "pointercancel": m.holdPointerCancel(i)
                    default:
                        var input = step.object ?? [:]
                        for drop in ["events", "pressed", "label"] { input[drop] = nil }
                        m.hold(i, .object(input))
                    }
                }
                let fired = host.actions[before...].map(\.name)
                let expected = step["events"]?.array?.compactMap(\.string) ?? []
                XCTAssertEqual(fired, expected, "\(name) step \(k): events")
                let node = try XCTUnwrap(m.node(id: "b"))
                XCTAssertEqual(hostPressed || node.states.contains("pressed"), step["pressed"]?.bool ?? false, "\(name) step \(k): pressed")
                XCTAssertEqual(node.props.str("label"), step["label"]?.string, "\(name) step \(k): label")
                if !plain { XCTAssertEqual(m.isHeld(id: "b"), step["pressed"]?.bool ?? false, "\(name) step \(k): isHeld") }
            }
        }
    }

    /// The hardware keys of a FOCUSED hold Button (Space / Enter down → up,
    /// repeat ignored, Escape cancels only while held), focus loss cancels,
    /// VoiceOver's activate toggles the hold.
    func testHoldKeysFocusLossAndVoiceOverThroughTheModel() throws {
        let host = Round1Host()
        let m = try makeModel(fixed: true)
        m.host = host
        let on = #"{"press":{"event":{"name":"press"}},"holdStart":{"event":{"name":"holdStart"}},"holdEnd":{"event":{"name":"holdEnd"}},"holdCancel":{"event":{"name":"holdCancel"}}}"#
        try m.setNested(json: #"{"id":"root","component":"Stack","children":[{"id":"b","component":"Button","props":{"label":"Talk","hold":true,"holdingLabel":"Talking"},"on":\#(on)},{"id":"other","component":"Button","props":{"label":"Other"},"on":{"press":{"event":{"name":"press"}}}}]}"#)
        let b = try XCTUnwrap(m.index(of: "b"))
        XCTAssertFalse(m.holdKey("space", up: false), "nothing focused")
        m.requestFocus("b", keyboard: true)
        XCTAssertTrue(m.holdKey("space", up: false))
        XCTAssertTrue(m.holdKey("space", up: false, repeat: true))
        XCTAssertEqual(m.node(id: "b")?.props.str("label"), "Talking")
        XCTAssertTrue(m.holdKey("space", up: true))
        XCTAssertEqual(host.actions.map(\.name), ["holdStart", "holdEnd"])
        XCTAssertFalse(m.holdKey("escape", up: false), "Escape passes through when not held")
        XCTAssertTrue(m.holdKey("enter", up: false))
        XCTAssertTrue(m.holdKey("escape", up: false))
        XCTAssertTrue(m.holdKey("enter", up: true))
        XCTAssertEqual(host.actions.map(\.name), ["holdStart", "holdEnd", "holdStart", "holdCancel"])
        XCTAssertFalse(m.holdKey("a", up: false))
        // Focus moving away cancels a held hold.
        XCTAssertTrue(m.holdKey("space", up: false))
        m.requestFocus("other", keyboard: true)
        XCTAssertEqual(host.actions.suffix(2).map(\.name), ["holdStart", "holdCancel"])
        XCTAssertFalse(m.isHeld(id: "b"))
        // A pointer press never fires `press`.
        m.press(b)
        XCTAssertFalse(host.actions.contains { $0.name == "press" })
        // VoiceOver: activate starts, activate again ends.
        m.holdToggle(b)
        XCTAssertTrue(m.isHeld(id: "b"))
        m.holdToggle(b)
        XCTAssertFalse(m.isHeld(id: "b"))
        XCTAssertEqual(host.actions.suffix(2).map(\.name), ["holdStart", "holdEnd"])
    }

    // MARK: - serif

    /// `ui-serif` (and a stack starting with it) = the system serif, not the sans.
    func testUiSerifResolvesToTheSystemSerif() throws {
        let serif = TextFonts.font(family: "ui-serif", weight: 400, size: 17)
        let stack = TextFonts.font(family: "ui-serif, Georgia, serif", weight: 400, size: 17)
        let sans = TextFonts.font(family: nil, weight: 400, size: 17)
        let name = CTFontCopyPostScriptName(serif) as String
        XCTAssertNotEqual(name, CTFontCopyPostScriptName(sans) as String)
        XCTAssertTrue(name.lowercased().contains("newyork") || name.lowercased().contains("serif") || name.lowercased().contains("times"), name)
        XCTAssertEqual(CTFontCopyPostScriptName(stack) as String, name)
        XCTAssertEqual(TextFonts.resolve("ui-serif").serif, true)
        XCTAssertEqual(TextFonts.resolve("serif").serif, true)
        XCTAssertEqual(TextFonts.resolve("ui-monospace").serif, false)
        let bold = TextFonts.font(family: "ui-serif", weight: 700, size: 17)
        XCTAssertNotEqual(CTFontCopyPostScriptName(bold) as String, name)
    }
}
