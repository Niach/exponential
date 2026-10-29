import XCTest
import SwiftUI
import ExponentialUICore
@testable import ExponentialUI

#if canImport(AppKit)
import AppKit

/// VAPP-163: three drifts of the painter on macOS against the web (the
/// spezl drafts): a `bar` Segmented's caption sits CENTRED under its icon,
/// the Composer shows its `placeholder`, and an Input's text / placeholder
/// is centred VERTICALLY in the field box (AppKit's cell drew at the top).
/// `VAPP163_SHOTS=<path prefix>` also writes `<prefix>-{bar,composer,input}.png`.
@MainActor
final class MacFieldAndBarPaintTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    func node(_ id: String, _ component: String, _ props: [String: JSONValue] = [:]) -> JSONValue {
        .object(["id": .string(id), "component": .string(component), "props": .object(props)])
    }

    struct Painted {
        let model: SurfaceModel
        let view: NSView
        let rep: NSBitmapImageRep
        var scale: CGFloat { CGFloat(rep.pixelsWide) / view.bounds.width }
    }

    /// Paint `child` in a Stack at `width` over the theme background.
    func paint(_ child: JSONValue, name: String, width: CGFloat = 390) throws -> Painted {
        var o = SurfaceOptions()
        o.theme = ThemeHandle.builtin("exponential")
        o.mode = .dark
        let m = try SurfaceModel(id: name, options: o, host: NoHost())
        var stack = node("col", "Stack", ["padding": .string("md")]).object!
        stack["children"] = .array([child])
        try m.setNested(json: JSONValue.object(stack).json)
        m.setViewport(width: width, height: 400)
        let height = max(m.surfaceSize.height, 40)
        let root = ExponentialSurface(model: m).frame(width: width, height: height, alignment: .topLeading).background(m.color("background") ?? .black)
        let view = NSHostingView(rootView: root)
        view.frame = CGRect(x: 0, y: 0, width: width, height: height)
        view.layoutSubtreeIfNeeded()
        _ = view.fittingSize
        view.layoutSubtreeIfNeeded()
        let rep = try XCTUnwrap(view.bitmapImageRepForCachingDisplay(in: view.bounds))
        view.cacheDisplay(in: view.bounds, to: rep)
        if let prefix = ProcessInfo.processInfo.environment["VAPP163_SHOTS"] {
            try rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: "\(prefix)-\(name).png"))
        }
        return Painted(model: m, view: view, rep: rep)
    }

    /// The ink (pixels off the background) inside `rect` (points): its
    /// bounding box in points, nil = none.
    func ink(_ p: Painted, in rect: CGRect) -> CGRect? {
        let s = p.scale
        guard let bg = p.rep.colorAt(x: 1, y: 1)?.usingColorSpace(.deviceRGB) else { return nil }
        var minX = Int.max, minY = Int.max, maxX = Int.min, maxY = Int.min
        for y in Int(rect.minY * s)..<Int(rect.maxY * s) {
            for x in Int(rect.minX * s)..<Int(rect.maxX * s) {
                guard let c = p.rep.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB) else { continue }
                let d = abs(c.redComponent - bg.redComponent) + abs(c.greenComponent - bg.greenComponent) + abs(c.blueComponent - bg.blueComponent)
                if d > 0.45 {
                    minX = min(minX, x); maxX = max(maxX, x); minY = min(minY, y); maxY = max(maxY, y)
                }
            }
        }
        guard minX <= maxX else { return nil }
        return CGRect(x: CGFloat(minX) / s, y: CGFloat(minY) / s, width: CGFloat(maxX - minX + 1) / s, height: CGFloat(maxY - minY + 1) / s)
    }

    func views<T: NSView>(_ type: T.Type, in root: NSView) -> [T] {
        root.subviews.flatMap { ($0 as? T).map { [$0] } ?? [] } + root.subviews.flatMap { views(type, in: $0) }
    }

    /// Each `bar` item is a column: the caption is centred under the icon
    /// (it sat at the item's leading edge).
    func testBarCaptionIsCentredInItsItem() throws {
        let items: JSONValue = .array([
            .object(["label": .string("Chats"), "value": .string("chats"), "icon": .string("message-circle")]),
            .object(["label": .string("Contacts"), "value": .string("contacts"), "icon": .string("users")]),
            .object(["label": .string("Settings"), "value": .string("settings"), "icon": .string("settings")]),
        ])
        let p = try paint(node("bar", "Segmented", ["variant": .string("bar"), "value": .string("chats"), "items": items]), name: "bar")
        let bar = p.model.frame(try XCTUnwrap(p.model.index(of: "bar")))
        let w = bar.width / 3
        for k in 0..<3 {
            let item = CGRect(x: bar.minX + CGFloat(k) * w, y: bar.minY, width: w, height: bar.height)
            // Caption rows: the lower half of the bar (the icon sits above).
            let caption = try XCTUnwrap(ink(p, in: CGRect(x: item.minX + 1, y: item.midY + 2, width: item.width - 2, height: item.height / 2 - 2)), "item \(k) caption painted")
            XCTAssertEqual(caption.midX, item.midX, accuracy: 2, "item \(k): caption centred in the item")
            let icon = try XCTUnwrap(ink(p, in: CGRect(x: item.minX + 1, y: item.minY + 1, width: item.width - 2, height: item.height / 2 - 2)), "item \(k) icon painted")
            XCTAssertEqual(icon.midX, caption.midX, accuracy: 2, "item \(k): caption under its icon")
        }
    }

    /// The Composer's text view draws its placeholder while empty.
    func testComposerShowsItsPlaceholder() throws {
        let p = try paint(node("composer", "Composer", ["placeholder": .string("Ask anything")]), name: "composer")
        let tv = try XCTUnwrap(views(OwnedTextField.PlaceholderTextView.self, in: p.view).first)
        XCTAssertEqual(tv.placeholder?.string, "Ask anything")
        let rect = try XCTUnwrap(tv.placeholderRect, "shown while empty")
        let inField = tv.convert(rect, to: p.view)
        XCTAssertNotNil(ink(p, in: inField.insetBy(dx: 0, dy: -1)), "the placeholder is painted")
        tv.string = "typed"
        XCTAssertNil(tv.placeholderRect, "hidden once there is text")
    }

    /// An Input's placeholder (and its text, the field editor's line) is
    /// centred vertically in the field box, as UITextField and the web.
    func testInputTextIsCentredVertically() throws {
        let p = try paint(node("search", "Input", ["name": .string("search"), "label": .string(""), "type": .string("search"), "placeholder": .string("Search")]), name: "input")
        let field = try XCTUnwrap(views(OwnedTextField.CenteredTextField.self, in: p.view).first)
        let line = try XCTUnwrap(field.cell?.drawingRect(forBounds: field.bounds))
        XCTAssertGreaterThan(field.bounds.height, line.height, "the field box is taller than one line")
        XCTAssertEqual(line.midY, field.bounds.midY, accuracy: 1, "the text line is centred in the field")
        let box = field.convert(field.bounds, to: p.view)
        let glyphs = try XCTUnwrap(ink(p, in: box), "the placeholder is painted")
        XCTAssertEqual(glyphs.midY, box.midY, accuracy: 3, "the placeholder glyphs sit in the middle of the field")
    }
}
#endif
