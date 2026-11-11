import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// A host that records everything the round-1 model hands out.
@MainActor
final class Round1Host: HostPlugin {
    var actions: [SurfaceActionEvent] = []
    var inputs: [SurfaceInputEvent] = []
    var calls: [SurfaceFunctionCall] = []
    var uploads: [SurfaceUploadEvent] = []
    var announced: [(String, String)] = []
    var copied: [String] = []
    var picks: [FilePickRequest] = []
    var ownPicker = false
    func onAction(_ event: SurfaceActionEvent) { actions.append(event) }
    func onInput(_ event: SurfaceInputEvent) { inputs.append(event) }
    func onFunctionCall(_ call: SurfaceFunctionCall) { calls.append(call) }
    func openUrl(_ url: String) {}
    func onUpload(_ event: SurfaceUploadEvent) { uploads.append(event) }
    func announce(text: String, live: String) { announced.append((text, live)) }
    func copy(_ text: String) -> Bool { copied.append(text); return true }
    func pickFiles(_ request: FilePickRequest) -> Bool { picks.append(request); return ownPicker }
}

/// The round-1 MODEL contract (VAPP-100): node slots + deltas,
/// settings into the core, every OutEvent kind, host commands, keyboard
/// focus and the a11y keys, Form submit, toast timers, and the shared
/// fixtures (`interactions-round1.json`, `bind-time.json` presses) replayed
/// through `SurfaceModel` the way gpui / React consume them.
@MainActor
final class Round1ModelTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private func model(_ tree: J, host: HostPlugin? = nil, settings: SurfaceSettings? = nil, width: CGFloat = 400) throws -> SurfaceModel {
        try model(JSONValue(any: tree), host: host, settings: settings, width: width)
    }

    private func model(_ tree: JSONValue, host: HostPlugin? = nil, settings: SurfaceSettings? = nil, width: CGFloat = 400) throws -> SurfaceModel {
        var options = SurfaceOptions()
        options.mode = .light
        options.settings = settings
        let m = try SurfaceModel(id: "r1", options: options, host: host)
        m.setViewport(width: width, height: 800)
        try m.setNested(json: tree.json)
        return m
    }

    /// Trees as plain Swift values (JSONValue has no literal conformances).
    typealias J = [String: Any]

    private func obj(_ pairs: J) -> J { pairs }

    private func root(_ children: [J]) -> J {
        ["id": "root", "component": "Box", "children": children]
    }

    private func props(_ d: J) -> Props { JSONValue(any: d).object ?? [:] }

    // MARK: - interactions-round1.json

    func testNumberFieldCasesStepRoundAndShowThroughTheModel() throws {
        let f = try Fixtures.json("interactions-round1.json")
        let cases = f["numberField"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            var props = (c["props"]?.any as? J) ?? [:]
            props["label"] = "N"
            props["name"] = "n"
            props["value"] = obj(["@path": "/v"])
            let m = try model(root([obj(["id": "n", "component": "NumberField", "props": props, "on": obj(["change": obj(["event": obj(["name": "change"])])])])]))
            if let v = c["value"], !v.isNull { m.setData(path: "/v", value: v) }
            let input = try XCTUnwrap(m.index(of: "n.input"), name)
            switch c["op"]?.string {
            case "increment"?, "decrement"?:
                m.press(id: "n.\(c["op"]!.string!)")
            case "commit"?:
                m.fieldFocused(input, true)
                m.fieldEdited(input, text: c["input"]?.displayText ?? "")
                m.fieldFocused(input, false)
            default:
                break
            }
            let expect = c["expect"] ?? .null
            if c["op"]?.string != "none", expect["value"] != c["value"] {
                XCTAssertEqual(m.data["v"]?.number, expect["value"]?.number, "\(name): /v")
            }
            XCTAssertEqual(m.node(id: "n.input")?.props["text"]?.displayText, expect["text"]?.displayText, "\(name): text")
            XCTAssertEqual(m.fieldText(input), expect["text"]?.displayText, "\(name): the host field echoes the core's text")
        }
    }

    func testCheckCasesFailFocusAndAnnounceThroughTheModel() throws {
        let f = try Fixtures.json("interactions-round1.json")
        let cases = f["checks"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let host = Round1Host()
            let tree = obj([
                "id": "f", "component": "Form", "props": obj(["name": "f"]),
                "on": obj(["submit": obj(["event": obj(["name": "submit"])]), "invalid": obj(["event": obj(["name": "invalid"])])]),
                "children": [
                    obj(["id": "agree", "component": "Checkbox", "props": obj(["label": "I agree", "name": "agree", "checked": obj(["@path": "/agreed"]), "checks": c["checks"]?.any ?? [Any]()])]),
                    obj(["id": "go", "component": "Button", "props": obj(["label": "Send", "submit": true])]),
                ],
            ])
            let m = try model(tree, host: host)
            m.setData(path: "", value: c["data"] ?? .object([:]))
            let failing = c["failing"]?.array?.compactMap(\.string) ?? []
            XCTAssertEqual(m.failingChecks("agree"), failing, "\(name): failing_checks")
            m.press(id: "go")
            let invalid = host.actions.first { $0.name == "invalid" }?.context["errors"]?.array?.compactMap { $0["message"]?.string } ?? []
            XCTAssertEqual(invalid, failing, "\(name): invalid errors")
            XCTAssertEqual(host.actions.contains { $0.name == "submit" }, c["submitted"]?.bool ?? false, "\(name): submitted")
            if !failing.isEmpty {
                // §3 Form: focus moves to the first invalid field (keyboard
                // focus: the ring shows) and `invalidFields` is announced.
                XCTAssertEqual(m.focusRequest?.id, "agree", name)
                XCTAssertEqual(m.focusedId, "agree", name)
                XCTAssertTrue(m.keyboardFocus, name)
                XCTAssertEqual(m.announcement?.live, "assertive", name)
                XCTAssertFalse(m.announcement?.text.isEmpty ?? true, name)
                XCTAssertEqual(host.announced.last?.1, "assertive", name)
                XCTAssertTrue(m.node(id: "agree")?.invalid ?? false, "\(name): the field shows invalid")
            }
        }
    }

    func testTableSortCasesOrderRowsThroughTheModel() throws {
        let f = try Fixtures.json("interactions-round1.json")
        for c in f["tableSort"]?.array ?? [] {
            let name = c["name"]?.string ?? "?"
            var column = (c["column"]?.any as? J) ?? [:]
            column["label"] = "V"
            column["sortable"] = true
            let sort: J = ["key": c["column"]?["key"]?.any ?? NSNull(), "direction": c["direction"]?.any ?? NSNull()]
            let m = try model(root([obj(["id": "t", "component": "Table", "props": obj(["columns": [column], "rows": c["rows"]?.any ?? [Any](), "sort": sort])])]))
            let body = try XCTUnwrap(m.node(id: "t.body"), name)
            let order = body.children.compactMap { m.node($0) }.filter { $0.part == "row" }.map { Int($0.props["index"]?.number ?? -1) }
            XCTAssertEqual(order, c["order"]?.array?.compactMap { $0.number.map { Int($0) } }, name)
        }
    }

    /// VAPP-146: "" is an ordinary option value (the trigger shows the
    /// option holding it; picking the option that holds the value fires
    /// nothing), through the core's popup and the native `selectPick`.
    func testSelectCasesTakeAnEmptyOptionValueThroughTheModel() throws {
        let f = try Fixtures.json("interactions-round1.json")
        let cases = f["select"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        let options: [J] = [["label": "Any", "value": ""], ["label": "Open", "value": "open"], ["label": "Done", "value": "done"]]
        for native in [false, true] {
            for c in cases {
                let name = "\(c["name"]?.string ?? "?")\(native ? " (selectPick)" : "")"
                let host = Round1Host()
                let m = try model(root([obj(["id": "s", "component": "Select", "props": obj(["label": "Status", "name": "s", "options": options, "value": obj(["@path": "/v"])]), "on": obj(["change": obj(["event": obj(["name": "change"])])])])]), host: host)
                if let v = c["value"], !v.isNull { m.setData(path: "/v", value: v) }
                for i in c["presses"]?.array?.compactMap(\.number).map({ Int($0) }) ?? [] {
                    if native {
                        m.selectPick(try XCTUnwrap(m.index(of: "s.trigger"), name), value: JSONValue(any: options[i]["value"]!))
                    } else {
                        m.press(id: "s.trigger")
                        XCTAssertNotNil(m.index(of: "s.item.\(i)"), "\(name): option \(i)")
                        m.press(id: "s.item.\(i)")
                    }
                }
                let expect = c["expect"] ?? .null
                // The open/close `change {open}` is not a pick.
                let changes = host.actions.filter { $0.name == "change" }.compactMap { ($0.payload?["value"] ?? $0.context["value"])?.string }
                XCTAssertEqual(changes, expect["changes"]?.array?.compactMap(\.string), "\(name): changes")
                XCTAssertEqual(m.data["v"] ?? .null, expect["value"] ?? .null, "\(name): /v")
                XCTAssertEqual(m.node(id: "s.trigger")?.props["text"]?.displayText, expect["shows"]?.string, "\(name): the trigger shows")
            }
        }
    }

    /// interactions-round1.json `overlayAnchor` (#EXP-1739): an open
    /// anchored Popover closes once its anchor scrolls, moves or leaves the
    /// tree. The core raises the close during the pass; the model's late
    /// events take it and the next pass drops the layer.
    func testOverlayAnchorCasesCloseOnScrollMoveAndRemoval() throws {
        let f = try Fixtures.json("interactions-round1.json")
        let fx = try XCTUnwrap(f["overlayAnchor"])
        let base = try XCTUnwrap(fx["surface"]?.object)
        let cases = fx["cases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let m = try model(.object(base))
            XCTAssertTrue(m.layers.contains { $0.owner == "pop" }, "\(name): opens against its anchor")
            let step = c["step"] ?? .null
            var children = base["children"]?.array ?? []
            if step["insertAbove"]?.bool == true, let above = fx["above"] { children.insert(above, at: 0) }
            if step["removeAnchor"]?.bool == true { children.removeAll { $0["id"]?.string == "target" } }
            if children != base["children"]?.array {
                var tree = base
                tree["children"] = .array(children)
                _ = try m.setNested(json: JSONValue.object(tree).json)
            }
            if let list = step["scroll"]?.string { m.scrollTo(id: list, offset: CGPoint(x: 0, y: step["y"]?.number ?? 0)) }
            if step["relayout"]?.bool == true { m.setViewport(width: 400, height: 800) }
            XCTAssertEqual(m.layers.contains { $0.owner == "pop" }, c["after"]?.bool, name)
        }
    }

    // MARK: - bind-time.json presses (two-way binding through the core)

    func testBindTimePressesWriteTheBoundPathThroughTheModel() throws {
        let f = try Fixtures.json("bind-time.json")
        var replayed = 0
        for c in (f["cases"]?.array ?? []) + (f["extra"]?.array ?? []) {
            let name = c["name"]?.string ?? "?"
            guard let input = c["input"] else { continue }
            for d in c["datasets"]?.array ?? [] {
                for p in d["presses"]?.array ?? [] {
                    guard let id = p["id"]?.string, let expected = p["outcome"]?["data"] else { continue }
                    let host = Round1Host()
                    let m = try model(input, host: host, width: 1024)
                    m.setData(path: "", value: d["data"] ?? .object([:]))
                    // The fixture runs the ACTION; a renderer never presses a
                    // disabled control (Pagination's prev at page 1).
                    guard let i = m.index(of: id), !m.isDisabled(i) else { continue }
                    m.press(id: id)
                    XCTAssertEqual(m.data, expected, "\(name): press \(id) on \(d["data"]?.json ?? "")")
                    replayed += 1
                }
            }
        }
        // Round 3: a Section writes `open` back only when collapsible, so the
        // bound-open case lost its press (29 presses remain).
        XCTAssertGreaterThan(replayed, 25, "the presses replayed through the model")
    }

    // MARK: - settings → core

    func testSettingsReachTheCore() throws {
        var tree = root([
            obj(["id": "b", "component": "Button", "props": obj(["label": "Go"])]),
            obj(["id": "t", "component": "Text", "props": obj(["text": "Hello"])]),
        ])
        tree["style"] = ["flexDirection": "column", "alignItems": "flex-start"]
        let m = try model(tree)
        let button = m.frame(m.index(of: "b")!).height
        let text = m.frame(m.index(of: "t")!).height
        XCTAssertEqual(m.direction, "ltr")

        // Density scales controls (contract §5).
        m.setDensity(.compact)
        XCTAssertLessThan(m.frame(m.index(of: "b")!).height, button, "compact controls are shorter")
        m.setDensity(.comfortable)
        XCTAssertGreaterThan(m.frame(m.index(of: "b")!).height, button, "comfortable controls are taller")
        m.setDensity(.default)
        XCTAssertEqual(m.frame(m.index(of: "b")!).height, button, accuracy: 0.5)

        // Font scale: every resolved type size scales, the measurer re-runs.
        m.setFontScale(2)
        XCTAssertGreaterThan(m.frame(m.index(of: "t")!).height, text * 1.6)
        XCTAssertEqual(m.textStyle(m.index(of: "t")!).fontSize, 28, accuracy: 0.5)
        m.setFontScale(nil)
        var p = m.platform
        p.fontScale = 2
        m.setPlatform(p)
        XCTAssertGreaterThan(m.frame(m.index(of: "t")!).height, text * 1.6, "nil = Dynamic Type")
        p.fontScale = 1
        m.setPlatform(p)

        // An RTL locale → the core's direction (contract §4).
        m.setLocale("ar")
        XCTAssertEqual(m.direction, "rtl")
        XCTAssertTrue(m.isRTL)
        XCTAssertEqual(m.layoutDirection, .rightToLeft)
        m.setLocale("en-US")
        XCTAssertEqual(m.layoutDirection, .leftToRight)

        // `mode: system` follows the platform live.
        m.setModeSetting(.system)
        p.prefersDark = true
        m.setPlatform(p)
        XCTAssertEqual(m.mode, .dark)
        p.prefersDark = false
        m.setPlatform(p)
        XCTAssertEqual(m.mode, .light)

        // High contrast (system flag): the painters query the effective theme.
        let base = m.theme
        m.setContrast(.system)
        p.highContrast = true
        m.setPlatform(p)
        XCTAssertNotNil(m.theme)
        XCTAssertFalse(m.theme === base, "the effective (contrast) theme")
        p.highContrast = false
        m.setPlatform(p)
        XCTAssertTrue(m.theme === base)

        // The resolved settings round-trip through the core.
        let s = m.surface.settings()
        XCTAssertEqual(s.locale, "en-US")
        XCTAssertEqual(s.mode, "system")
        XCTAssertEqual(s.hoverCloseMs, 150)
        XCTAssertNotNil(s.today)
    }

    // MARK: - keyboard

    func testTabOrderRovingArrowsRtlAndEnter() throws {
        let host = Round1Host()
        let tree = root([
            obj(["id": "a", "component": "Button", "props": obj(["label": "A"]), "on": obj(["press": obj(["event": obj(["name": "a"])])])]),
            obj(["id": "off", "component": "Button", "props": obj(["label": "Off", "disabled": true])]),
            obj(["id": "tabs", "component": "Tabs", "props": obj(["tabs": [obj(["label": "One", "value": "1"]), obj(["label": "Two", "value": "2"]), obj(["label": "Three", "value": "3"])], "value": "1"]), "children": [
                obj(["id": "p1", "component": "Text", "props": obj(["text": "one"])]),
                obj(["id": "p2", "component": "Text", "props": obj(["text": "two"])]),
                obj(["id": "p3", "component": "Text", "props": obj(["text": "three"])]),
            ]]),
            obj(["id": "b", "component": "Button", "props": obj(["label": "B"])]),
        ])
        let m = try model(tree, host: host)
        let order = m.focusOrder().compactMap { m.node($0)?.id }
        XCTAssertEqual(order, ["a", "tabs.tab.0", "b"], "paint order, disabled skipped, ONE stop per tab list")
        XCTAssertTrue(m.handleKey(key: "tab"))
        XCTAssertEqual(m.focusedId, "a")
        XCTAssertTrue(m.focusVisible("a"), "keyboard focus shows the ring")
        XCTAssertTrue(m.states(of: "a").contains("focus-visible"))
        m.handleKey(key: "tab", shift: true)
        XCTAssertEqual(m.focusedId, "b", "Shift+Tab wraps")
        XCTAssertFalse(m.focusVisible("a"))
        m.handleKey(key: "tab")
        XCTAssertEqual(m.focusedId, "a")
        // Enter / Space press the focused node.
        XCTAssertTrue(m.handleKey(key: "enter"))
        XCTAssertEqual(host.actions.map(\.name), ["a"])
        // Tabs: arrows move AND activate, wrapping; Home / End.
        m.handleKey(key: "tab")
        XCTAssertEqual(m.focusedId, "tabs.tab.0")
        m.handleKey(key: "right")
        XCTAssertEqual(m.focusedId, "tabs.tab.1")
        XCTAssertTrue(m.node(id: "tabs.tab.1")?.selected ?? false)
        m.handleKey(key: "end")
        XCTAssertEqual(m.focusedId, "tabs.tab.2")
        m.handleKey(key: "right")
        XCTAssertEqual(m.focusedId, "tabs.tab.0", "wraps")
        // RTL flips the horizontal arrows.
        m.setLocale("he")
        XCTAssertTrue(m.isRTL)
        m.handleKey(key: "left")
        XCTAssertEqual(m.focusedId, "tabs.tab.1", "ArrowLeft = forward under rtl")
        // A pointer press ends keyboard mode (no ring).
        m.pressDown("b")
        XCTAssertFalse(m.keyboardFocus)
        m.pressUp("b")
    }

    func testDialogFocusTrapEscapeAndReturn() throws {
        let tree = root([
            obj(["id": "dlg", "component": "Dialog", "props": obj(["title": "Edit"]), "slots": obj(["trigger": obj(["id": "open", "component": "Button", "props": obj(["label": "Open"])])]), "children": [
                obj(["id": "x", "component": "Button", "props": obj(["label": "X"])]),
                obj(["id": "y", "component": "Button", "props": obj(["label": "Y"])]),
            ]]),
            obj(["id": "after", "component": "Button", "props": obj(["label": "After"])]),
        ])
        let m = try model(tree)
        guard let trigger = m.trigger(of: "dlg") else { throw XCTSkip("no Dialog trigger wiring in this catalog") }
        m.requestFocus(trigger.id, keyboard: true)
        XCTAssertTrue(m.handleKey(key: "enter"))
        XCTAssertEqual(m.modalLayers.count, 1)
        let focused = try XCTUnwrap(m.focusedId.flatMap { m.node(id: $0) })
        XCTAssertGreaterThan(focused.layer, 0, "focus moved into the dialog")
        // The trap: Tab cycles inside the dialog only.
        let inside = Set(m.focusOrder())
        for _ in 0..<5 {
            m.handleKey(key: "tab")
            XCTAssertTrue(inside.contains(m.focusedIndex ?? -1))
        }
        XCTAssertTrue(m.handleKey(key: "escape"))
        XCTAssertTrue(m.modalLayers.isEmpty)
        XCTAssertEqual(m.focusedId, trigger.id, "closing returns focus to the trigger")
        XCTAssertEqual(m.focusRequest?.id, trigger.id)
    }

    func testFieldKeysSegmentedKeysAndAFocusedToast() throws {
        let tree = root([
            obj(["id": "n", "component": "NumberField", "props": obj(["label": "N", "name": "n", "value": obj(["@path": "/v"]), "step": 1, "max": 100])]),
            obj(["id": "g", "component": "Segmented", "props": obj(["items": [obj(["label": "A", "value": "a"]), obj(["label": "B", "value": "b"]), obj(["label": "C", "value": "c"])], "value": "a"])]),
            obj(["id": "toast", "component": "Toast", "props": obj(["title": "Saved", "duration": 0])]),
        ])
        let m = try model(tree)
        m.setData(path: "/v", value: .number(5))
        let input = try XCTUnwrap(m.index(of: "n.input"))
        XCTAssertTrue(m.node(input)!.isTextField)
        XCTAssertTrue(m.fieldKey(input, key: "up"))
        XCTAssertEqual(m.data["v"]?.number, 6)
        XCTAssertTrue(m.fieldKey(input, key: "down", shift: true), "Shift = ×10")
        XCTAssertEqual(m.data["v"]?.number, -4)
        XCTAssertFalse(m.fieldKey(input, key: "left"), "the field keeps its other keys")
        // Segmented: arrows move the roving item (wrap), Space toggles it.
        let g = try XCTUnwrap(m.index(of: "g"))
        m.requestFocus("g", keyboard: true)
        XCTAssertEqual(m.segmentedFocusIndex(g), 0)
        m.handleKey(key: "left")
        XCTAssertEqual(m.segmentedFocusIndex(g), 2, "wraps")
        m.handleKey(key: "space")
        XCTAssertEqual(m.segmentedValues(g), ["c"])
        // Escape on a focused toast dismisses it; a sticky toast has no timer.
        XCTAssertNil(m.toastRemainingMs("toast"))
        if let close = m.nodes.first(where: { !$0.removed && $0.owner == "toast" && $0.isFocusable }) {
            m.requestFocus(close.id, keyboard: true)
            XCTAssertTrue(m.toastPaused("toast"), "focus pauses a toast")
            XCTAssertTrue(m.handleKey(key: "escape"))
            XCTAssertTrue(m.toasts.isEmpty)
        }
    }

    func testCalendarAndRovingHelpers() {
        XCTAssertEqual(SurfaceModel.arrowStep("right", rtl: false, vertical: false), 1)
        XCTAssertEqual(SurfaceModel.arrowStep("right", rtl: true, vertical: false), -1)
        XCTAssertEqual(SurfaceModel.arrowStep("left", rtl: true, vertical: false), 1)
        XCTAssertNil(SurfaceModel.arrowStep("down", rtl: false, vertical: false))
        XCTAssertEqual(SurfaceModel.arrowStep("down", rtl: true, vertical: true), 1)
        XCTAssertEqual(SurfaceModel.rovingTarget([4, 7, 9], current: 9, key: "right", step: 1), 4)
        XCTAssertEqual(SurfaceModel.rovingTarget([4, 7, 9], current: 4, key: "left", step: -1), 9)
        XCTAssertEqual(SurfaceModel.rovingTarget([4, 7, 9], current: 7, key: "home", step: nil), 4)
        XCTAssertEqual(SurfaceModel.rovingTarget([4, 7, 9], current: 7, key: "end", step: nil), 9)
        XCTAssertEqual(SurfaceModel.nextFocus([3, 5, 9], current: nil, backwards: true), 9)
        XCTAssertEqual(SurfaceModel.nextFocus([3, 5, 9], current: 4, backwards: false), 3)
        let z = CivilDate.days(2026, 10, 8)
        XCTAssertTrue(CivilDate.civil(z) == (2026, 10, 8))
        XCTAssertTrue(CivilDate.civil(z + 31) == (2026, 11, 8))
        XCTAssertTrue(CivilDate.addMonths(2026, 1, -1) == (2025, 12))
        XCTAssertTrue(CivilDate.addMonths(2026, 12, 1) == (2027, 1))
        XCTAssertEqual(CivilDate.daysInMonth(2028, 2), 29)
        XCTAssertEqual(CivilDate.iso(2026, 3, 1), "2026-03-01")
        XCTAssertEqual(SurfaceModel.chartPointCount(props(["kind": "pie", "series": [["values": [1, 2, 3]]]])), 3)
        XCTAssertEqual(SurfaceModel.chartPointCount(props(["categories": ["a", "b"], "series": [["values": [1, 2, 3, 4]]]])), 4)
    }

    func testDatePickerGridKeys() throws {
        let tree = root([obj(["id": "d", "component": "DatePicker", "props": obj(["label": "When", "name": "when", "value": "2026-10-14"])])])
        let m = try model(tree, settings: SurfaceSettings(mode: .light, today: "2026-10-08"))
        m.setOpen("d", true)
        guard let day = m.dayNode(owner: "d", iso: "2026-10-14") else { throw XCTSkip("no calendar grid in this core") }
        m.requestFocus(m.node(day)!.id, keyboard: true)
        m.handleKey(key: "right")
        XCTAssertEqual(m.focusedIndex.flatMap { m.node($0) }?.props.str("date"), "2026-10-15")
        m.handleKey(key: "down")
        XCTAssertEqual(m.focusedIndex.flatMap { m.node($0) }?.props.str("date"), "2026-10-22")
        m.handleKey(key: "pagedown")
        XCTAssertEqual(m.focusedIndex.flatMap { m.node($0) }?.props.str("date"), "2026-11-22", "PageDown = next month, the calendar pages")
        m.handleKey(key: "pageup", shift: true)
        XCTAssertEqual(m.focusedIndex.flatMap { m.node($0) }?.props.str("date"), "2025-11-22", "Shift+PageUp = previous year")
        XCTAssertTrue(m.handleKey(key: "enter"))
        XCTAssertEqual(m.node(id: "d")?.props["value"]?.string, "2025-11-22", "Enter chooses (unbound: the core keeps it)")
        XCTAssertFalse(m.layers.contains { $0.owner == "d" }, "and closes")
    }

    // MARK: - commands, announce, copy, files, toasts, hover

    func testHostCommandsFocusAnnounceAndScrollIntoView() throws {
        let host = Round1Host()
        var rows: [J] = []
        for i in 0..<200 { rows.append(obj(["id": "r\(i)", "component": "Text", "props": obj(["text": "Row \(i)"])])) }
        let tree = root([
            obj(["id": "go", "component": "Button", "props": obj(["label": "Go"])]),
            obj(["id": "list", "component": "List", "style": obj(["height": 300, "overflow": "scroll"]), "children": rows]),
        ])
        let m = try model(tree, host: host)
        m.command(.focus(id: "go"))
        XCTAssertEqual(m.focusRequest?.id, "go")
        XCTAssertEqual(m.focusedId, "go")
        m.command(.announce(text: "Saved", live: "assertive"))
        XCTAssertEqual(m.announcement?.text, "Saved")
        XCTAssertEqual(m.announcement?.live, "assertive")
        XCTAssertEqual(host.announced.last?.0, "Saved")
        let listIndex = try XCTUnwrap(m.index(of: "list"))
        XCTAssertNil(m.scrollJumps[listIndex] as ScrollJump?)
        m.command(.scrollIntoView(id: "r150"))
        let jump = try XCTUnwrap(m.scrollJumps[listIndex], "the native scroll view is told to follow")
        XCTAssertGreaterThan(jump.offset.y, 1000)
        XCTAssertNotNil(m.index(of: "r150"), "the row's window is laid out")
        // A report from the scroll view itself is no jump.
        let serial = jump.serial
        m.scrollReported(listIndex, offset: CGPoint(x: 0, y: jump.offset.y + 10))
        XCTAssertEqual(m.scrollJumps[listIndex]?.serial, serial)
    }

    func testCodeBlockCopyGoesToTheHostAndAnnounces() throws {
        let host = Round1Host()
        let m = try model(root([obj(["id": "cb", "component": "CodeBlock", "props": obj(["code": "let x = 1", "language": "swift"])])]), host: host)
        let copy = try XCTUnwrap(m.index(of: "cb.copy"))
        m.press(copy)
        XCTAssertEqual(host.copied, ["let x = 1"])
        XCTAssertEqual(m.announcement?.live, "polite")
        XCTAssertFalse(m.announcement?.text.isEmpty ?? true)
    }

    /// EXP-1324: the copy part carries `text` (ANSI escapes stripped): the
    /// clipboard gets that, never the raw escapes.
    func testAnAnsiCodeBlockCopiesItsPlainText() throws {
        let host = Round1Host()
        let m = try model(root([obj(["id": "cb", "component": "CodeBlock", "props": obj(["code": "\u{1b}[31mred\u{1b}[0m ok", "language": "ansi"])])]), host: host)
        let copy = try XCTUnwrap(m.index(of: "cb.copy"))
        XCTAssertEqual(m.node(copy)?.props.str("text"), "red ok")
        m.press(copy)
        XCTAssertEqual(host.copied, ["red ok"])
    }

    func testFilePickAndUpload() throws {
        let host = Round1Host()
        let tree = root([obj(["id": "f", "component": "FileUpload", "props": obj(["label": "Files", "name": "files", "accept": "image/*,.pdf", "multiple": true]), "on": obj(["upload": obj(["event": obj(["name": "upload"])])])])])
        let m = try model(tree, host: host)
        let zone = try XCTUnwrap(m.index(of: "f.dropzone"))
        m.press(zone)
        XCTAssertEqual(host.picks.map(\.componentId), ["f"])
        XCTAssertEqual(m.filePickRequest?.componentId, "f", "no host picker: the surface presents .fileImporter")
        XCTAssertEqual(m.filePickRequest?.multiple, true)
        XCTAssertEqual(m.filePickRequest?.accept, "image/*,.pdf")
        XCTAssertTrue(SurfaceModel.contentTypes(accept: "image/*,.pdf").contains(.image))
        XCTAssertTrue(SurfaceModel.contentTypes(accept: "image/*,.pdf").contains(.pdf))
        m.filePickFinished()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("r1-\(UUID().uuidString).png")
        try Data([0x89, 0x50, 0x4E, 0x47, 1, 2, 3]).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        m.filesPicked(componentId: "f", urls: [url])
        XCTAssertEqual(host.uploads.count, 1)
        XCTAssertEqual(host.uploads[0].name, "files")
        XCTAssertEqual(host.uploads[0].files.first?.size, 7)
        XCTAssertEqual(host.uploads[0].files.first?.type, "image/png")
        XCTAssertEqual(host.uploads[0].files.first?.data?.count, 7)
        let upload = try XCTUnwrap(host.actions.first { $0.name == "upload" })
        XCTAssertEqual(upload.context["files"]?.array?.first?["name"]?.string, url.lastPathComponent)
        XCTAssertEqual(upload.context["files"]?.array?.first?["type"]?.string, "image/png")
    }

    func testToastTimerRunsAndPausesWhileHovered() async throws {
        let host = Round1Host()
        let toast = { (id: String) in
            self.obj(["id": id, "component": "Toast", "props": self.obj(["title": "Saved", "duration": 150]), "on": self.obj(["dismiss": self.obj(["event": self.obj(["name": "dismiss"])])])])
        }
        let m = try model(root([toast("t1"), toast("t2")]), host: host)
        XCTAssertEqual(Set(m.toasts.map(\.id)), ["t1", "t2"])
        XCTAssertTrue(m.layers.contains { $0.isToast })
        m.toastHover(id: "t2", true)
        XCTAssertTrue(m.toastPaused("t2"))
        try await eventually { m.toasts.map(\.id) == ["t2"] }
        XCTAssertEqual(m.toasts.map(\.id), ["t2"], "t1 ran out; the hovered t2 is held")
        XCTAssertEqual(host.actions.filter { $0.name == "dismiss" }.map(\.componentId), ["t1"])
        m.toastHover(id: "t2", false)
        try await eventually { m.toasts.isEmpty }
        XCTAssertTrue(m.toasts.isEmpty)
        XCTAssertFalse(m.layers.contains { $0.isToast })
    }

    func testHoverOpensATooltipAfterTheDelayAndTheCoreClosesIt() async throws {
        let tree = root([obj(["id": "tip", "component": "Tooltip", "props": obj(["content": "Hint"]), "children": [
            obj(["id": "anchor", "component": "Button", "props": obj(["label": "?"])]),
        ]])])
        let m = try model(tree)
        guard let trigger = m.trigger(of: "tip") else { throw XCTSkip("no tooltip trigger") }
        m.setHover(id: trigger.id, true)
        XCTAssertFalse(m.layers.contains { $0.kind == "Tooltip" }, "the platform delay first")
        try await eventually { m.layers.contains { $0.kind == "Tooltip" } }
        XCTAssertTrue(m.layers.contains { $0.kind == "Tooltip" })
        m.setHover(id: trigger.id, false)
        try await eventually { !m.layers.contains { $0.kind == "Tooltip" } }
        XCTAssertFalse(m.layers.contains { $0.kind == "Tooltip" }, "the core's hoverTimer closed it")
    }

    func testEnterInASingleLineFieldSubmitsItsForm() throws {
        let host = Round1Host()
        let tree = obj([
            "id": "f", "component": "Form", "props": obj(["name": "f"]),
            "on": obj(["submit": obj(["event": obj(["name": "submit"])])]),
            "children": [obj(["id": "email", "component": "Input", "props": obj(["label": "Email", "name": "email"])])],
        ])
        let m = try model(tree, host: host)
        let field = try XCTUnwrap(m.index(of: "email.field"))
        m.fieldFocused(field, true)
        m.fieldEdited(field, text: "a@b.c")
        m.fieldCommitted(field)
        let submit = try XCTUnwrap(host.actions.first { $0.name == "submit" })
        XCTAssertEqual(submit.context["values"]?["email"]?.string, "a@b.c")
        XCTAssertEqual(host.inputs.last?.kind, .commit)
    }

    // MARK: - node slots

    func testDeltaPatchedSlotsMatchAFullRead() throws {
        let m = try model(root([
            obj(["id": "a", "component": "Text", "props": obj(["text": "A"])]),
            obj(["id": "b", "component": "Text", "props": obj(["text": "B"])]),
        ]))
        try m.setNested(json: JSONValue(any: root([
            obj(["id": "b", "component": "Text", "props": obj(["text": "B2"])]),
            obj(["id": "c", "component": "Button", "props": obj(["label": "C"])]),
        ])).json)
        XCTAssertNil(m.index(of: "a"))
        XCTAssertEqual(m.node(id: "b")?.props.str("text"), "B2")
        XCTAssertNotNil(m.node(id: "c"))
        let full = m.surface.nodes().filter { !$0.removed }.map { "\($0.index):\($0.id)" }.sorted()
        let ours = m.nodes.filter { !$0.removed }.map { "\($0.index):\($0.id)" }.sorted()
        XCTAssertEqual(ours, full)
        for n in m.nodes { XCTAssertEqual(m.nodes[n.index].index, n.index, "slots are stable") }
        XCTAssertEqual(m.rootIndex.flatMap { m.node($0) }?.id, "root")
        XCTAssertEqual(m.node(m.rootIndex!)?.children.compactMap { m.node($0)?.id }, ["b", "c"], "children in paint order")
    }

    // MARK: - interactions-round1.json fieldWrites (#VAPP-288)

    /// An Input `draft` bound to /draft and a Button `save` whose press is
    /// `functionCall set`: a host write (setData) is an echo a focused field
    /// ignores; an action's `set` (a programmatic write: the write epoch
    /// moves) lands even while focused, the outstanding edit dropped.
    private func fieldWritesModel(pressPath: String?, pressValue: JSONValue?, seed: JSONValue, host: HostPlugin? = nil) throws -> SurfaceModel {
        var save: J = ["id": "save", "component": "Button", "props": obj(["label": "Save"])]
        if let pressPath {
            save["on"] = obj(["press": obj(["functionCall": obj(["@call": "set", "args": obj(["path": pressPath, "value": pressValue?.any ?? NSNull()])])])])
        }
        let m = try model(root([obj(["id": "draft", "component": "Input", "props": obj(["label": "Draft", "name": "draft", "value": obj(["@path": "/draft"])])]), save]), host: host)
        m.setData(path: "/draft", value: seed)
        return m
    }

    func testFieldWritesCasesKeepTheEchoRuleAndAdoptProgrammaticSets() async throws {
        let cases = try Fixtures.json("interactions-round1.json")["fieldWrites"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let m = try fieldWritesModel(pressPath: c["pressPath"]?.string, pressValue: c["pressValue"], seed: c["seed"] ?? .string(""), host: Round1Host())
            let field = try XCTUnwrap(m.index(of: "draft.field"), name)
            XCTAssertNotNil(m.field(field), "\(name): field state")
            for step in c["steps"]?.array ?? [] {
                switch step["op"]?.string {
                case "focus"?:
                    m.fieldFocused(field, true)
                case "blur"?:
                    m.fieldFocused(field, false)
                case "type"?:
                    m.fieldEdited(field, text: step["text"]?.string ?? "")
                    try await eventually { m.field(field)?.pendingChange == false }
                case "hostWrite"?:
                    m.setData(path: step["path"]?.string ?? "", value: step["value"])
                case "press"?:
                    m.press(id: "save")
                default:
                    XCTFail("\(name): unknown step \(step.json)")
                }
            }
            let expect = c["expect"] ?? .null
            XCTAssertEqual(m.fieldText(field), expect["text"]?.string, "\(name): text")
            if let data = expect["data"] {
                XCTAssertEqual(m.data["draft"] ?? .null, data, "\(name): /draft")
            }
        }
    }

    /// An action's `set` lands while the edit is still inside the debounce:
    /// the press sends the edit first, then the set (back to the pre-edit
    /// value) wins, the late `change` never fires.
    func testASetBackToThePreEditValueLandsMidDebounce() async throws {
        let host = Round1Host()
        let m = try fieldWritesModel(pressPath: "/draft", pressValue: .string("x"), seed: .string("x"), host: host)
        let field = try XCTUnwrap(m.index(of: "draft.field"))
        m.fieldFocused(field, true)
        m.fieldEdited(field, text: "xyz")
        let generation = try XCTUnwrap(m.field(field)).writeGeneration
        m.press(id: "save")
        XCTAssertEqual(m.fieldText(field), "x")
        XCTAssertEqual(m.data["draft"], .string("x"))
        XCTAssertGreaterThan(try XCTUnwrap(m.field(field)).writeGeneration, generation, "the view reloads its text")
        XCTAssertFalse(try XCTUnwrap(m.field(field)).pendingChange)
        let sent = host.inputs.count
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(host.inputs.count, sent, "no late change")
        XCTAssertEqual(m.data["draft"], .string("x"))
    }

    func testOnlyProgrammaticWritesBumpTheWriteEpoch() throws {
        let m = try fieldWritesModel(pressPath: nil, pressValue: nil, seed: .string(""))
        let epoch = m.surface.writeEpoch()
        m.setData(path: "/draft", value: .string("host"))
        XCTAssertEqual(m.surface.writeEpoch(), epoch, "setData is host data")
        m.assignData(path: "/draft", value: .string("set"))
        XCTAssertGreaterThan(m.surface.writeEpoch(), epoch, "assignData is a programmatic write")
        XCTAssertEqual(m.data["draft"], .string("set"))
    }

    // MARK: - interactions-round1.json boxHover (#EDE-65)

    /// The pointer entering a Box fires its `on.hover` (no payload), leaving
    /// fires nothing, a pressable Box still presses: `boxHover` is what the
    /// painter's `onHover` calls.
    /// #FEED-201: `tilePress` (StatTile and Card presses) has the same shape.
    func testBoxHoverCasesFireOnEnteringThroughTheModel() throws {
        for key in ["boxHover", "tilePress"] {
        let f = try XCTUnwrap(Fixtures.json("interactions-round1.json")[key])
        let cases = f["cases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let host = Round1Host()
            let m = try model(f["surface"]!, host: host)
            for step in c["steps"]?.array ?? [] {
                if let id = step["enter"]?.string {
                    m.boxHover(id: id, true)
                } else if let id = step["leave"]?.string {
                    m.boxHover(id: id, false)
                } else if let id = step["press"]?.string {
                    m.press(id: id)
                } else {
                    XCTFail("\(name): unknown step \(step.json)")
                }
            }
            let want = c["events"]?.array ?? []
            XCTAssertEqual(host.actions.map(\.name), want.compactMap { $0["name"]?.string }, "\(name): names")
            XCTAssertEqual(host.actions.map(\.context), want.map { $0["context"] ?? .object([:]) }, "\(name): contexts")
        }
        }
    }

    // MARK: - interactions-round1.json pickerPick (#EXP-1760, #EXP-1775)

    /// Every pickerPick case through the MODEL: IconPicker / ColorPicker are
    /// macros over a Popover; the trigger (or a child trigger) opens it, a
    /// pick writes /value, fires `change` {value} and closes it; an inline
    /// ColorPicker picks with no popover. The actions fired {name, context},
    /// /value and whether the macro root's Popover is open.
    func testPickerPickCasesOpenPickAndCloseThroughTheModel() throws {
        let f = try XCTUnwrap(Fixtures.json("interactions-round1.json")["pickerPick"])
        let surfaces = try XCTUnwrap(f["surfaces"]?.object)
        let cases = f["cases"]?.array ?? []
        XCTAssertEqual(cases.count, 7)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let surface = try XCTUnwrap(surfaces[c["surface"]?.string ?? ""], name)
            let rootId = try XCTUnwrap(surface["id"]?.string, name)
            let host = Round1Host()
            let m = try model(surface, host: host)
            for (key, value) in c["data"]?.object ?? [:] { m.setData(path: "/\(key)", value: value) }
            for step in c["steps"]?.array ?? [] {
                let id = try XCTUnwrap(step["press"]?.string, "\(name): unknown step \(step.json)")
                XCTAssertNotNil(m.index(of: id), "\(name): \(id)")
                m.press(id: id)
            }
            let want = c["events"]?.array ?? []
            XCTAssertEqual(host.actions.map(\.name), want.compactMap { $0["name"]?.string }, "\(name): names")
            XCTAssertEqual(host.actions.map(\.context), want.map { $0["context"] ?? .object([:]) }, "\(name): contexts")
            XCTAssertEqual(m.data["value"] ?? .null, c["value"] ?? .null, "\(name): /value")
            XCTAssertEqual(m.layers.contains { $0.owner == rootId && $0.kind == "Popover" }, c["open"]?.bool == true, "\(name): open")
        }
    }

    // MARK: - interactions-round1.json keyRecorder (#EXP-1629)

    /// Every keyRecorder case through the MODEL: `press` = the generic press
    /// of the field, `down` / `up` / `modifiers` / `mouse` = `keyRecorderRecord`
    /// with the canonical payload the capture views send (mouse 1 and 2
    /// send nothing), `blur` = `keyRecorderBlur`, `hostWrite` = host data;
    /// the actions fired {name, context} and /recording.
    func testKeyRecorderCasesStartRecordAndStopThroughTheModel() throws {
        let f = try XCTUnwrap(Fixtures.json("interactions-round1.json")["keyRecorder"])
        let cases = f["cases"]?.array ?? []
        XCTAssertEqual(cases.count, 10)
        func flags(_ mods: JSONValue?) -> KeyRecorderFlags {
            let held = Set((mods?.array ?? []).compactMap(\.string))
            return KeyRecorderFlags(ctrl: held.contains("ctrl"), alt: held.contains("alt"), shift: held.contains("shift"), meta: held.contains("meta"))
        }
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let host = Round1Host()
            let m = try model(f["surface"]!, host: host)
            let field = "ptt.field"
            let index = try XCTUnwrap(m.index(of: field), "\(name): no field")
            XCTAssertTrue(m.isKeyRecorderField(index), "\(name): the field is a KeyRecorder field")
            for step in c["steps"]?.array ?? [] {
                if step["press"] != nil {
                    m.keyRecorderPress(id: field)
                } else if step["blur"] != nil {
                    m.keyRecorderBlur(id: field)
                } else if let key = step["down"]?.string {
                    m.keyRecorderRecord(id: field, KeyRecorderInput(type: .down, key: key, flags: flags(step["mods"])))
                } else if let key = step["up"]?.string {
                    m.keyRecorderRecord(id: field, KeyRecorderInput(type: .up, key: key, flags: flags(step["mods"])))
                } else if let mods = step["modifiers"] {
                    m.keyRecorderRecord(id: field, KeyRecorderInput(type: .modifiers, flags: flags(mods)))
                } else if let button = step["mouse"]?.number {
                    m.keyRecorderRecord(id: field, KeyRecorderInput(type: .mouse, button: Int(button)))
                } else if let w = step["hostWrite"] {
                    m.setData(path: w["path"]?.string ?? "", value: w["value"] ?? .null)
                } else {
                    XCTFail("\(name): unknown step \(step.json)")
                }
            }
            let want = c["events"]?.array ?? []
            XCTAssertEqual(host.actions.map(\.name), want.compactMap { $0["name"]?.string }, "\(name): names")
            XCTAssertEqual(host.actions.map(\.context), want.map { $0["context"] ?? .object([:]) }, "\(name): contexts")
            XCTAssertEqual(m.data["recording"] ?? .null, c["recording"] ?? .null, "\(name): /recording")
            XCTAssertEqual(m.keyRecorderRecording(index), c["recording"]?.bool == true, "\(name): the field's recording prop")
        }
    }

    /// The capture views' tables: the HID usage (iOS `UIKey.keyCode`) and the
    /// Mac key code → the canonical name; the physical key, never the
    /// shifted character; outside the list = nil.
    func testKeyRecorderKeyNamesAreCanonical() {
        let hid: [(Int, String?)] = [
            (0x04, "a"), (0x0E, "k"), (0x1D, "z"), (0x1E, "1"), (0x26, "9"), (0x27, "0"),
            (0x28, "enter"), (0x58, "enter"), (0x29, "escape"), (0x2A, "backspace"), (0x2B, "tab"), (0x2C, "space"),
            (0x2D, "-"), (0x2E, "="), (0x2F, "["), (0x30, "]"), (0x31, "\\"), (0x33, ";"), (0x34, "'"),
            (0x35, "`"), (0x36, ","), (0x37, "."), (0x38, "/"),
            (0x3A, "f1"), (0x45, "f12"), (0x68, "f13"), (0x73, "f24"),
            (0x49, "insert"), (0x4A, "home"), (0x4B, "pageup"), (0x4C, "delete"), (0x4D, "end"), (0x4E, "pagedown"),
            (0x4F, "right"), (0x50, "left"), (0x51, "down"), (0x52, "up"),
            (0x32, nil), (0x39, nil), (0x53, nil), (0x74, nil), (0xE0, nil),
        ]
        for (u, want) in hid { XCTAssertEqual(KeyRecorderKeys.name(hidUsage: u), want, "hid 0x\(String(u, radix: 16))") }
        XCTAssertEqual(KeyRecorderKeys.modifier(hidUsage: 0xE0), .ctrl)
        XCTAssertEqual(KeyRecorderKeys.modifier(hidUsage: 0xE5), .shift)
        XCTAssertEqual(KeyRecorderKeys.modifier(hidUsage: 0xE6), .alt)
        XCTAssertEqual(KeyRecorderKeys.modifier(hidUsage: 0xE3), .meta)
        XCTAssertNil(KeyRecorderKeys.modifier(hidUsage: 0x04))
        let mac: [(Int, String?)] = [
            (0x00, "a"), (0x28, "k"), (0x06, "z"), (0x12, "1"), (0x1D, "0"), (0x69, "f13"), (0x7A, "f1"),
            (0x7E, "up"), (0x7B, "left"), (0x31, "space"), (0x30, "tab"), (0x35, "escape"), (0x33, "backspace"),
            (0x75, "delete"), (0x2A, "\\"), (0x32, "`"), (0x27, "'"),
            (0x37, nil), (0x39, nil), (0x0A, nil), (0x5D, nil),
        ]
        for (c, want) in mac { XCTAssertEqual(KeyRecorderKeys.name(macKeyCode: c), want, "mac 0x\(String(c, radix: 16))") }
        XCTAssertEqual(KeyRecorderKeys.modifier(macKeyCode: 0x37), .meta)
        XCTAssertEqual(KeyRecorderKeys.modifier(macKeyCode: 0x3E), .ctrl)
        XCTAssertEqual((0...5).map { KeyRecorderKeys.button(macButtonNumber: $0) }, [nil, nil, 3, 4, 5, nil])
        // The payload every painter sends.
        let p = KeyRecorderInput(type: .down, key: "k", flags: KeyRecorderFlags(ctrl: true)).payload
        XCTAssertEqual(p, .object(["type": .string("down"), "key": .string("k"), "ctrl": .bool(true), "alt": .bool(false), "shift": .bool(false), "meta": .bool(false)]))
    }

    // MARK: - interactions-round1.json composerKeys (#EDE-65)

    /// Every composerKeys case through the MODEL: `type` = the field's edit
    /// (written through: the debounce flushed), `key` = `composerKey` (what
    /// the text views ask first), else the field's own Return without a
    /// modifier = `fieldCommitted` (the Composer sends); `hostWrite` = host
    /// data; whether the key was taken, the actions fired and /draft.
    func testComposerKeyCasesFireKeyOrSubmitThroughTheModel() throws {
        let f = try XCTUnwrap(Fixtures.json("interactions-round1.json")["composerKeys"])
        let cases = f["cases"]?.array ?? []
        XCTAssertEqual(cases.count, 10)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let host = Round1Host()
            let m = try model(f["surface"]!, host: host)
            for (path, value) in c["data"]?.object ?? [:] { m.setData(path: "/\(path)", value: value) }
            let index = try XCTUnwrap(m.index(of: "c"), "\(name): no composer")
            for step in c["steps"]?.array ?? [] {
                if let text = step["type"]?.string {
                    m.fieldEdited(index, text: text)
                    m.flushPending(index)
                } else if let key = step["key"]?.string {
                    let modified = !(step["mods"]?.array ?? []).isEmpty
                    let composing = step["composing"]?.bool == true
                    var handled = m.composerKey(index, key: key, modified: modified, composing: composing)
                    if !handled, key == "Enter", !modified, !composing {
                        m.fieldCommitted(index)
                        handled = true
                    }
                    XCTAssertEqual(handled, step["handled"]?.bool, "\(name): \(key) taken")
                } else if let w = step["hostWrite"] {
                    m.setData(path: w["path"]?.string ?? "", value: w["value"] ?? .null)
                } else {
                    XCTFail("\(name): unknown step \(step.json)")
                }
            }
            let want = c["events"]?.array ?? []
            XCTAssertEqual(host.actions.map(\.name), want.compactMap { $0["name"]?.string }, "\(name): names")
            XCTAssertEqual(host.actions.map(\.context), want.map { $0["context"] ?? .object([:]) }, "\(name): contexts")
            XCTAssertEqual(m.data["draft"] ?? .null, c["draft"] ?? .null, "\(name): /draft")
        }
        XCTAssertEqual(SurfaceModel.composerMenuKey("Tab", modified: true), nil)
        XCTAssertEqual(SurfaceModel.composerMenuKey("ArrowLeft", modified: false), nil)
    }

    // MARK: - interactions-round1.json composerSlots / composerPaste (#EDE-65 R)

    /// Every composerSlots case through the MODEL (the core's layout): the
    /// slot roots sit in the controls row in `order` (start → end, mirrored
    /// in RTL), centred on the send disc's row (`c.sendSpace`), inside the
    /// composer; `trailing` ends before the disc; a press on a slot child
    /// fires its own action. The attach control's place is the leaf's
    /// (`ComposerLeaf`: before `trailing`).
    func testComposerSlotCasesSitInTheControlsRowThroughTheModel() throws {
        let f = try XCTUnwrap(Fixtures.json("interactions-round1.json")["composerSlots"])
        let cases = f["cases"]?.array ?? []
        XCTAssertEqual(cases.count, 18)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let rtl = c["direction"]?.string == "rtl"
            guard case .object(var composer) = f["surface"]!, case .object(var props) = composer["props"] ?? .object([:]) else { return XCTFail("\(name): surface") }
            for (k, v) in c["props"]?.object ?? [:] { props[k] = v }
            composer["props"] = .object(props)
            let slots = c["slots"]?.object ?? [:]
            if !slots.isEmpty { composer["slots"] = .object(slots) }
            let tree: JSONValue = .object(["id": .string("root"), "component": .string("Box"), "style": .object(["display": .string("flex"), "flexDirection": .string("column"), "direction": .string(rtl ? "rtl" : "ltr")]), "children": .array([.object(composer)])])
            let host = Round1Host()
            let m = try model(tree, host: host)
            let frame = { (id: String) in m.index(of: id).map { m.frame($0) } }
            let cf = try XCTUnwrap(frame("c"), "\(name): composer")
            guard !slots.isEmpty else {
                XCTAssertNil(frame("c.bar"), "\(name): no row without slots")
                continue
            }
            let send = try XCTUnwrap(frame("c.sendSpace"), "\(name): the disc's square")
            // #EXP-1762: `above` sits inside the card at its content top, the
            // content's full width (the same inset either side), over the row.
            let hidden = c["hiddenAbove"]
            if let hidden {
                // #EDE-65: a hidden child = the slot absent (the core lays
                // nothing out for it); shown it grows the composer, hidden
                // again it is back.
                XCTAssertNil(frame("c.above"), "\(name): a hidden above lays nothing out")
                XCTAssertNil(frame("strip"), "\(name): a hidden above lays nothing out")
                let path = "/\(hidden["path"]?.string ?? "")"
                m.setData(path: path, value: hidden["value"] ?? .bool(true))
                let grown = try XCTUnwrap(frame("c"), "\(name): composer")
                let above = try XCTUnwrap(frame("c.above"), "\(name): shown above laid out")
                XCTAssertGreaterThan(above.height, 0, "\(name): the shown above has a height")
                XCTAssertGreaterThan(grown.height, cf.height + above.height, "\(name): grows by its height + the gap")
                m.setData(path: path, value: .bool(false))
                let back = try XCTUnwrap(frame("c"), "\(name): composer")
                XCTAssertEqual(back.height, cf.height, accuracy: 0.01, "\(name): hidden again")
                XCTAssertNil(frame("c.above"), "\(name): hidden again")
            } else if slots["above"] != nil {
                let above = try XCTUnwrap(frame("c.above"), "\(name): above laid out")
                let strip = try XCTUnwrap(frame("strip"), "\(name): the above child")
                let inset = above.minX - cf.minX
                XCTAssertGreaterThan(inset, 0, "\(name): above inside the card")
                XCTAssertEqual(cf.maxX - above.maxX, inset, accuracy: 0.01, "\(name): above spans the content width")
                XCTAssertEqual(above.minY - cf.minY, inset, accuracy: 0.01, "\(name): above at the content top")
                XCTAssertEqual(strip.width, above.width, accuracy: 0.01, "\(name): the above child spans it")
                XCTAssertLessThanOrEqual(above.maxY, send.minY, "\(name): above over the row")
            }
            var along: [(CGFloat, String)] = []
            for (slot, id) in [("leading", "plus"), ("trailing", "ring")] {
                guard let b = frame(id) else {
                    XCTAssertNil(slots[slot], "\(name): \(slot) laid out")
                    continue
                }
                XCTAssertEqual(b.midY, send.midY, accuracy: 0.01, "\(name): \(slot) centred on the disc")
                XCTAssertTrue(cf.insetBy(dx: -0.01, dy: -0.01).contains(b), "\(name): \(slot) inside the composer")
                along.append((rtl ? -b.maxX : b.minX, slot))
                if slot == "trailing" {
                    XCTAssertTrue(rtl ? b.minX >= send.maxX : b.maxX <= send.minX, "\(name): trailing before the disc")
                }
            }
            let order = (c["order"]?.array ?? []).compactMap(\.string).filter { $0 == "leading" || $0 == "trailing" }
            XCTAssertEqual(along.sorted { $0.0 < $1.0 }.map(\.1), order, "\(name): order")
            for step in c["steps"]?.array ?? [] { m.press(id: step["press"]?.string ?? "") }
            let want = c["events"]?.array ?? []
            XCTAssertEqual(host.actions.map(\.name), want.compactMap { $0["name"]?.string }, "\(name): events")
        }
    }

    /// #EXP-1762 composerSlots `field`: a flat Composer's text starts the
    /// field recipe's paddingTop below the row's top, one line (the disc's
    /// row) or grown, the text view reaching the row's bottom (neutral:
    /// paddingTop `$spacing.sm` 8, line 20, disc 36; the core replays the
    /// recipe values).
    func testAFlatComposerSetsItsTextAtTheFieldTopPadding() throws {
        let f = try XCTUnwrap(Fixtures.json("interactions-round1.json")["composerSlots"])
        let cases = (f["cases"]?.array ?? []).filter { $0["field"] != nil }
        XCTAssertEqual(cases.count, 2)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let rows = try CGFloat(XCTUnwrap(c["field"]?["rows"]?.number))
            let row: CGFloat = max(36, 8 + rows * 20)
            XCTAssertEqual(row > 36.5, c["field"]?["grown"]?.bool == true, "\(name): grown")
            let box = ComposerLeaf.flatField(row: row, padTop: 8)
            XCTAssertEqual(box.y, 8, accuracy: 0.001, "\(name): the text at the field's paddingTop")
            XCTAssertEqual(box.y + box.height, row, accuracy: 0.001, "\(name): the field reaches the row's bottom")
        }
    }

    /// Every composerPaste case through the MODEL: `composerPaste` with the
    /// clipboard's files (what the text views read off the pasteboard,
    /// `ComposerPasteboard`); not taken + text = the field's own edit.
    func testComposerPasteCasesAttachImagesThroughTheModel() throws {
        let f = try XCTUnwrap(Fixtures.json("interactions-round1.json")["composerPaste"])
        let cases = f["cases"]?.array ?? []
        XCTAssertEqual(cases.count, 5)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let host = Round1Host()
            let m = try model(f["surface"]!, host: host)
            for (path, value) in c["data"]?.object ?? [:] { m.setData(path: "/\(path)", value: value) }
            let index = try XCTUnwrap(m.index(of: "c"), "\(name): no composer")
            let files = (c["clipboard"]?["files"]?.array ?? []).map { SurfaceUploadFile(name: $0["name"]?.string ?? "", size: Int($0["size"]?.number ?? 0), type: $0["type"]?.string ?? "", data: Data(count: Int($0["size"]?.number ?? 0))) }
            let taken = m.composerPaste(index, files: files)
            if !taken, let text = c["clipboard"]?["text"]?.string {
                m.fieldEdited(index, text: (m.data["draft"]?.string ?? "") + text)
                m.flushPending(index)
            }
            let want = c["events"]?.array ?? []
            XCTAssertEqual(host.actions.map(\.name), want.compactMap { $0["name"]?.string }, "\(name): names")
            XCTAssertEqual(host.actions.map(\.context), want.map { $0["context"] ?? .object([:]) }, "\(name): contexts")
            XCTAssertEqual(host.uploads.flatMap { $0.files.map(\.name) }, (c["uploads"]?.array ?? []).compactMap(\.string), "\(name): uploads")
            XCTAssertTrue(host.uploads.allSatisfy { $0.componentId == "c" && $0.name == "c" }, "\(name): the upload names the Composer")
            XCTAssertEqual(m.data["draft"] ?? .null, c["draft"] ?? .null, "\(name): /draft")
        }
    }

    // MARK: - interactions-round1.json foldRow (#VAPP-284)

    /// A foldable Row (children): a press on the head or its fold button, or
    /// ArrowRight / ArrowLeft through the model's keyboard on the focused one
    /// (swapped in rtl); the events the host sees and the head's `expanded`.
    func testFoldRowCasesPressFoldAndTakeTheArrowsThroughTheModel() throws {
        let fold = try Fixtures.json("interactions-round1.json")["foldRow"]
        let cases = fold?["cases"]?.array ?? []
        XCTAssertEqual(cases.count, 12)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            var row = fold?["surface"]?.object ?? [:]
            if c["pressable"]?.bool == false, var on = row["on"]?.object {
                on["press"] = nil
                row["on"] = .object(on)
            }
            var tree: Props = ["id": .string("root"), "component": .string("Box"), "children": .array([.object(row)])]
            if c["rtl"]?.bool == true { tree["style"] = .object(["direction": .string("rtl")]) }
            let host = Round1Host()
            var options = SurfaceOptions()
            options.theme = ThemeHandle.builtin("neutral")
            options.mode = .light
            let m = try SurfaceModel(id: "fold", options: options, host: host)
            m.setViewport(width: 400, height: 800)
            try m.setNested(json: JSONValue.object(tree).json)
            if let v = c["open"], !v.isNull { m.setData(path: "/open", value: v) }
            let step = c["step"]?.object ?? [:]
            let part = (step["press"] ?? step["on"])?.string ?? "head"
            let target = part == "head" ? "issues.head" : "issues.head.fold"
            XCTAssertNotNil(m.index(of: target), "\(name): no \(target)")
            if step["press"] != nil {
                m.press(id: target)
            } else {
                m.requestFocus(target, keyboard: true)
                XCTAssertEqual(m.focusedId, target, "\(name): \(target) takes focus")
                let key = step["key"]?.string == "ArrowLeft" ? "left" : "right"
                XCTAssertTrue(m.handleKey(key: key), "\(name): the arrow is the fold's")
            }
            XCTAssertEqual(host.actions.map(\.name), c["events"]?.array?.compactMap(\.string), "\(name): events")
            for a in host.actions where a.name == "fold" {
                XCTAssertEqual(a.context["open"]?.bool, c["after"]?.bool, "\(name): fold context")
            }
            XCTAssertEqual(m.node(id: "issues.head")?.accessibility?["expanded"]?.bool, c["after"]?.bool, "\(name): expanded after")
        }
    }

    // MARK: - interactions-round1.json tree (#VAPP-285)

    /// A Tree: a press on a row (leaf = select, branch = fold, lazy = load),
    /// or a key through the model's keyboard on the focused treeitem (the
    /// core's tree keys move the focus or fold, rtl-aware); the events the
    /// host sees, where the focus lands, /open and /picked after.
    func testTreeCasesSelectFoldFetchAndMoveTheFocusThroughTheModel() throws {
        let t = try Fixtures.json("interactions-round1.json")["tree"]
        let rowIds = t?["rowIds"]?.object ?? [:]
        let cases = t?["cases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        let keys = ["ArrowUp": "up", "ArrowDown": "down", "ArrowLeft": "left", "ArrowRight": "right", "Home": "home", "End": "end"]
        for c in cases {
            let name = c["name"]?.string ?? "?"
            var tree: Props = ["id": .string("root"), "component": .string("Box"), "children": .array([t?["surface"] ?? .null])]
            if c["rtl"]?.bool == true { tree["style"] = .object(["direction": .string("rtl")]) }
            let host = Round1Host()
            var options = SurfaceOptions()
            options.theme = ThemeHandle.builtin("neutral")
            options.mode = .light
            let m = try SurfaceModel(id: "tree", options: options, host: host)
            m.setViewport(width: 400, height: 800)
            try m.setNested(json: JSONValue.object(tree).json)
            m.setData(path: "/open", value: c["open"] ?? .array([]))
            let step = c["step"]?.object ?? [:]
            let item = (step["press"] ?? step["on"])?.string ?? "?"
            let target = rowIds[item]?.string ?? "?"
            XCTAssertNotNil(m.index(of: target), "\(name): no \(target)")
            var moved: String?
            if step["press"] != nil {
                m.press(id: target)
            } else {
                m.requestFocus(target, keyboard: true)
                XCTAssertEqual(m.focusedId, target, "\(name): \(target) takes focus")
                let key = keys[step["key"]?.string ?? ""] ?? "?"
                XCTAssertTrue(m.handleKey(key: key), "\(name): \(key) is the tree's")
                if let now = m.focusedId, now != target {
                    moved = rowIds.first(where: { $0.value.string == now })?.key ?? now
                }
            }
            XCTAssertEqual(host.actions.map(\.name), c["events"]?.array?.compactMap(\.string), "\(name): events")
            XCTAssertEqual(moved, c["after"]?["focus"]?.string, "\(name): focus")
            XCTAssertEqual(m.data["open"] ?? .null, c["after"]?["open"] ?? .null, "\(name): /open")
            XCTAssertEqual(m.data["picked"] ?? .null, c["after"]?["picked"] ?? .null, "\(name): /picked")
            if let visible = c["after"]?["visible"]?.array?.compactMap(\.string) {
                let shown = rowIds.filter { m.index(of: $0.value.string ?? "") != nil }.map(\.key).sorted()
                XCTAssertEqual(shown, visible.sorted(), "\(name): visible rows")
            }
        }
    }
}
