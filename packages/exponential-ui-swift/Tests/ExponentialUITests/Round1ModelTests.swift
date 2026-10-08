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
            props["value"] = obj(["path": "/v"])
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
                    obj(["id": "agree", "component": "Checkbox", "props": obj(["label": "I agree", "name": "agree", "checked": obj(["path": "/agreed"]), "checks": c["checks"]?.any ?? [Any]()])]),
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
        XCTAssertGreaterThan(replayed, 30, "the presses replayed through the model")
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

    func testFieldKeysToggleGroupKeysAndAFocusedToast() throws {
        let tree = root([
            obj(["id": "n", "component": "NumberField", "props": obj(["label": "N", "name": "n", "value": obj(["path": "/v"]), "step": 1, "max": 100])]),
            obj(["id": "g", "component": "ToggleGroup", "props": obj(["items": [obj(["label": "A", "value": "a"]), obj(["label": "B", "value": "b"]), obj(["label": "C", "value": "c"])], "value": "a"])]),
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
        // ToggleGroup: arrows move the roving item (wrap), Space toggles it.
        let g = try XCTUnwrap(m.index(of: "g"))
        m.requestFocus("g", keyboard: true)
        XCTAssertEqual(m.toggleGroupFocusIndex(g), 0)
        m.handleKey(key: "left")
        XCTAssertEqual(m.toggleGroupFocusIndex(g), 2, "wraps")
        m.handleKey(key: "space")
        XCTAssertEqual(m.toggleGroupValues(g), ["c"])
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
        try await Task.sleep(for: .milliseconds(450))
        XCTAssertEqual(m.toasts.map(\.id), ["t2"], "t1 ran out; the hovered t2 is held")
        XCTAssertEqual(host.actions.filter { $0.name == "dismiss" }.map(\.componentId), ["t1"])
        m.toastHover(id: "t2", false)
        try await Task.sleep(for: .milliseconds(450))
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
        try await Task.sleep(for: .milliseconds(450))
        XCTAssertTrue(m.layers.contains { $0.kind == "Tooltip" })
        m.setHover(id: trigger.id, false)
        try await Task.sleep(for: .milliseconds(400))
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
}
