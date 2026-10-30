import XCTest
import ExponentialUICore
import SwiftUI
@testable import ExponentialUI

/// The round-2 painter asks (contract §7 "Painter asks", VAPP-100) as the
/// SwiftUI painter adopts them: text style fields, the surface Formatter,
/// Resizable drags and keys, sticky offsets, windowed lists, keyframe
/// frames.
@MainActor
final class Round2PainterTests: XCTestCase {
    func testTextStyleCarriesSpacingTransformAndItalic() {
        let plain = TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: nil)
        var spaced = plain
        spaced.letterSpacing = 2
        // Letter spacing follows every character (CSS).
        XCTAssertEqual(TextShaper.width("abcd", spaced), TextShaper.layoutUnit(TextShaper.width("abcd", plain) + 8), accuracy: 0.05)
        var upper = plain
        upper.textTransform = "uppercase"
        XCTAssertEqual(upper.shown("Run two"), "RUN TWO")
        XCTAssertEqual(TextShaper.measure("abc", upper, wrap: nil, lines: nil).width, TextShaper.width("ABC", plain))
        var cap = plain
        cap.textTransform = "capitalize"
        XCTAssertEqual(cap.shown("hello big world"), "Hello Big World")
        let italic = TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: nil, italic: true)
        let font = TextShaper.attributes(italic)[.font] as! PlatformFont
        #if canImport(AppKit)
        XCTAssertTrue(font.fontDescriptor.symbolicTraits.contains(.italic))
        #else
        XCTAssertTrue(font.fontDescriptor.symbolicTraits.contains(.traitItalic))
        #endif
    }

    func testTheSurfaceFormatsThroughFoundation() throws {
        let m = try makeModel(fixed: true)
        XCTAssertNotNil(m.formatter)
        XCTAssertEqual(m.formatter?.locale(), "en-US")
        m.setSettings(SurfaceSettings(locale: "de-DE", mode: .dark, timeZone: "Europe/Berlin"))
        XCTAssertEqual(m.formatter?.locale(), "de-DE")
        XCTAssertEqual(m.formatter?.number(value: 1234.5, decimals: 2, grouping: true), "1.234,50")
        try m.setNested(json: #"{"id":"t","component":"Text","props":{"text":{"call":"formatNumber","args":{"value":1234.5,"decimals":1}}}}"#)
        XCTAssertEqual(m.node(id: "t")?.props.text("text"), "1.234,5", "\(m.node(id: "t")?.props.json ?? "-") \(m.issues)")
    }

    func testAResizableHandleDragsAndKeysThroughTheCore() throws {
        let m = try makeModel(fixed: true)
        try m.setNested(json: #"{"id":"split","component":"Resizable","props":{"direction":"horizontal","handle":true,"style":{"height":200}},"children":[{"id":"a","component":"Text","props":{"text":"A"}},{"id":"b","component":"Text","props":{"text":"B"}}]}"#)
        guard let handle = m.nodes.first(where: { $0.ownerComponent == "Resizable" && $0.part == "handle" && !$0.removed }) else { return XCTFail("no handle") }
        let before = m.frame(m.index(of: "a")!).width
        m.resizeDrag(handle.index, phase: "start", delta: 0)
        m.resizeDrag(handle.index, phase: "move", delta: 40)
        m.resizeDrag(handle.index, phase: "end", delta: 40)
        let dragged = m.frame(m.index(of: "a")!).width
        XCTAssertGreaterThan(dragged, before + 30)
        m.nodeKey(handle.index, key: "ArrowLeft")
        XCTAssertLessThan(m.frame(m.index(of: "a")!).width, dragged)
    }

    func testStickyNodesCarryTheCoreOffset() throws {
        let m = try makeModel(fixed: true)
        var rows = (0..<40).map { #"{"id":"r\#($0)","component":"Text","props":{"text":"row \#($0)"}}"# }
        rows.insert(#"{"id":"head","component":"Text","props":{"text":"Head","style":{"position":"sticky","top":0}}}"#, at: 0)
        try m.setNested(json: #"{"id":"page","component":"Box","children":[{"id":"root","component":"Box","props":{"style":{"display":"flex","flexDirection":"column","height":200,"overflowY":"auto"}},"children":[\#(rows.joined(separator: ","))]}]}"#)
        guard let head = m.index(of: "head") else { return XCTFail("no head") }
        XCTAssertTrue(m.style(head).sticky)
        m.scrollTo(id: "root", offset: CGPoint(x: 0, y: 120))
        XCTAssertEqual(m.sticky[head]?.height ?? 0, 120, accuracy: 1, "\(m.sticky) scroll \(String(describing: m.scroll(m.index(of: "root")!)))")
    }

    func testKeyframeFramesComeFromTheCore() throws {
        let m = try makeModel(fixed: true)
        try m.setNested(json: #"{"id":"p","component":"Box","props":{"style":{"animation":"pulse","opacity":0.5,"width":10,"height":10}}}"#)
        guard let anim = m.style(m.index(of: "p")!).animation else { return XCTFail("no animation on the visual") }
        XCTAssertEqual(anim.name, "pulse")
        XCTAssertNil(anim.iterations)
        let mid = anim.frame(at: anim.durationMs / 2, reducedMotion: false)
        XCTAssertEqual(mid.paintedOpacity(own: 0.5), 0.25, accuracy: 1e-3)
        XCTAssertEqual(anim.frame(at: anim.durationMs / 2, reducedMotion: true).opacity, 1)
    }

    func testScrollToIndexIsAHostCommand() throws {
        let m = try makeModel(fixed: true)
        m.setData(path: "/items", value: .array((0..<200).map { .object(["id": .string("i\($0)"), "title": .string("Item \($0)")]) }))
        try m.setNested(json: #"{"id":"list","component":"List","props":{"style":{"height":300}},"template":{"path":"/items","key":"id","component":"row"},"children":[{"id":"row","component":"Text","props":{"text":{"path":"title"}}}]}"#)
        guard let list = m.lists["list"] else { return XCTFail("no list info") }
        XCTAssertTrue(list.windowed)
        XCTAssertFalse(list.horizontal)
        m.command(.scrollToIndex(id: "list", index: 150, align: "start"))
        let after = m.lists["list"]!
        XCTAssertTrue((after.start...after.end).contains(150), "window \(after.start)…\(after.end)")
    }

    // MARK: - EXP-1324: Text shimmer, Trace, CodeBlock variant/size

    /// `fixtures/text-shimmer.json` through the facade and `TextShimmer`.
    func testTextShimmerColoursReplayTheFixture() throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        let cases = try Round2Suites.textShimmer()
        XCTAssertGreaterThan(cases.count, 3)
        XCTAssertEqual(cases.compactMap { c in c.error.map { "\(c.name): \($0)" } }, [])
    }

    /// YO-30: `fixtures/text-clusters.json`: Characters = the fixture's
    /// clusters, tracking per cluster.
    func testTextClustersReplayTheFixture() throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        let cases = try Round2Suites.textClusters()
        XCTAssertEqual(cases.count, 4)
        XCTAssertEqual(cases.compactMap { c in c.error.map { "\(c.name): \($0)" } }, [])
    }

    // MARK: - NEW-77: VoiceWave

    /// `fixtures/voice-wave.json` through the facade and `VoiceWaveFrame`.
    func testVoiceWaveReplaysTheFixture() throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        let cases = try Round2Suites.voiceWave()
        XCTAssertGreaterThan(cases.count, 3)
        XCTAssertEqual(cases.compactMap { c in c.error.map { "\(c.name): \($0)" } }, [])
    }

    /// NEW-77 THE CHILD RULE, measured (`Round2Suites.voiceWaveChild`):
    /// each fixture child case as a VoiceWave of the case's edge around a
    /// solid child of the case's own size, rendered twice (the child filled,
    /// then transparent); the child's coverage is the difference, so its
    /// DRAWN bounds (after the painter's scale about the centre) come out
    /// to the sub-pixel and must sit within 0.5 px of the fixture's `child`
    /// box; at `sm` nothing is drawn.
    func testVoiceWaveChildrenAreDrawnAtTheFixturesChildBox() throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        #if canImport(AppKit)
        let sizes: [CGFloat: String] = [32: "sm", 56: "md", 160: "lg", 200: "xl"]
        func render(_ c: Round2Suites.VoiceWaveChildCase, filled: Bool) throws -> (NSBitmapImageRep, CGFloat) {
            let m = try makeModel(fixed: true)
            let fill = filled ? #","backgroundColor":"$color.destructive""# : ""
            try m.setNested(json: #"{"id":"w","component":"VoiceWave","props":{"activity":"\#(c.activity)","size":"\#(sizes[c.size] ?? "md")"},"children":[{"id":"c","component":"Box","style":{"width":\#(c.child.width),"height":\#(c.child.height)\#(fill)}}]}"#)
            XCTAssertEqual(m.frame(try XCTUnwrap(m.index(of: "w"))).origin, .zero)
            let host = NSHostingView(rootView: ExponentialSurface(model: m).frame(width: c.size, height: c.size, alignment: .topLeading).environment(\.xuiReducedMotion, true))
            host.frame = CGRect(x: 0, y: 0, width: c.size, height: c.size)
            host.layoutSubtreeIfNeeded()
            let rep = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
            host.cacheDisplay(in: host.bounds, to: rep)
            return (rep, CGFloat(rep.pixelsWide) / c.size)
        }
        func rgb(_ rep: NSBitmapImageRep, _ x: Int, _ y: Int) -> [Double] {
            guard let c = rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { return [0, 0, 0] }
            return [c.redComponent * c.alphaComponent, c.greenComponent * c.alphaComponent, c.blueComponent * c.alphaComponent]
        }
        let cases = try Round2Suites.voiceWaveChildCases()
        XCTAssertGreaterThanOrEqual(cases.count, 4)
        func measure(_ c: Round2Suites.VoiceWaveChildCase) throws -> CGRect? {
            let (filled, k) = try render(c, filled: true)
            let (empty, _) = try render(c, filled: false)
            let n = Int((c.size * k).rounded())
            let mid = n / 2
            // The child's colour where it surely covers: the box centre.
            let full = rgb(filled, mid, mid)
            // Coverage of one device pixel: how far the filled render moved
            // from the empty one, against the full colour.
            func coverage(_ x: Int, _ y: Int) -> Double {
                let a = rgb(filled, x, y), b = rgb(empty, x, y)
                let span = zip(full, b).map { abs($0 - $1) }.max() ?? 0
                guard span > 0.05 else { return 0 }
                return min(1, (zip(a, b).map { abs($0 - $1) }.max() ?? 0) / span)
            }
            // A box rasterized by area coverage: its length is the sum, its
            // start the first covered pixel plus what that pixel lacks.
            func extent(_ cov: [Double]) -> (start: Double, length: Double)? {
                let length = cov.reduce(0, +)
                guard length > 0.02, let first = cov.firstIndex(where: { $0 > 0.02 }) else { return nil }
                return (Double(first) + 1 - cov[first], length)
            }
            guard let h = extent((0..<n).map { coverage($0, mid) }), let v = extent((0..<n).map { coverage(mid, $0) }) else { return nil }
            return CGRect(x: h.start / k, y: v.start / k, width: h.length / k, height: v.length / k)
        }
        var drawn: [String: CGRect] = [:]
        for c in cases { drawn[c.name] = try measure(c) }
        let failed = try Round2Suites.voiceWaveChild { drawn[$0.name] }
        XCTAssertEqual(failed.compactMap { c in c.error.map { "\(c.name): \($0)" } }, [])
        #endif
    }

    /// The root recipe's edge, the idle opacity, the ring colours, the
    /// centred child (none at `sm`) and the img name with the activity.
    func testVoiceWaveMeasuresPaintsAndReads() throws {
        let m = try makeModel(fixed: true)
        try m.setNested(json: #"{"id":"col","component":"Stack","props":{"direction":"vertical","align":"start"},"children":[{"id":"vw","component":"VoiceWave","props":{"activity":"listening","level":0.6,"bands":[0.9,0.6,1,0.5],"size":"lg","label":"Voice"},"children":[{"id":"mark","component":"Icon","props":{"name":"mic"}}]},{"id":"quiet","component":"VoiceWave","props":{"activity":"idle","size":"sm"}}]}"#)
        let vw = try XCTUnwrap(m.index(of: "vw"))
        // The core centres the one child at its own size (no stretch).
        let probe = try makeModel(fixed: true)
        try probe.setNested(json: #"{"id":"v","component":"VoiceWave","props":{"activity":"listening","size":"lg"},"children":[{"id":"c","component":"Text","props":{"text":"Child"}}]}"#)
        let child = probe.frame(try XCTUnwrap(probe.index(of: "c")))
        XCTAssertEqual(child.midX, 80, accuracy: 0.5)
        XCTAssertEqual(child.midY, 80, accuracy: 0.5)
        XCTAssertLessThan(child.height, 40)
        let quiet = try XCTUnwrap(m.index(of: "quiet"))
        XCTAssertEqual(m.frame(vw).size, CGSize(width: 160, height: 160))
        XCTAssertEqual(m.frame(quiet).size, CGSize(width: 32, height: 32))
        XCTAssertEqual(m.style(quiet).opacity ?? 1, 0.35, accuracy: 1e-6)
        XCTAssertNotEqual(m.style(vw).opacity ?? 1, 0.35)
        let cx = LeafContext(model: m, node: m.node(vw)!, size: m.frame(vw).size, style: m.style(vw), ink: m.ink(vw), textStyle: m.textStyle(vw))
        XCTAssertNotNil(cx.part("VoiceWave", "outer").color)
        XCTAssertNotNil(cx.part("VoiceWave", "inner").color)
        let lg = try XCTUnwrap(VoiceWaveFrame.compute(size: 160, activity: "listening", level: 0.6, bands: nil, elapsed: 1, reducedMotion: false))
        XCTAssertEqual(lg.childSize, 35.2, accuracy: 0.01)
        XCTAssertEqual(lg.phase, 0.9, accuracy: 0.01)
        XCTAssertEqual(VoiceWaveFrame.compute(size: 160, activity: "listening", level: 0.6, bands: nil, elapsed: 1, reducedMotion: true)?.phase, 0)
        XCTAssertEqual(VoiceWaveFrame.compute(size: 32, activity: "idle", level: 0, bands: nil, elapsed: 0, reducedMotion: false)?.childSize, 0)
        XCTAssertEqual(VoiceWaveFrame.activity("banana"), "idle")
        XCTAssertEqual(VoiceWaveFrame.activity(""), "idle")
        XCTAssertEqual(VoiceWaveFrame.activity("muted"), "muted")
        let a = A11yInfo.of(m.node(vw)!, model: m, style: m.style(vw))
        XCTAssertEqual(a.label, "Voice, listening")
        XCTAssertTrue(a.traits.contains(.isImage))
        XCTAssertFalse(a.hidden)
        XCTAssertFalse(a.container)
        XCTAssertTrue(A11yInfo.of(m.node(quiet)!, model: m, style: m.style(quiet)).hidden)
        #if canImport(AppKit)
        let host = NSHostingView(rootView: ExponentialSurface(model: m).frame(width: 390, height: 300))
        host.frame = CGRect(x: 0, y: 0, width: 390, height: 300)
        host.layoutSubtreeIfNeeded()
        XCTAssertNotNil(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        #endif
    }

    /// The gradient: 200% + 2 × spread wide, sliding background-position
    /// 100% → 0% (rtl reversed); five stops centred on the highlight at
    /// CSS 90° + angle.
    func testTextShimmerGeometry() throws {
        let s = try XCTUnwrap(TextShimmer.resolve(color: "#a1a1a1", dark: true))
        XCTAssertEqual(s.gradientWidth(100), 320)
        XCTAssertEqual(s.offset(width: 100, phase: 0, rtl: false), -220)
        XCTAssertEqual(s.offset(width: 100, phase: 1, rtl: false), 0, accuracy: 1e-9)
        XCTAssertEqual(s.offset(width: 100, phase: 0, rtl: true), 0)
        let (g, start, end) = s.gradient(size: CGSize(width: 320, height: 20))
        XCTAssertEqual(g.stops.count, 5)
        XCTAssertEqual(g.stops[2].location, 0.5)
        XCTAssertEqual(g.stops[0].location + g.stops[4].location, 1, accuracy: 1e-9)
        XCTAssertLessThan(start.x, end.x, "left → right")
        XCTAssertLessThan(start.y, end.y, "110deg leans down")
        XCTAssertEqual((start.x + end.x) / 2, 0.5, accuracy: 1e-9)
        XCTAssertEqual(Color(hex: "#73737399")!.hexRGBA(), "#73737399")
        XCTAssertEqual(Color(hex: "#a1a1a1")!.hexRGBA(), "#a1a1a1")
    }

    /// A shimmering Text keeps the prop on its leaf and paints (masked
    /// gradient; reduced motion = the plain text).
    func testAShimmeringTextPaints() throws {
        let m = try makeModel(fixed: true)
        try m.setNested(json: #"{"id":"t","component":"Text","props":{"text":"Running tests","shimmer":true}}"#)
        let t = try XCTUnwrap(m.index(of: "t"))
        XCTAssertTrue(m.node(t)?.props.flag("shimmer") ?? false)
        XCTAssertNotNil(TextShimmer.resolve(color: m.ink(t).hexRGBA(), dark: true))
        #if canImport(AppKit)
        let host = NSHostingView(rootView: ExponentialSurface(model: m).frame(width: 390, height: 200))
        host.frame = CGRect(x: 0, y: 0, width: 390, height: 200)
        host.layoutSubtreeIfNeeded()
        XCTAssertNotNil(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        #endif
    }

    /// TraceItem: the rail = the content's LEFT border only, the chevron's
    /// negative margin from the core, rotated 90° open, a pending line dims.
    func testTraceItemPaintsItsRecipes() throws {
        let m = try makeModel(fixed: true)
        let item = { (open: Bool, status: String) in
            #"{"id":"trace","component":"Trace","children":[{"id":"ti","component":"TraceItem","props":{"title":"Ran bun test","icon":"trace-terminal","description":"4s","collapsible":true,"open":\#(open),"status":"\#(status)"},"children":[{"id":"c","component":"Text","props":{"text":"Child"}}]}]}"#
        }
        try m.setNested(json: item(true, "done"))
        XCTAssertEqual(m.nodes.filter { $0.component == "Unknown" && !$0.removed }.map(\.id), [])
        let content = try XCTUnwrap(m.index(of: "ti.content"))
        let rail = m.style(content).borderWidths
        XCTAssertEqual(rail[3], 1, "left border = the rail: \(rail)")
        XCTAssertEqual(rail[0] + rail[1] + rail[2], 0)
        let line = try XCTUnwrap(m.index(of: "ti.line"))
        let chevron = try XCTUnwrap(m.index(of: "ti.line.chevron"))
        let desc = try XCTUnwrap(m.index(of: "ti.line.description"))
        XCTAssertEqual(m.frame(chevron).minX - m.frame(desc).maxX, 8 - 4, accuracy: 0.5, "gap 8, marginLeft -4")
        XCTAssertNotEqual(m.style(chevron).transform, .identity, "open = rotate(90deg)")
        XCTAssertNotEqual(m.style(line).opacity ?? 1, 0.6)
        try m.setNested(json: item(false, "pending"))
        XCTAssertEqual(m.style(m.index(of: "ti.line.chevron")!).transform, .identity)
        XCTAssertEqual(m.style(m.index(of: "ti.line")!).opacity ?? 1, 0.6, accuracy: 1e-6)
    }

    /// CodeBlock `variant` / `size` reach the core's recipes; the new token
    /// kinds of `ansi` come back coloured.
    func testCodeBlockVariantSizeAndAnsiTokens() throws {
        let m = try makeModel(fixed: true)
        try m.setNested(json: #"{"id":"cb","component":"CodeBlock","props":{"code":"\u001b[32mok\u001b[0m \u001b[33mwarn\u001b[0m \u001b[31mfail\u001b[0m","language":"ansi","variant":"destructive","size":"sm"}}"#)
        let cb = try XCTUnwrap(m.index(of: "cb"))
        let plain = try makeModel(fixed: true)
        try plain.setNested(json: #"{"id":"cb","component":"CodeBlock","props":{"code":"ok","language":"ansi"}}"#)
        XCTAssertNotEqual(m.style(cb).borderColor, plain.style(plain.index(of: "cb")!).borderColor, "destructive variant recipe")
        let line = try XCTUnwrap(m.nodes.first { $0.recipeComponent == "CodeBlock" && $0.part == "code" && !$0.removed })
        let kinds = Set(line.props.list("tokens").compactMap { $0["kind"]?.string })
        XCTAssertTrue(kinds.isSuperset(of: ["success", "warning", "error"]), "\(kinds)")
        for kind in ["success", "warning", "error"] {
            XCTAssertNotNil(m.part("CodeBlock", "token", props: ["kind": .string(kind), "language": .string("ansi")]).color, kind)
        }
    }
}
