import XCTest
import ExponentialUICore
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
}
