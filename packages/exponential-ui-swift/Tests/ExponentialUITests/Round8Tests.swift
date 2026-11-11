import XCTest
import SwiftUI
import CoreText
import ExponentialUICore
@testable import ExponentialUI

/// Round 8 (packages/exponential-ui/docs/round-8-contract.md): `Canvas`
/// over uniffi (`fixtures/canvas.json`: compile, refuse, fit, pointer), the
/// painter's native `text` ops, the Canvas box / a11y / focus, the Checkbox
/// `indeterminate` cases of `interactions-round1.json`, and the Progress
/// `value` in tabular figures.
@MainActor
final class Round8Tests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    // MARK: - canvas.json

    func testCanvasCompileCasesSampleTheirPackedFramesAndDecode() throws {
        let f = try Fixtures.json("canvas.json")
        let cases = f["compile"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let id = c["id"]!.string!
            let player = try CanvasPlayer(canvasJson: c["canvas"]!.json)
            XCTAssertEqual(player.slotIds(), c["slotIds"]!.array!.compactMap(\.string), "\(id) slotIds")
            let size = c["canvas"]!["size"]!.array!.compactMap(\.number).map(Float.init)
            XCTAssertEqual(player.size(), size, "\(id) size")
            let want = c["packed"]!.array!.map { Float($0.number!) }
            let got = player.sample(slotsJson: c["slots"].map(\.json) ?? "")
            XCTAssertEqual(got, want, "\(id) packed")
            XCTAssertEqual(got.first, 2, "\(id): VERSION 2")
            let list = try XCTUnwrap(MotionList(packed: got), "\(id) decodes")
            let ops = c["list"]!["ops"]!.array!
            XCTAssertEqual(list.ops.count, ops.count, "\(id) op count")
            // The text ops: every field the painter reads, in source order, last.
            let wantTexts = ops.filter { $0["op"]?.string == "text" }
            let gotTexts: [(String, Bool, Int, CGFloat, Int, Int, MotionList.Color4, CGAffineTransform)] = list.ops.compactMap {
                if case let .text(t, text, mono, weight, size, align, baseline, color) = $0 { return (text, mono, weight, size, align, baseline, color, t) }
                return nil
            }
            XCTAssertEqual(gotTexts.count, wantTexts.count, "\(id) text ops")
            if !wantTexts.isEmpty {
                let firstText = list.ops.firstIndex { if case .text = $0 { return true } else { return false } }!
                XCTAssertTrue(list.ops[firstText...].allSatisfy { if case .text = $0 { return true } else { return false } }, "\(id): text after every shape op")
            }
            let weights = ["regular": 400, "medium": 500, "semibold": 600, "bold": 700]
            for (g, w) in zip(gotTexts, wantTexts) {
                XCTAssertEqual(g.0, w["text"]!.string!, id)
                XCTAssertEqual(g.1, w["font"]!.string! == "mono", "\(id) font")
                XCTAssertEqual(g.2, weights[w["weight"]!.string!], "\(id) weight")
                XCTAssertEqual(g.3, CGFloat(w["size"]!.number!), accuracy: 1e-4, "\(id) size")
                XCTAssertEqual(g.4, ["start", "center", "end"].firstIndex(of: w["align"]!.string!), "\(id) align")
                XCTAssertEqual(g.5, ["top", "middle", "alphabetic"].firstIndex(of: w["baseline"]!.string!), "\(id) baseline")
                let rgba = w["color"]!.array!.compactMap(\.number)
                XCTAssertEqual([g.6.r, g.6.g, g.6.b, g.6.a], rgba.map { Double(Float($0)) }, "\(id) color")
                let t = w["transform"]!.array!.compactMap(\.number).map { CGFloat(Float($0)) }
                XCTAssertEqual([g.7.a, g.7.b, g.7.c, g.7.d, g.7.tx, g.7.ty], t, "\(id) transform")
            }
        }
    }

    func testCanvasRefuseCasesThrowTheirIssues() throws {
        let f = try Fixtures.json("canvas.json")
        for c in f["refuse"]?.array ?? [] {
            let id = c["id"]!.string!
            do {
                _ = try CanvasPlayer(canvasJson: c["canvas"]!.json)
                XCTFail("\(id): compiled")
            } catch let UiError.Invalid(reason) {
                let issues = JSONValue.parse(reason).array ?? []
                let want = c["issues"]!.array!
                XCTAssertEqual(issues.map { $0["code"]?.string ?? "" }, want.map { $0["code"]!.string! }, "\(id) codes")
                XCTAssertEqual(issues.map { $0["path"]?.string ?? "" }, want.map { $0["path"]!.string! }, "\(id) paths")
            }
        }
    }

    func testCanvasFitMatricesAreThePaintersFit() throws {
        let f = try Fixtures.json("canvas.json")
        for c in f["fit"]?.array ?? [] {
            let s = c["size"]!.array!.compactMap(\.number), b = c["box"]!.array!.compactMap(\.number)
            let m = MotionPainter.fit(width: s[0], height: s[1], into: CGSize(width: b[0], height: b[1]), mode: c["fit"]!.string!)
            let want = c["matrix"]!.array!.compactMap(\.number)
            for (g, w) in zip([m.a, m.b, m.c, m.d, m.tx, m.ty], want) { XCTAssertEqual(g, w, accuracy: 1e-9, "\(c.json)") }
        }
    }

    func testCanvasPointerMapsThroughCanvasToDrawing() throws {
        let f = try Fixtures.json("canvas.json")
        for c in f["pointer"]?.array ?? [] {
            let s = c["size"]!.array!.compactMap(\.number), b = c["box"]!.array!.compactMap(\.number)
            for p in c["points"]!.array! {
                let at = p["at"]!.array!.compactMap(\.number), want = p["drawing"]!.array!.compactMap(\.number)
                let got = canvasToDrawing(x: at[0], y: at[1], width: s[0], height: s[1], boxWidth: b[0], boxHeight: b[1], fit: c["fit"]!.string!)
                XCTAssertEqual(got.count, 2)
                XCTAssertEqual(got[0], want[0], accuracy: 0.01, "\(p.json)")
                XCTAssertEqual(got[1], want[1], accuracy: 0.01, "\(p.json)")
            }
        }
    }

    /// `upright`: text takes the translation and √|det| of fit × transform
    /// only, whatever rotation or skew its groups carry.
    func testCanvasUprightCasesPlaceTextByTranslationAndScaleOnly() throws {
        let f = try Fixtures.json("canvas.json")
        let cases = f["upright"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let id = c["id"]!.string!
            let player = try CanvasPlayer(canvasJson: c["canvas"]!.json)
            let list = try XCTUnwrap(MotionList(packed: player.sample(slotsJson: "")), id)
            let b = c["box"]!.array!.compactMap(\.number)
            let base = MotionPainter.fit(width: list.width, height: list.height, into: CGSize(width: b[0], height: b[1]), mode: c["fit"]!.string!)
            let got: [(String, CGPoint, CGFloat)] = list.ops.compactMap {
                guard case let .text(t, text, _, _, size, _, _, _) = $0 else { return nil }
                let placed = MotionPainter.textAnchor(transform: t.concatenating(base), size: size)
                return (text, placed.anchor, placed.size)
            }
            let want = c["texts"]!.array!
            XCTAssertEqual(got.count, want.count, "\(id) texts")
            for (g, w) in zip(got, want) {
                let a = w["anchor"]!.array!.compactMap(\.number)
                XCTAssertEqual(g.0, w["text"]!.string!, id)
                XCTAssertEqual(g.1.x, CGFloat(a[0]), accuracy: 1e-3, "\(id) \(g.0) x")
                XCTAssertEqual(g.1.y, CGFloat(a[1]), accuracy: 1e-3, "\(id) \(g.0) y")
                XCTAssertEqual(g.2, CGFloat(w["size"]!.number!), accuracy: 1e-3, "\(id) \(g.0) size")
            }
        }
    }

    // MARK: - The text op

    /// The placement rule: align physical, baseline from CTFont ascent /
    /// descent, the anchor = (0, 0) through fit × transform.
    func testTextOriginFollowsAlignAndBaseline() {
        let m = CGAffineTransform(a: 2, b: 0, c: 0, d: 2, tx: 100, ty: 50)
        let w: CGFloat = 40, asc: CGFloat = 15, desc: CGFloat = 5
        XCTAssertEqual(MotionPainter.textOrigin(transform: m, align: 0, baseline: 2, width: w, ascent: asc, descent: desc), CGPoint(x: 100, y: 50))
        XCTAssertEqual(MotionPainter.textOrigin(transform: m, align: 1, baseline: 0, width: w, ascent: asc, descent: desc), CGPoint(x: 80, y: 65))
        XCTAssertEqual(MotionPainter.textOrigin(transform: m, align: 2, baseline: 1, width: w, ascent: asc, descent: desc), CGPoint(x: 60, y: 55))
        XCTAssertEqual(MotionPainter.uniformScale(m), 2)
    }

    /// A `text-only` frame paints glyph pixels natively (the raster skips
    /// them), in the op's colour, around the anchor.
    func testTextOpsPaintNatively() throws {
        let doc = ##"{"size":[100,40],"shapes":[{"type":"text","text":"HHHH","position":[50,20],"size":20,"weight":"bold","align":"center","baseline":"middle","color":"#ff0000"}]}"##
        let player = try CanvasPlayer(canvasJson: doc)
        let list = try XCTUnwrap(MotionList(packed: player.sample(slotsJson: "")))
        let view = Canvas { ctx, size in MotionPainter.paint(list, in: &ctx, box: size) }.frame(width: 100, height: 40)
        let r = ImageRenderer(content: view)
        r.scale = 1
        r.isOpaque = false
        let img = try XCTUnwrap(r.cgImage)
        var buf = [UInt8](repeating: 0, count: 100 * 40 * 4)
        buf.withUnsafeMutableBytes { p in
            let ctx = CGContext(data: p.baseAddress, width: 100, height: 40, bitsPerComponent: 8, bytesPerRow: 400,
                                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            ctx.draw(img, in: CGRect(x: 0, y: 0, width: 100, height: 40))
        }
        var minX = 100, maxX = 0, minY = 40, maxY = 0, red = 0
        for y in 0..<40 {
            for x in 0..<100 {
                let i = (y * 100 + x) * 4
                guard buf[i + 3] > 128 else { continue }
                if buf[i] > 200, buf[i + 1] < 60 { red += 1 }
                minX = min(minX, x); maxX = max(maxX, x); minY = min(minY, y); maxY = max(maxY, y)
            }
        }
        XCTAssertGreaterThan(red, 50, "red glyph pixels")
        // Centred on x = 50, the caps around y = 20 (middle baseline).
        XCTAssertEqual(Double(minX + maxX) / 2, 50, accuracy: 3)
        XCTAssertLessThan(minY, 20)
        XCTAssertGreaterThan(maxY, 20)
    }

    // MARK: - The Canvas node

    func testCanvasBoxesLikeAMotionNamesItsImageAndTakesNoFocus() throws {
        let m = try makeModel()
        try m.setNested(json: #"{"id":"root","component":"Box","style":{"width":120,"display":"flex","flexDirection":"column","alignItems":"flex-start"},"children":[{"id":"cv","component":"Canvas","props":{"size":[200,100],"label":"Visits","pointer":true,"shapes":[{"type":"rect","position":[50,25],"size":[100,50]},{"type":"fill","color":"$color.primary"}]},"on":{"press":{"event":{"name":"hit"}}}},{"id":"bare","component":"Canvas","props":{"size":[40,40],"shapes":[]}}]}"#)
        let cv = try XCTUnwrap(m.index(of: "cv"))
        XCTAssertEqual(m.frame(cv).size, CGSize(width: 120, height: 60))
        XCTAssertEqual(m.frame(try XCTUnwrap(m.index(of: "bare"))).size, CGSize(width: 40, height: 40))
        let a = A11yInfo.of(m.node(cv)!, model: m, style: m.style(cv))
        XCTAssertEqual(a.label, "Visits")
        XCTAssertTrue(a.traits.contains(.isImage))
        XCTAssertFalse(a.traits.contains(.isButton), "pointer adds no role")
        XCTAssertFalse(m.node(cv)!.isFocusable, "pointer adds no focus stop")
        let bare = try XCTUnwrap(m.index(of: "bare"))
        XCTAssertTrue(A11yInfo.of(m.node(bare)!, model: m, style: m.style(bare)).hidden, "decorative without a label")
        // The theme fills the token slots; dark = `$mode`.
        let source = CanvasSource()
        let list = try XCTUnwrap(source.frame(props: m.node(cv)!.props, theme: m.theme, mode: .dark))
        let fills: [MotionList.Color4] = list.ops.compactMap { if case let .fill(_, _, .solid(c), _) = $0 { return c } else { return nil } }
        let hex = try XCTUnwrap(m.theme?.colorHex("primary", mode: .dark))
        let rgba = try XCTUnwrap(MotionSource.rgba(hex))
        XCTAssertEqual(fills.first.map { [$0.r, $0.g, $0.b] } ?? [], rgba.prefix(3).map { Double(Float($0)) })
    }

    // MARK: - Checkbox indeterminate (interactions-round1.json › checkbox)

    func testCheckboxIndeterminateCasesReplayThroughTheModel() throws {
        let f = try Fixtures.json("interactions-round1.json")["checkbox"]!
        let cases = f["cases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            var surface = f["surface"]!.object!
            var props = surface["props"]!.object!
            if let unbound = c["unbound"]?.object {
                for (k, v) in unbound { props[k] = v }
            }
            surface["props"] = .object(props)
            let host = Round8Host()
            var options = SurfaceOptions()
            options.theme = ThemeHandle.builtin("exponential")
            options.mode = .light
            let m = try SurfaceModel(id: "cb", options: options, host: host)
            m.setViewport(width: 400, height: 800)
            try m.setNested(json: JSONValue.object(surface).json)
            if c["unbound"] == nil, let data = c["data"]?.object {
                for (k, v) in data { m.setData(path: "/\(k)", value: v) }
            }
            XCTAssertEqual(state(m), c["before"]!.string!, "\(name): before")
            for (i, step) in (c["steps"]?.array ?? []).enumerated() {
                m.press(id: "c.box")
                let change = host.actions.last { $0.name == "change" }
                XCTAssertEqual(change?.context["checked"]?.bool, step["change"]?["checked"]?.bool, "\(name) #\(i): change.checked")
                XCTAssertEqual(change?.context["indeterminate"]?.bool, step["change"]?["indeterminate"]?.bool, "\(name) #\(i): change.indeterminate")
                if c["unbound"] == nil {
                    XCTAssertEqual(m.data["checked"]?.bool, step["checked"]?.bool, "\(name) #\(i): /checked")
                    XCTAssertEqual(m.data["mixed"]?.bool, step["mixed"]?.bool, "\(name) #\(i): /mixed")
                } else {
                    let o = try XCTUnwrap(m.node(id: "c"))
                    XCTAssertEqual(o.props["checked"]?.bool, step["checked"]?.bool, "\(name) #\(i): checked prop")
                    XCTAssertEqual(o.props["indeterminate"]?.bool ?? false, step["mixed"]?.bool, "\(name) #\(i): indeterminate prop")
                }
                XCTAssertEqual(state(m), step["state"]!.string!, "\(name) #\(i): state")
            }
        }
    }

    /// The state a painter exposes: the box's a11y value + the glyph it draws.
    private func state(_ m: SurfaceModel) -> String {
        guard let box = m.index(of: "c.box") else { return "?" }
        if m.indeterminate(box) { return "mixed" }
        return m.checked(box) ? "true" : "false"
    }

    func testMixedCheckboxReadsMixedAndDrawsTheDash() throws {
        let m = try makeModel(mode: .light)
        try m.setNested(json: #"{"id":"c","component":"Checkbox","props":{"label":"Select all","checked":true,"indeterminate":true}}"#)
        var values: [String] = []
        for i in m.nodes.indices where !(m.node(i)?.hidden ?? true) {
            let a = A11yInfo.of(m.node(i)!, model: m, style: m.style(i))
            if a.role == "checkbox" { values.append(a.value ?? "") }
        }
        XCTAssertTrue(values.contains { $0.contains("mixed") }, "\(values)")
        XCTAssertFalse(values.contains { $0.contains("on") }, "mixed wins over checked: \(values)")
        XCTAssertEqual(BuiltinIcons.name("Checkbox.check.indeterminate"), "ui-minus")
    }

    // MARK: - Progress value (tabular figures)

    func testProgressValueSetsTabularFigures() throws {
        let m = try makeModel(mode: .light)
        try m.setNested(json: #"{"id":"p","component":"Progress","props":{"value":42,"label":"Upload","valueText":"42 / 120 MB","detail":"About a minute left"}}"#)
        let value = try XCTUnwrap(m.index(of: "p.header.value"), "the value part")
        XCTAssertTrue(m.textStyle(value).tabular, "the value part sets tabular numerals")
        let label = try XCTUnwrap(m.index(of: "p.header.label"))
        XCTAssertFalse(m.textStyle(label).tabular)
        XCTAssertNotNil(m.index(of: "p.detail"), "the detail caption")
    }

    /// The value's box is the width it PAINTS at: the measurer shapes it in
    /// the same tabular face the painter draws (round-8 iOS shot read
    /// "41 MB/s · 12..." and ended short: the core names the leaf's owner
    /// `Progress` to the measurer, the model's node has none).
    func testProgressValueMeasuresAtItsPaintedWidth() throws {
        let m = try makeModel(mode: .light)
        try m.setNested(json: #"{"id":"p","component":"Progress","props":{"value":41,"label":"Download","valueText":"41 MB/s · 12 min"}}"#)
        let value = try XCTUnwrap(m.index(of: "p.header.value"))
        let ts = m.textStyle(value)
        XCTAssertTrue(ts.tabular)
        let painted = TextShaper.width(ts.shown("41 MB/s · 12 min"), ts)
        XCTAssertEqual(m.frame(value).width, painted, accuracy: 0.01, "measured box = painted line")
        let label = try XCTUnwrap(m.index(of: "p.header.label"))
        XCTAssertEqual(m.frame(value).maxX, m.frame(m.index(of: "p")!).maxX, accuracy: 0.01, "flush right")
        XCTAssertLessThan(m.frame(label).maxX, m.frame(value).minX)
        // Meter's `value` is the same counting text (the core names its owner `Meter`).
        let meter = try makeModel(mode: .light)
        try meter.setNested(json: #"{"id":"mm","component":"Meter","props":{"segments":[{"value":40,"tone":"primary","label":"Posts"}],"max":100,"label":"Quota","value":"65%"}}"#)
        let mv = try XCTUnwrap(meter.index(of: "mm.header.value"))
        let mts = meter.textStyle(mv)
        XCTAssertTrue(mts.tabular)
        XCTAssertEqual(meter.frame(mv).width, TextShaper.width("65%", mts), accuracy: 0.01, "Meter value: measured box = painted line")
    }

    /// The style key `fontVariantNumeric: "tabular-nums"` (#EXP-1629): on a
    /// Text, inherited from a Box like CSS (the core's restyle), off on a
    /// plain Text; measured in the tnum face it paints in.
    func testFontVariantNumericTabularNumsMeasuresAndPaintsWithTnum() throws {
        let m = try makeModel(theme: "neutral", mode: .light)
        try m.setNested(json: #"{"id":"row","component":"Box","style":{"flexDirection":"row","alignItems":"flex-start"},"children":[{"id":"age","component":"Text","props":{"text":"10m"},"style":{"fontVariantNumeric":"tabular-nums"}},{"id":"col","component":"Box","style":{"fontVariantNumeric":"tabular-nums"},"children":[{"id":"kid","component":"Text","props":{"text":"3h"}}]},{"id":"plain","component":"Text","props":{"text":"11"}}]}"#)
        let age = try XCTUnwrap(m.index(of: "age"))
        let ts = m.textStyle(age)
        XCTAssertTrue(ts.tabular)
        XCTAssertTrue(m.textStyle(try XCTUnwrap(m.index(of: "kid"))).tabular, "inherited like CSS")
        XCTAssertFalse(m.textStyle(try XCTUnwrap(m.index(of: "plain"))).tabular)
        let features = CTFontCopyFeatureSettings(TextFonts.font(family: ts.fontFamily, weight: ts.fontWeight, size: ts.fontSize, tabular: ts.tabular)) as? [[String: Any]] ?? []
        XCTAssertTrue(features.contains { $0[kCTFontFeatureTypeIdentifierKey as String] as? Int == kNumberSpacingType && $0[kCTFontFeatureSelectorIdentifierKey as String] as? Int == kMonospacedNumbersSelector }, "the painted face shapes tnum: \(features)")
        XCTAssertEqual(m.frame(age).width, TextShaper.width(ts.shown("10m"), ts), accuracy: 0.01, "measured as painted (tnum)")
    }
}

@MainActor
final class Round8Host: HostPlugin {
    var actions: [SurfaceActionEvent] = []
    func onAction(_ event: SurfaceActionEvent) { actions.append(event) }
    func onInput(_ event: SurfaceInputEvent) {}
    func onFunctionCall(_ call: SurfaceFunctionCall) {}
    func openUrl(_ url: String) {}
}
