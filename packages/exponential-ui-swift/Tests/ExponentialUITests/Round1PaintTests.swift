import XCTest
import SwiftUI
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-100 (contract §2, §4, §5, §6): the round-1 paint keys parsed
/// from the core's resolved visuals, motion, rtl glyphs, the layer stack
/// and the accessibility mapping.
@MainActor
final class Round1PaintTests: XCTestCase {
    private func surface(_ tree: JSONValue, width: CGFloat = 400) throws -> SurfaceModel {
        let m = try makeModel("paint", theme: "neutral", mode: .light, width: width, fixed: true)
        try m.setNested(json: tree.json)
        return m
    }

    private func node(_ id: String, _ component: String, props: JSONValue = .object([:]), style: JSONValue? = nil, children: [JSONValue]? = nil) -> JSONValue {
        var o: [String: JSONValue] = ["id": .string(id), "component": .string(component), "props": props]
        if let style { o["style"] = style }
        if let children { o["children"] = .array(children) }
        return .object(o)
    }

    private func root(_ kids: [JSONValue]) -> JSONValue {
        node("root", "Stack", props: .object(["gap": .string("sm")]), children: kids)
    }

    // MARK: - §2 keys

    func testBordersRadiiAndGradientsComeFromTheCore() throws {
        let m = try surface(root([
            node("sides", "Box", style: .object([
                "height": .number(40), "borderColor": .string("#ff0000"), "borderWidth": .number(1), "borderBottomWidth": .number(3), "borderStyle": .string("dashed"),
                "borderRadius": .number(4), "borderTopLeftRadius": .number(12),
            ])),
            node("grad", "Box", style: .object([
                "height": .number(40), "backgroundColor": .string("#000000"),
                "backgroundGradient": .object(["angle": .number(90), "stops": .array([.object(["color": .string("#ffffff"), "offset": .number(0)]), .object(["color": .string("#00000000"), "offset": .number(0.5)]), .object(["color": .string("#0000ff"), "offset": .number(1)])])]),
            ])),
        ]))
        let s = m.style(m.index(of: "sides")!)
        XCTAssertEqual(s.borderWidths, [1, 1, 3, 1])
        XCTAssertEqual(s.borderWidth, 3)
        XCTAssertFalse(s.uniformBorder)
        XCTAssertEqual(s.borderStyle, "dashed")
        XCTAssertEqual(s.radii, [12, 4, 4, 4])
        XCTAssertEqual(s.radius, 12)
        XCTAssertEqual(s.clampedRadii(CGSize(width: 100, height: 10)), [5, 4, 4, 4], "a corner never exceeds half the short side")
        XCTAssertEqual(s.insets.bottom, 3, "a leaf's content inset counts the side's border")
        let g = m.style(m.index(of: "grad")!)
        XCTAssertNotNil(g.background, "the gradient paints OVER the colour")
        XCTAssertEqual(g.gradient?.stops.count, 3)
        XCTAssertEqual(g.gradient?.angle, 90)
        XCTAssertEqual(g.gradient?.stops.map(\.location), [0, 0.5, 1])
    }

    func testGradientAnglesMapToCSSLines() {
        func pts(_ a: CGFloat, _ size: CGSize = CGSize(width: 100, height: 100)) -> (UnitPoint, UnitPoint) {
            let p = GradientPaint(angle: a, stops: []).points(in: size)
            return (p.start, p.end)
        }
        func near(_ a: UnitPoint, _ x: CGFloat, _ y: CGFloat, file: StaticString = #filePath, line: UInt = #line) {
            XCTAssertEqual(a.x, x, accuracy: 1e-6, file: file, line: line)
            XCTAssertEqual(a.y, y, accuracy: 1e-6, file: file, line: line)
        }
        // 0 = to top, 90 = to right, 180 = to bottom (CSS).
        near(pts(0).0, 0.5, 1); near(pts(0).1, 0.5, 0)
        near(pts(90).0, 0, 0.5); near(pts(90).1, 1, 0.5)
        near(pts(180).0, 0.5, 0); near(pts(180).1, 0.5, 1)
        // 45° on a square: corner to corner.
        near(pts(45).0, 0, 1); near(pts(45).1, 1, 0)
        // 45° on a wide box: the line reaches the corners' perpendiculars
        // (CSS length |w sin a| + |h cos a|), so it overshoots the box.
        let wide = pts(45, CGSize(width: 200, height: 100))
        XCTAssertLessThan(wide.0.y, 1.5)
        XCTAssertGreaterThan(wide.0.y, 1)
    }

    func testTransformVisibilityPointerAndCursorParse() throws {
        let m = try surface(root([
            node("moved", "Box", style: .object(["height": .number(20), "transform": .string("translate(10px, 4px) scale(2) rotate(90deg)")])),
            node("ghost", "Box", style: .object(["height": .number(20), "visibility": .string("hidden"), "pointerEvents": .string("none"), "cursor": .string("pointer"), "userSelect": .string("none")])),
            node("clipped", "Box", style: .object(["height": .number(20), "overflowX": .string("hidden"), "overflowY": .string("auto")])),
        ]))
        let t = m.style(m.index(of: "moved")!).transform
        XCTAssertEqual(t.tx, 10, accuracy: 1e-4)
        XCTAssertEqual(t.ty, 4, accuracy: 1e-4)
        XCTAssertEqual(t.scale, 2, accuracy: 1e-4)
        XCTAssertEqual(t.rotate, 90, accuracy: 1e-3)
        XCTAssertEqual(m.frame(m.index(of: "moved")!).height, 20, "paint-only: the frame never moves")
        let g = m.style(m.index(of: "ghost")!)
        XCTAssertTrue(g.invisible)
        XCTAssertTrue(g.pointerNone)
        XCTAssertEqual(g.cursor, "pointer")
        XCTAssertEqual(g.userSelect, "none")
        XCTAssertEqual(m.frame(m.index(of: "ghost")!).height, 20, "hidden keeps its box")
        let c = m.style(m.index(of: "clipped")!)
        XCTAssertTrue(c.clipX && c.clipY && c.scrollY && !c.scrollX)
        let scroll = try XCTUnwrap(m.scroll(m.index(of: "clipped")!), "the core reports the scroll container")
        XCTAssertTrue(scroll.scrollsY && !scroll.scrollsX)
    }

    func testTransformsFoldInSourceOrder() {
        let a = PaintTransform.parse(#"[{"op":"translate","x":10,"y":0},{"op":"scale","factor":2}]"#)
        XCTAssertEqual(a.tx, 10); XCTAssertEqual(a.scale, 2)
        let b = PaintTransform.parse(#"[{"op":"scale","factor":2},{"op":"translate","x":10,"y":0}]"#)
        XCTAssertEqual(b.tx, 20, "a scale first doubles the later translate")
        XCTAssertTrue(PaintTransform.parse(nil).isIdentity)
    }

    func testTextKeysShapeTheRuns() throws {
        let m = try surface(root([
            node("t", "Text", props: .object(["text": .string("hello world")]), style: .object(["letterSpacing": .number(2), "textTransform": .string("capitalize"), "textDecoration": .string("underline"), "fontStyle": .string("italic"), "fontWeight": .number(800), "textAlign": .string("justify")])),
        ]))
        let s = m.style(m.index(of: "t")!)
        XCTAssertEqual(s.letterSpacing, 2)
        XCTAssertEqual(s.textTransform, "capitalize")
        XCTAssertEqual(s.textDecoration, .underline)
        XCTAssertTrue(s.italic)
        XCTAssertEqual(s.textAlign, "justify")
        XCTAssertEqual(m.textStyle(m.index(of: "t")!).fontWeight, 800, "weights 300–800 reach the text style")
        let paint = TextPaint().merged(s)
        let src = TextShaper.attributed("hello world", .body)
        let shaped = TextLabel.shaped(src, paint, rtl: false)
        XCTAssertEqual(shaped.string, "Hello World")
        XCTAssertEqual(shaped.attribute(.kern, at: 0, effectiveRange: nil) as? CGFloat, 2)
        XCTAssertEqual(shaped.attribute(.underlineStyle, at: 0, effectiveRange: nil) as? Int, NSUnderlineStyle.single.rawValue)
        let font = shaped.attribute(.font, at: 0, effectiveRange: nil) as? PlatformFont
        #if canImport(UIKit)
        XCTAssertTrue(font?.fontDescriptor.symbolicTraits.contains(.traitItalic) ?? false)
        #else
        XCTAssertTrue(font?.fontDescriptor.symbolicTraits.contains(.italic) ?? false)
        #endif
        XCTAssertEqual(TextPaint.transform("it's a well-known fact", "capitalize"), "It's A Well-Known Fact")
        XCTAssertEqual(TextPaint.transform("MiXeD", "lowercase"), "mixed")
        // rtl: the natural alignment sits on the right, the base direction flips.
        let rtl = TextLabel.shaped(src, TextPaint(), rtl: true)
        let p = rtl.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle
        XCTAssertEqual(p?.baseWritingDirection, .rightToLeft)
        XCTAssertEqual(p?.alignment, .right)
        XCTAssertTrue(TextLabel.shaped(src, TextPaint(), rtl: false) === src, "plain ltr text is untouched")
    }

    // MARK: - motion

    func testTransitionsAnimateOnlyWithADurationAndNeverUnderReducedMotion() throws {
        let m = try surface(root([
            node("anim", "Box", style: .object(["height": .number(20), "transition": .string("$motion.fast"), "transitionEasing": .string("$ease.decelerate")])),
            node("still", "Box", style: .object(["height": .number(20)])),
        ]))
        let a = try XCTUnwrap(m.style(m.index(of: "anim")!).transition)
        XCTAssertGreaterThan(a.durationMs, 0)
        XCTAssertEqual(a.easing.count, 4)
        XCTAssertNotNil(a.animation())
        XCTAssertNil(a.animation(reduceMotion: true), "reduced motion = 0 ms")
        XCTAssertNil(m.style(m.index(of: "still")!).transition, "nothing animates without `transition`")
        // The surface's reduced-motion setting: the core resolves 0 ms.
        m.setReducedMotion(true)
        XCTAssertNil(m.style(m.index(of: "anim")!).transition)
        XCTAssertTrue(m.reducedMotion)
        XCTAssertNil(PaintTransition(durationMs: 0, easing: []).animation())
    }

    func testCubicBezierMatchesCSS() {
        XCTAssertEqual(cubicBezier([0.25, 0.1, 0.25, 1], 0), 0, accuracy: 1e-4)
        XCTAssertEqual(cubicBezier([0.25, 0.1, 0.25, 1], 1), 1, accuracy: 1e-4)
        XCTAssertEqual(cubicBezier([0, 0, 1, 1], 0.3), 0.3, accuracy: 1e-3)
        XCTAssertGreaterThan(cubicBezier([0.2, 0, 0, 1], 0.5), 0.5, "decelerating curves run ahead")
    }

    // MARK: - §4 rtl

    func testRTLMirrorsExactlyTheContractGlyphs() throws {
        let locale = JSONValue.parse(try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("catalog/locale.json"), encoding: .utf8))
        let names = Set((locale["rtlMirroredIcons"]?.array ?? []).compactMap(\.string))
        XCTAssertFalse(names.isEmpty)
        XCTAssertEqual(RTLGlyphs.mirrored, names)
        XCTAssertTrue(RTLGlyphs.mirrors("ui-chevron-right", rtl: true))
        XCTAssertFalse(RTLGlyphs.mirrors("ui-chevron-right", rtl: false))
        XCTAssertFalse(RTLGlyphs.mirrors("ui-chevron-down", rtl: true), "up/down never flip")
        XCTAssertFalse(RTLGlyphs.mirrors("ui-play", rtl: true), "media transport never flips")
    }

    func testAnRTLLocaleFlipsTheSurfaceDirection() throws {
        let m = try surface(root([node("a", "Text", props: .object(["text": .string("مرحبا")]))]))
        XCTAssertFalse(m.isRTL)
        m.setLocale("ar-EG")
        XCTAssertTrue(m.isRTL)
    }

    // MARK: - §5 layers

    func testLayersStackModalAndDismiss() throws {
        let m = try surface(root([
            node("dlg", "Dialog", props: .object(["title": .string("Sure?"), "open": .bool(true), "dismissible": .bool(false)]), children: [node("dlg-body", "Text", props: .object(["text": .string("Body")]))]),
            node("toast", "Toast", props: .object(["title": .string("Saved"), "open": .bool(true)])),
        ]), width: 390)
        let dialog = try XCTUnwrap(m.layers.first { $0.kind == "Dialog" })
        XCTAssertTrue(dialog.modal)
        XCTAssertFalse(dialog.dismissible)
        XCTAssertGreaterThan(dialog.frame.width, 0)
        if let toast = m.layers.first(where: { $0.kind == "Toast" }) {
            XCTAssertTrue(toast.isToast)
            XCTAssertFalse(toast.modal)
            XCTAssertFalse(PaintedLayers.dismissesOnOutsidePress(toast), "a toast never closes on an outside press")
            XCTAssertEqual(PaintedLayers.stacked(m.layers).last?.kind, "Toast", "toasts stack above overlays")
        }
        // A scrim press on a non-dismissible dialog does nothing.
        m.dismissLayer(dialog.owner)
        XCTAssertTrue(m.layers.contains { $0.kind == "Dialog" })
    }

    func testADismissibleLayerClosesThroughTheCore() throws {
        let host = RecordingHost()
        let m = try surface(root([
            node("pop", "Popover", props: .object(["open": .bool(true)]), children: [node("pop-body", "Text", props: .object(["text": .string("Hi")]))]),
        ]))
        m.host = host
        let pop = try XCTUnwrap(m.layers.first { $0.owner == "pop" })
        XCTAssertFalse(pop.modal)
        XCTAssertTrue(PaintedLayers.dismissesOnOutsidePress(pop))
        XCTAssertTrue(pop.dismissible)
        m.dismissLayer(pop.owner)
        XCTAssertFalse(m.layers.contains { $0.owner == "pop" })
        XCTAssertEqual(m.justDismissed, "pop", "the trigger's own press does not reopen it")
    }

    func testSheetDragsOnlyTowardItsEdge() {
        XCTAssertEqual(sheetOffset("bottom", 30, 50), CGPoint(x: 0, y: 50))
        XCTAssertEqual(sheetOffset("bottom", 30, -50), CGPoint(x: 0, y: 0))
        XCTAssertEqual(sheetOffset("left", -40, 10), CGPoint(x: -40, y: 0))
        XCTAssertEqual(sheetOffset("right", -40, 10), .zero)
        XCTAssertEqual(sheetOffset("top", 0, -70), CGPoint(x: 0, y: -70))
        XCTAssertEqual(sheetDismissDistance, 64)
    }

    // MARK: - §6 accessibility

    func testRolesMapToTraits() throws {
        let m = try surface(root([
            node("btn", "Button", props: .object(["label": .string("Save")])),
            node("lnk", "Link", props: .object(["label": .string("Docs"), "href": .string("https://example.com")])),
            node("hd", "Heading", props: .object(["text": .string("Title"), "level": .string("h2")])),
            node("prog", "Progress", props: .object(["value": .number(30), "max": .number(60), "label": .string("Upload")])),
            node("img", "Image", props: .object(["src": .string("x.png"), "alt": .string("A cat")])),
            node("deco", "Icon", props: .object(["name": .string("ui-check")])),
            node("named", "Icon", props: .object(["name": .string("ui-check"), "label": .string("Done")])),
            node("live", "Text", props: .object(["text": .string("3 new"), "live": .string("polite")])),
            node("gone", "Text", props: .object(["text": .string("x")]), style: .object(["visibility": .string("hidden")])),
        ]))
        func info(_ id: String) -> A11yInfo {
            let i = m.index(of: id)!
            return A11yInfo.of(m.node(i)!, model: m, style: m.style(i))
        }
        XCTAssertTrue(info("btn").traits.contains(.isButton))
        XCTAssertEqual(info("btn").label, "Save")
        XCTAssertTrue(info("lnk").traits.contains(.isLink))
        XCTAssertTrue(info("img").traits.contains(.isImage))
        XCTAssertEqual(info("img").label, "A cat")
        XCTAssertTrue(info("deco").hidden, "decorative icons leave the tree")
        XCTAssertFalse(info("named").hidden)
        XCTAssertTrue(info("named").traits.contains(.isImage))
        XCTAssertTrue(info("live").live)
        XCTAssertTrue(info("live").traits.contains(.updatesFrequently))
        XCTAssertTrue(info("gone").hidden, "visibility: hidden leaves the a11y tree")
        // Macro parts carry their `$a11y`: the heading level, the progress values.
        let heading = m.nodes.first { $0.accessibility?["role"]?.string == "heading" || A11yInfo.role(of: $0, model: m) == "heading" }
        let h = try XCTUnwrap(heading, "a heading node")
        let hi = A11yInfo.of(h, model: m, style: m.style(h.index))
        XCTAssertTrue(hi.traits.contains(.isHeader))
        XCTAssertEqual(hi.heading, .h2)
        let progress = try XCTUnwrap(m.nodes.first { A11yInfo.role(of: $0, model: m) == "progressbar" })
        let pi = A11yInfo.of(progress, model: m, style: m.style(progress.index))
        XCTAssertEqual(pi.value, "50%")
        XCTAssertFalse(pi.container, "a progress bar is one element")
        XCTAssertEqual(A11yInfo.componentRole("Slider"), "slider")
        XCTAssertEqual(A11yInfo.componentRole("Skeleton"), "hidden")
        XCTAssertEqual(A11yInfo.traits(role: "dialog"), .isModal)
        XCTAssertEqual(A11yInfo.headingLevel(3), .h3)
    }

    func testDialogContentIsAModalDialog() throws {
        let m = try surface(root([
            node("dlg", "Dialog", props: .object(["title": .string("Sure?"), "open": .bool(true)]), children: [node("dlg-body", "Text", props: .object(["text": .string("Body")]))]),
        ]))
        let content = try XCTUnwrap(m.nodes.first { $0.ownerComponent == "Dialog" && $0.part == "content" })
        XCTAssertEqual(A11yInfo.role(of: content, model: m), "dialog")
        XCTAssertTrue(A11yInfo.of(content, model: m, style: m.style(content.index)).traits.contains(.isModal))
    }

    func testThePainterPaintsTheRound1Keys() throws {
        // Every node of a tree using the new keys paints (no crash, no
        // missing body) in a hosting view, layers included.
        let m = try surface(root([
            node("g", "Box", style: .object(["height": .number(30), "backgroundGradient": .object(["angle": .number(45), "stops": .array([.object(["color": .string("#ff0000"), "offset": .number(0)]), .object(["color": .string("#0000ff"), "offset": .number(1)])])]), "borderTopWidth": .number(2), "borderColor": .string("#000000"), "borderStyle": .string("dotted"), "transform": .string("rotate(10deg)"), "transition": .string("$motion.fast")])),
            node("s", "Box", style: .object(["height": .number(60), "overflowY": .string("scroll")]), children: (0..<10).map { node("r\($0)", "Text", props: .object(["text": .string("Row \($0)")])) }),
            node("i", "Icon", props: .object(["name": .string("ui-chevron-right")]), style: .object(["transform": .string("rotate(90deg)")])),
            node("dlg", "Dialog", props: .object(["title": .string("Sure?"), "open": .bool(true)]), children: [node("dlg-body", "Text", props: .object(["text": .string("Body")]))]),
        ]))
        m.surface.setLocale(locale: "he")
        m.invalidate(structure: true)
        var painted = Set<Int>()
        m.paintProbe = { painted.insert($0) }
        defer { m.paintProbe = nil }
        let view = ExponentialSurface(model: m).frame(width: 400)
        #if canImport(AppKit)
        let host = NSHostingView(rootView: view)
        host.frame = CGRect(x: 0, y: 0, width: 400, height: 800)
        host.layoutSubtreeIfNeeded()
        #else
        let host = UIHostingController(rootView: view).view!
        host.frame = CGRect(x: 0, y: 0, width: 400, height: 800)
        host.layoutIfNeeded()
        #endif
        for id in ["g", "s", "r0", "i", "dlg-body"] {
            XCTAssertTrue(painted.contains(m.index(of: id)!), "\(id) painted")
        }
    }
}
