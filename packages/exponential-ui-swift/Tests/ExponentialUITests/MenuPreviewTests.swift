import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// #EXP-1629: `fixtures/interactions-round1.json` `menuRows` + `menuPreview`
/// through the SwiftUI painter's MODEL (the core's fixed measure): the core
/// lays out an item's `identifier` (between its icon and its label) and
/// `detail` (after the label); a row's `preview` shows as the menu's own
/// layer while the row is hovered (`setHover`, after 150 ms on the test's
/// clock: the core's `hoverTimer` through `hoverTimerSink`) or keyboard-
/// highlighted (`setFocus` from the keyboard, at once), hides 80 ms after
/// both left, never takes Escape or the focus order, and its nested
/// surface's events fire the Menu's `action`.
@MainActor
final class MenuPreviewTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private func mount(_ tree: JSONValue, host: Round1Host) throws -> SurfaceModel {
        let m = try makeModel("menu", theme: "neutral", mode: .light, width: 390)
        m.host = host
        try m.setNested(json: tree.json)
        m.pass()
        m.setOpen("m", true)
        m.pass()
        return m
    }

    private func menuTree(_ items: JSONValue) -> JSONValue {
        .object(["id": .string("m"), "component": .string("Menu"), "props": .object(["label": .string("More"), "items": items])])
    }

    func testMenuRowCases() throws {
        let cases = try XCTUnwrap(Fixtures.json("interactions-round1.json")["menuRows"]?.array)
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let m = try mount(menuTree(c["items"]!), host: Round1Host())
            if let sub = c["submenu"]?.number {
                m.press(id: "m.item.\(Int(sub))")
                m.pass()
            }
            for r in c["rows"]?.array ?? [] {
                let key = r["key"]?.string ?? "?"
                let row = try XCTUnwrap(m.node(id: "m.item.\(key)"), "\(name): row \(key)")
                var parts: [String] = []
                for child in row.children {
                    guard let n = m.node(child) else { continue }
                    switch n.part {
                    case "itemLabel":
                        if n.props["icon"]?.string != nil { parts.append("icon") }
                        parts.append("label")
                        XCTAssertEqual(n.props.text("text"), r["label"]?.string, "\(name): \(key) label")
                    case "itemIcon": parts.append("icon")
                    case "check": parts.append("check")
                    case "identifier":
                        parts.append("identifier")
                        XCTAssertEqual(n.props.text("text"), r["identifier"]?.string, "\(name): \(key) identifier")
                    case "detail":
                        parts.append("detail")
                        XCTAssertEqual(n.props.text("text"), r["detail"]?.string, "\(name): \(key) detail")
                    case "shortcut": parts.append("shortcut")
                    case "submenuIndicator": parts.append("submenuIndicator")
                    default: break
                    }
                }
                XCTAssertEqual(parts, r["parts"]?.array?.compactMap(\.string), "\(name): \(key) parts")
            }
        }
    }

    /// The row whose preview shows (its layer's root `m.preview.<key>`).
    private func shown(_ m: SurfaceModel) -> String? {
        let previews = m.layers.filter { l in m.node(l.root).map { $0.ownerComponent == "Menu" && $0.part == "preview" } ?? false }
        XCTAssertLessThanOrEqual(previews.count, 1, "one preview at most")
        return previews.first.flatMap { m.node($0.root)?.id }.map { String($0.dropFirst("m.preview.".count)) }
    }

    /// Every Text / Button text of the preview's nested surface.
    private func previewTexts(_ m: SurfaceModel, _ key: String) throws -> [String] {
        let leaf = try XCTUnwrap(m.node(id: "m.preview.\(key).surface"))
        let nested = try XCTUnwrap(m.embedModel(id: leaf.id, props: leaf.props))
        if nested.width == 0 { nested.setViewport(width: 224, height: 800) }
        nested.pass()
        return nested.order.compactMap { nested.node($0) }.filter { !$0.hidden }.compactMap { n in
            n.component == "Text" ? n.props.text("text") : n.component == "Button" ? n.props.text("label") : nil
        }.filter { !$0.isEmpty }
    }

    func testMenuPreviewCases() throws {
        let fx = try XCTUnwrap(Fixtures.json("interactions-round1.json")["menuPreview"])
        let cases = try XCTUnwrap(fx["cases"]?.array)
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let host = Round1Host()
            let m = try mount(fx["menu"]!, host: host)
            var now = 0.0
            var due: Double?
            m.hoverTimerSink = { owner, ms in
                XCTAssertEqual(owner, "m.preview", "\(name): the timer's owner")
                due = now + ms
            }
            if let sub = c["submenu"]?.number {
                m.press(id: "m.item.\(Int(sub))")
                m.pass()
            }
            var hovered: String?
            for (i, step) in (c["steps"]?.array ?? []).enumerated() {
                let before = host.actions.count
                if case .object(let o) = step, o.keys.contains("hover") {
                    if let h = hovered { m.setHover(id: "m.item.\(h)", false) }
                    hovered = step["hover"]?.string
                    if let h = hovered { m.setHover(id: "m.item.\(h)", true) }
                }
                if case .object(let o) = step, o.keys.contains("highlight") {
                    m.setFocus(step["highlight"]?.string.map { "m.item.\($0)" }, keyboard: true)
                }
                if let over = step["overPreview"]?.bool {
                    let key = try XCTUnwrap(shown(m), "\(name): step \(i): a preview to hover")
                    m.setHover(id: "m.preview.\(key)", over)
                }
                m.pass()
                if let wait = step["wait"]?.number {
                    now += wait
                    while let d = due, d <= now {
                        due = nil
                        let at = now
                        now = d
                        m.dispatch(m.surface.hoverTimeout(owner: "m.preview"))
                        now = at
                        m.pass()
                    }
                }
                if let inner = step["press"]?.string {
                    let key = try XCTUnwrap(shown(m))
                    let leaf = try XCTUnwrap(m.node(id: "m.preview.\(key).surface"))
                    let nested = try XCTUnwrap(m.embedModel(id: leaf.id, props: leaf.props))
                    if nested.width == 0 { nested.setViewport(width: 224, height: 800) }
                    nested.press(id: inner)
                    m.pass()
                }
                let key = shown(m)
                XCTAssertEqual(key, step["shown"]?.string, "\(name): step \(i): shown")
                if let key {
                    let texts = try previewTexts(m, key)
                    for t in step["texts"]?.array?.compactMap(\.string) ?? [] {
                        XCTAssertTrue(texts.contains { $0.contains(t) }, "\(name): step \(i): \(t) in \(texts)")
                    }
                    // The preview never takes the focus order (the menu keeps it).
                    XCTAssertFalse(m.focusOrder().contains { m.node($0)?.id.hasPrefix("m.preview.") ?? false }, "\(name): step \(i): focus order")
                }
                let heard = host.actions.dropFirst(before).filter { $0.name != "picked" }.map { JSONValue.object(["name": .string($0.name), "context": $0.context]) }
                XCTAssertEqual(heard, step["events"]?.array ?? [], "\(name): step \(i): events")
                XCTAssertTrue(m.layers.contains { $0.owner == "m" && m.node($0.root)?.id == "m.content" }, "\(name): step \(i): the menu stays open")
            }
        }
    }
}
