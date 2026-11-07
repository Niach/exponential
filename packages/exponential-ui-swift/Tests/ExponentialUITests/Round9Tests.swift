import XCTest
import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives
@testable import ExponentialUI

/// Round 9 (packages/exponential-ui/docs/round-9-contract.md): the check
/// severities (§6.6, `interactions-round1.json` `checkSeverities`,
/// `buttonChecks`), `accessibility.live` + `hidden` (§5.1) and the Markdown
/// inline profile (§9.1, `markdown-inline.json`).
@MainActor
final class Round9Tests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private final class Host: HostPlugin {
        var actions: [SurfaceActionEvent] = []
        var announced: [String] = []
        func onAction(_ event: SurfaceActionEvent) { actions.append(event) }
        func announce(text: String, live: String) { announced.append("\(live): \(text)") }
    }

    private func model(_ tree: [String: Any], data: JSONValue? = nil, host: HostPlugin? = nil, theme: String = "neutral", width: CGFloat = 390) throws -> SurfaceModel {
        let m = try makeModel("r9", theme: theme, mode: .dark, width: width)
        if let host { m.host = host }
        let json = String(data: try JSONSerialization.data(withJSONObject: tree), encoding: .utf8)!
        try m.setNested(json: json)
        if let data { m.setData(path: "", value: data) }
        m.pass()
        return m
    }

    private func form(_ children: [[String: Any]]) -> [String: Any] {
        [
            "id": "f", "component": "Form", "props": ["name": "f"],
            "on": ["submit": ["event": ["name": "submit"]], "invalid": ["event": ["name": "invalid"]]],
            "children": children + [["id": "go", "component": "Button", "props": ["label": "Send", "submit": true]]],
        ]
    }

    // MARK: - §6.6 severities

    func testCheckSeveritiesShowOneMessageAndOnlyAnErrorBlocks() throws {
        let cases = try XCTUnwrap(Fixtures.json("interactions-round1.json")["checkSeverities"]?.array)
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]!.string!
            let host = Host()
            let m = try model(form([["id": "x", "component": "Input", "props": ["label": "X", "name": "x", "value": "v", "validateOn": "submit", "checks": c["checks"]!.any]]]), data: c["data"], host: host)
            let want = c["shown"]?["severity"]?.string
            XCTAssertEqual(m.fieldChecks("x").shown?.severity, want, "\(name): shown")
            XCTAssertEqual(m.fieldChecks("x").shown?.message, c["shown"]?["message"]?.string, "\(name): shown message")
            XCTAssertEqual(m.fieldChecks("x").errors, c["errors"]!.array!.compactMap(\.string), "\(name): errors")
            XCTAssertNil(m.node(id: "x.error"), "\(name): nothing before the submit")
            m.press(id: "go")
            m.pass()
            let errors = host.actions.first { $0.name == "invalid" }?.context["errors"]?.array?.compactMap { $0["message"]?.string } ?? []
            XCTAssertEqual(errors, c["errors"]!.array!.compactMap(\.string), "\(name): errors")
            XCTAssertEqual(host.actions.contains { $0.name == "submit" }, c["submitted"]!.bool!, "\(name): submitted")
            let part = m.node(id: "x.error")
            if let want {
                XCTAssertEqual(part?.props.str("text"), c["shown"]?["message"]?.string, "\(name): message")
                XCTAssertEqual(part?.partStates, [want == "error" ? "invalid" : want], "\(name): part state")
                XCTAssertEqual(part?.a11y.role, want == "error" ? "alert" : "status", "\(name): role")
            } else {
                XCTAssertNil(part, "\(name): no message")
            }
            XCTAssertEqual(m.node(id: "x.field")?.partStates.contains("invalid"), c["invalid"]!.bool!, "\(name): field invalid")
        }
    }

    func testButtonChecksDisableOnAnErrorAlone() throws {
        let cases = try XCTUnwrap(Fixtures.json("interactions-round1.json")["buttonChecks"]?.array)
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let host = Host()
            let m = try model(["id": "root", "component": "Box", "children": [
                ["id": "b", "component": "Button", "props": ["label": "Go", "checks": c["checks"]!.any], "on": ["press": ["event": ["name": "press"]]]],
            ]], data: c["data"], host: host)
            m.press(id: "b")
            XCTAssertEqual(host.actions.isEmpty, c["disabled"]!.bool!, c["name"]!.string!)
        }
    }

    /// Same `sm` type, only the colour: `$color.warning`, `$color.mutedForeground`,
    /// on every control with an `error` part; the control keeps its border.
    func testSeverityColoursOnEveryControl() throws {
        let options: [Any] = [["label": "A", "value": "a"], ["label": "B", "value": "b"]]
        let controls: [(String, [String: Any])] = [
            ("Input", [:]), ("Textarea", [:]), ("Checkbox", [:]),
            ("Radio", ["options": options]), ("ChoiceTiles", ["options": options]), ("Select", ["options": options]),
        ]
        for theme in ["neutral", "exponential"] {
            for (component, props) in controls {
                for (severity, token) in [("warning", "warning"), ("info", "mutedForeground")] {
                    var p = props
                    p["label"] = "L"
                    p["name"] = "x"
                    p["checks"] = [["condition": ["@path": "/r"]]]
                    p["validateOn"] = "submit"
                    let plain = try model(form([["id": "x", "component": component, "props": ["label": "L", "name": "x"].merging(props) { a, _ in a }]]), theme: theme)
                    let m = try model(form([["id": "x", "component": component, "props": p]]), data: .object(["r": .object(["valid": .bool(false), "severity": .string(severity), "message": .string("Note")])]), theme: theme)
                    m.submitForm(id: "f")
                    m.pass()
                    let i = try XCTUnwrap(m.index(of: "x.error"), "\(component) \(severity)")
                    XCTAssertEqual(m.ink(i), m.color(token), "\(theme) \(component) \(severity)")
                    // No `invalid` paint anywhere on the control.
                    for n in m.nodes where !n.removed && n.id != "x.error" { XCTAssertFalse(n.partStates.contains("invalid"), "\(component): \(n.id)") }
                    for id in ["x.field", "x.trigger"] {
                        guard let a = m.index(of: id), let b = plain.index(of: id) else { continue }
                        XCTAssertEqual(m.style(a).borderColor, plain.style(b).borderColor, "\(component): the normal border")
                    }
                }
            }
        }
    }

    func testAnErrorPartStillPaintsTheErrorColour() throws {
        let m = try model(form([["id": "x", "component": "Input", "props": ["label": "L", "name": "x", "validateOn": "submit", "checks": [["condition": ["@path": "/r"], "message": "No"]]]]]), data: .object(["r": .bool(false)]))
        m.submitForm(id: "f")
        m.pass()
        let i = try XCTUnwrap(m.index(of: "x.error"))
        XCTAssertEqual(m.ink(i), m.color("destructive"))
    }

    // MARK: - §5.1 live + hidden

    func testAccessibilityLiveWinsOverTextLive() throws {
        let m = try model(["id": "root", "component": "Box", "children": [
            ["id": "a", "component": "Text", "props": ["text": "a", "live": "polite"], "accessibility": ["live": "assertive"]],
            ["id": "b", "component": "Text", "props": ["text": "b", "live": "polite"], "accessibility": ["live": "off"]],
            ["id": "c", "component": "Text", "props": ["text": "c", "live": "polite"]],
            ["id": "d", "component": "Text", "props": ["text": "d", "live": "off"]],
        ]])
        XCTAssertEqual(m.node(id: "a")?.live, "assertive")
        XCTAssertNil(m.node(id: "b")?.live)
        XCTAssertEqual(m.node(id: "c")?.live, "polite")
        XCTAssertNil(m.node(id: "d")?.live)
        func info(_ id: String) -> A11yInfo { let n = m.node(id: id)!; return A11yInfo.of(n, model: m, style: m.style(n.index)) }
        XCTAssertTrue(info("a").live)
        XCTAssertFalse(info("b").live)
        XCTAssertEqual(info("c").role, "status")
        XCTAssertNotEqual(info("d").role, "status")
    }

    func testALiveTextAndAnInlineMarkdownAnnounceTheirChanges() throws {
        let m = try model(["id": "root", "component": "Box", "children": [
            ["id": "t", "component": "Text", "props": ["text": ["@path": "/t"]], "accessibility": ["live": "assertive"]],
            ["id": "md", "component": "Markdown", "props": ["text": ["@path": "/m"], "profile": "inline"], "accessibility": ["live": "polite"]],
        ]], data: .object(["t": .string("one"), "m": .string("uno")]))
        m.setData(path: "/t", value: .string("two"))
        m.pass()
        XCTAssertEqual(m.announcement?.text, "two")
        XCTAssertEqual(m.announcement?.live, "assertive")
        m.setData(path: "/m", value: .string("dos"))
        m.pass()
        XCTAssertEqual(m.announcement?.text, "dos")
        XCTAssertEqual(m.announcement?.live, "polite")
    }

    func testFieldMessagesSpeakAlertForAnErrorStatusForAnAdvisory() throws {
        for (severity, role, live) in [("error", "alert", "assertive"), ("warning", "status", "polite"), ("info", "status", "polite")] {
            let host = Host()
            let m = try model(form([["id": "x", "component": "Input", "props": ["label": "L", "name": "x", "value": "v", "validateOn": "submit", "checks": [["condition": ["@path": "/r"]]]]]]), data: .object(["r": .object(["valid": .bool(false), "severity": .string(severity), "message": .string("Note")])]), host: host)
            m.submitForm(id: "f")
            m.pass()
            let n = try XCTUnwrap(m.node(id: "x.error"))
            XCTAssertEqual(A11yInfo.of(n, model: m, style: m.style(n.index)).role, role, severity)
            XCTAssertEqual(n.live, live, severity)
            XCTAssertTrue(host.announced.contains("\(live): Note"), "\(severity): \(host.announced)")
        }
    }

    func testAccessibilityHiddenIsADynamicBooleanAndKeepsThePaint() throws {
        for (value, hidden) in [(JSONValue.bool(true), true), (.bool(false), false), (.string("TRUE"), true), (.string("yes"), false), (.number(1), true), (.number(0), false), (.null, false)] {
            let m = try model(["id": "root", "component": "Box", "children": [
                ["id": "t", "component": "Text", "props": ["text": "Hi"], "accessibility": ["hidden": ["@path": "/h"]]],
            ]], data: .object(["h": value]))
            let n = try XCTUnwrap(m.node(id: "t"))
            XCTAssertEqual(A11yInfo.of(n, model: m, style: m.style(n.index)).hidden, hidden, "\(value)")
            XCTAssertFalse(n.hidden, "still painted")
            XCTAssertGreaterThan(m.frame(n.index).height, 0)
        }
    }

    // MARK: - §9.1 Markdown inline

    func testMarkdownInlineRecipesReplayTheFixture() throws {
        let cases = try XCTUnwrap(Fixtures.json("markdown-inline.json")["cases"]?.array)
        XCTAssertFalse(cases.isEmpty)
        let tokens = try XCTUnwrap(JSONValue(any: JSONSerialization.jsonObject(with: Data(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("themes/neutral.theme.json"))))["tokens"])
        func token(_ ref: String?) -> CGFloat? {
            var v: JSONValue? = tokens
            for key in (ref ?? "").dropFirst().split(separator: ".") { v = v?[String(key)] }
            return v?.number.map { CGFloat($0) }
        }
        let theme = try XCTUnwrap(ThemeHandle.builtin("neutral"))
        for c in cases {
            let part = c["part"]!.string!
            let variant = c["variant"]?.string
            let level = c["level"]?.number.map { Int($0) } ?? 0
            let name = "\(part) \(variant ?? "\(level)")"
            let recipes = part == "paragraph" ? MarkdownPainter.inlineRecipes(paragraph: variant!) : MarkdownPainter.inlineRecipes(heading: level)
            let want = c["recipes"]!.array!.map { r in
                MarkdownPainter.InlineRecipe(component: r["component"]!.string!, part: r["part"]!.string!, props: r["props"]!.object!)
            }
            XCTAssertEqual(recipes, want, name)
            let props: Props = ["profile": .string("inline"), "variant": .string(variant ?? "body")]
            let styles = MarkdownPainter.styles(theme: theme, mode: .dark, body: .body, props: props)
            let spec = styles.spec(of: part == "paragraph" ? .paragraph : .heading(level))
            let neutral = c["neutral"]!
            XCTAssertEqual(spec.size, token(neutral["fontSize"]?.string), "\(name) size")
            XCTAssertEqual(spec.lineHeight, token(neutral["lineHeight"]?.string), "\(name) line height")
            XCTAssertEqual(CGFloat(spec.weight), token(neutral["fontWeight"]?.string), "\(name) weight")
            XCTAssertEqual(spec.family, theme.sansFamily, "\(name) family")
            XCTAssertEqual(MarkdownPainter.inlineInk(part == "paragraph" ? .paragraph : .heading(level), props: props, model: try model(["id": "root", "component": "Box"])), theme.color("foreground", mode: .dark), name)
        }
    }

    /// A plain inline Markdown boxes exactly like the Text of its variant.
    func testAnInlineParagraphMeasuresLikeItsText() throws {
        for variant in ["body", "caption"] {
            let words = "The quick brown fox jumps over the lazy dog and keeps running along the river bank"
            let m = try model(["id": "root", "component": "Stack", "children": [
                ["id": "md", "component": "Markdown", "props": ["text": words, "profile": "inline", "variant": variant]],
                ["id": "t", "component": "Text", "props": ["text": words, "variant": variant]],
            ]], width: 200)
            XCTAssertEqual(m.frame(m.index(of: "md")!).size, m.frame(m.index(of: "t")!).size, variant)
        }
    }

    func testTheInlineProfileAnswersToNoHostDensityAndListsStayDocument() throws {
        let inline: Props = ["profile": .string("inline"), "variant": .string("caption")]
        XCTAssertNil(MarkdownPainter.props(inline, chat: true)["density"])
        XCTAssertEqual(MarkdownPainter.props([:], chat: true)["density"], .string("chat"))
        let theme = try XCTUnwrap(ThemeHandle.builtin("neutral"))
        let document = MarkdownPainter.styles(theme: theme, mode: .dark, body: .body, props: [:])
        let styles = MarkdownPainter.styles(theme: theme, mode: .dark, body: .body, props: inline)
        XCTAssertNil(document.paragraph)
        XCTAssertEqual(styles.spec(of: .quote), document.spec(of: .quote))
        XCTAssertEqual(styles.blockGap, document.blockGap)
        XCTAssertEqual(styles.headingTops, document.headingTops)
        XCTAssertEqual(styles.headingBottoms, document.headingBottoms)
    }
}
