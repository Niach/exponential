import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// #EDE-65 lane M: `fixtures/embed.json` replayed through `EmbedCard` (the
/// pure half, `parse` + `errorRoot`) and through `SurfaceModel` (`render`,
/// `events`): the host mounted under the neutral theme, each Embed's card a
/// nested model, the painted texts read in the host's pre-order with the
/// nested surface in place of its Embed.
@MainActor
final class EmbedTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    /// Records what reaches the HOST (never the card's own host).
    private final class Host: HostPlugin {
        var actions: [SurfaceActionEvent] = []
        var calls: [SurfaceFunctionCall] = []
        func onAction(_ event: SurfaceActionEvent) { actions.append(event) }
        func onFunctionCall(_ call: SurfaceFunctionCall) { calls.append(call) }
    }

    private func mount(_ c: JSONValue, host: Host) throws -> SurfaceModel {
        let m = try makeModel("embed", theme: "neutral", mode: .dark, width: 390)
        m.host = host
        try m.setNested(json: c["root"]!.json)
        m.setData(path: "", value: c["data"] ?? .object([:]))
        m.pass()
        return m
    }

    /// The Embed nodes in painted (pre-) order.
    private func embeds(_ m: SurfaceModel) -> [NodeInfo] {
        m.order.compactMap { m.node($0) }.filter { $0.component == "Embed" && $0.part == nil && !$0.hidden }
    }

    /// The card of an Embed, laid out (the painter's `ExponentialSurface`
    /// reports its width; the measurer already did when it was asked).
    private func card(_ m: SurfaceModel, _ n: NodeInfo) throws -> SurfaceModel {
        let nested = try XCTUnwrap(m.embedModel(id: n.id, props: n.props), n.id)
        if nested.width == 0 { nested.setViewport(width: m.width, height: 800) }
        return nested
    }

    /// Every painted Text in the host's pre-order, each card's in place.
    private func texts(_ m: SurfaceModel) throws -> [String] {
        var out: [String] = []
        for i in m.order {
            guard let n = m.node(i), !n.hidden else { continue }
            if n.component == "Embed", n.part == nil {
                out += try texts(card(m, n))
            } else if n.component == "Text" {
                let t = n.props.text("text")
                if !t.isEmpty { out.append(t) }
            }
        }
        return out
    }

    private func assertTexts(_ painted: [String], _ c: JSONValue, _ name: String) {
        var at = 0
        for want in c["texts"]?.array?.compactMap(\.string) ?? [] {
            guard let i = painted[at...].firstIndex(of: want) else {
                XCTFail("\(name): \(want) not painted in order (painted \(painted))")
                continue
            }
            at = i + 1
        }
        for gone in c["absent"]?.array?.compactMap(\.string) ?? [] {
            XCTAssertFalse(painted.contains(gone), "\(name): \(gone) painted")
        }
    }

    // MARK: - the pure half

    func testParseCases() throws {
        let cases = try XCTUnwrap(Fixtures.json("embed.json")["parse"]?.array)
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let card = EmbedCard.card(c["surface"])
            XCTAssertEqual(card != nil, c["valid"]?.bool, name)
            if let card {
                XCTAssertEqual(JSONValue.object(card.data), c["data"], "\(name): data")
                XCTAssertEqual(card.root, c["surface"]?["root"], "\(name): root")
            }
            let mounted = EmbedCard.mount(c["surface"])
            XCTAssertEqual(mounted.valid, c["valid"]?.bool, "\(name): mount")
            if !mounted.valid {
                XCTAssertEqual(mounted.card.root, EmbedCard.errorRoot, "\(name): the error box")
                XCTAssertEqual(mounted.card.data, [:], "\(name): the error box's data")
            }
        }
    }

    func testErrorRootPayloadAndSurfaceId() throws {
        XCTAssertEqual(EmbedCard.errorRoot, try Fixtures.json("embed.json")["errorRoot"])
        XCTAssertEqual(EmbedCard.event, "action")
        XCTAssertEqual(EmbedCard.payload(name: "reply", context: .object(["text": .string("Yes")])), .object(["name": .string("reply"), "context": .object(["text": .string("Yes")])]))
        XCTAssertEqual(EmbedCard.payload(name: "go", context: nil), .object(["name": .string("go"), "context": .object([:])]))
        XCTAssertEqual(EmbedCard.surfaceId(host: "chat", component: "card"), "chat/card")
    }

    // MARK: - the model

    func testRenderCases() throws {
        let cases = try XCTUnwrap(Fixtures.json("embed.json")["render"]?.array)
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let m = try mount(c, host: Host())
            assertTexts(try texts(m), c, name)
            // Each card is its own surface: `<host>/<embed id>`.
            for n in embeds(m) {
                XCTAssertEqual(try card(m, n).id, EmbedCard.surfaceId(host: m.id, component: n.id), "\(name): surface id")
            }
        }
    }

    func testEventCases() throws {
        let cases = try XCTUnwrap(Fixtures.json("embed.json")["events"]?.array)
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let host = Host()
            let m = try mount(c, host: host)
            let painted = embeds(m)
            let which = Int(c["press"]?["embed"]?.number ?? 0)
            XCTAssertLessThan(which, painted.count, "\(name): embed")
            guard which < painted.count else { continue }
            let nested = try card(m, painted[which])
            let target = try XCTUnwrap(c["press"]?["node"]?.string)
            XCTAssertNotNil(nested.index(of: target), "\(name): \(target) inside the card")
            XCTAssertNil(m.index(of: target), "\(name): the card's ids stay inner")
            nested.press(id: target)
            m.pass()
            let heard = host.actions.map { JSONValue.object(["name": .string($0.name), "context": $0.context]) }
            XCTAssertEqual(heard, c["actions"]?.array ?? [], "\(name): actions")
            XCTAssertEqual(host.actions.allSatisfy { $0.event == EmbedCard.event && $0.componentId == painted[which].id }, true, "\(name): from the Embed's action")
            let functions = (c["functions"]?.array ?? []).compactMap { $0.string ?? $0["name"]?.string }
            XCTAssertEqual(host.calls.map(\.name), functions, "\(name): functions")
            assertTexts(try texts(m), c, name)
        }
    }

    /// A changed `surface` value remounts fresh; an Embed that leaves the
    /// surface unmounts its card.
    func testChangedSurfaceRemountsAndARemovedEmbedUnmounts() throws {
        let surface = { (text: String) -> JSONValue in
            .object(["root": .object(["id": .string("t"), "component": .string("Text"), "props": .object(["text": .string(text)])])])
        }
        let root: JSONValue = .object(["id": .string("host"), "component": .string("Stack"), "children": .array([
            .object(["id": .string("card"), "component": .string("Embed"), "props": .object(["surface": .object(["@path": .string("/card")])])]),
        ])])
        let m = try mount(.object(["root": root, "data": .object(["card": surface("First")])]), host: Host())
        XCTAssertEqual(try texts(m), ["First"])
        let first = try XCTUnwrap(m.embedModel(id: "card", props: try XCTUnwrap(m.node(id: "card")).props))
        XCTAssertEqual(m.embedValid(id: "card"), true)
        m.setData(path: "/card", value: surface("Second"))
        m.pass()
        XCTAssertEqual(try texts(m), ["Second"])
        let second = try XCTUnwrap(m.embedModel(id: "card", props: try XCTUnwrap(m.node(id: "card")).props))
        XCTAssertFalse(first === second, "a new surface value is a new card")
        m.setData(path: "/card", value: .string("not a card"))
        m.pass()
        XCTAssertEqual(try texts(m), ["This card could not be shown"])
        XCTAssertEqual(m.embedValid(id: "card"), false)
        try m.setNested(json: JSONValue.object(["id": .string("host"), "component": .string("Stack"), "children": .array([])]).json)
        m.pass()
        XCTAssertNil(m.embedValid(id: "card"), "unmounted with its Embed")
    }
}
