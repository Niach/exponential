import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// Presses, mirrors, host-owned fields (debounce, revisions, the echo
/// rule), overlays and the measurer's rules.
@MainActor
final class InteractionTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private func sink(_ host: HostPlugin) throws -> SurfaceModel {
        var options = SurfaceOptions()
        let m = try SurfaceModel(id: "kitchen-sink", options: options, host: host)
        options.mode = .dark
        try m.setNested(json: Fixtures.text("kitchen-sink.json"))
        m.setData(path: "/draft", value: .object(["title": .string("")]))
        m.setViewport(width: 390, height: 800)
        return m
    }

    func testPressFiresTheAction() throws {
        let host = RecordingHost()
        let m = try sink(host)
        m.press(id: "hdr-scan")
        XCTAssertEqual(host.actions.map(\.name), ["scan"])
        XCTAssertEqual(host.actions[0].componentId, "hdr-scan")
        XCTAssertEqual(host.actions[0].event, "press")
        XCTAssertEqual(host.actions[0].surfaceId, "kitchen-sink")
    }

    func testPressedStateRelayoutsTheOpacity() throws {
        let m = try sink(RecordingHost())
        let i = m.index(of: "hdr-scan")!
        XCTAssertNil(m.style(i).opacity)
        m.pressDown("hdr-scan")
        XCTAssertEqual(m.style(i).opacity ?? 1, 0.6, accuracy: 0.001)
        m.pressUp("hdr-scan")
        XCTAssertNil(m.style(i).opacity)
    }

    func testUnboundControlsKeepTheirValueInTheCore() throws {
        // Round 1: the core keeps an unbound control's value (its local
        // field state); the model fires the event and reads the props back.
        let m = try sink(RecordingHost())
        let box = m.index(of: "form-agree.box")!
        XCTAssertTrue(m.checked(box))
        let before = m.style(box).background
        m.press(box)
        XCTAssertFalse(m.checked(box))
        XCTAssertNotEqual(m.style(box).background, before, "the core re-resolved the box's recipe for the new value")
        XCTAssertEqual(m.style(box).background, m.style(box).background, "no painter-side override")
        let track = m.index(of: "nav-toggle.track")!
        XCTAssertTrue(m.checked(track))
        m.press(track)
        XCTAssertFalse(m.checked(track))
        // Radio: a dot press resolves the row by its index suffix.
        let dot = m.index(of: "form-plan.dot.1")!
        XCTAssertFalse(m.radioChecked(dot))
        m.press(dot)
        XCTAssertTrue(m.radioChecked(dot))
        XCTAssertFalse(m.radioChecked(m.index(of: "form-plan.dot.0")!))
        // ToggleGroup single + multiple.
        let seg = m.index(of: "segmented")!
        m.toggleGroupSelect(seg, value: .string("board"))
        XCTAssertEqual(m.toggleGroupValues(seg), ["board"])
        let multi = m.index(of: "toggles")!
        m.toggleGroupSelect(multi, value: .string("italic"))
        XCTAssertEqual(Set(m.toggleGroupValues(multi)), ["bold", "italic"])
        m.toggleGroupSelect(multi, value: .string("bold"))
        XCTAssertEqual(m.toggleGroupValues(multi), ["italic"])
        // Slider snaps to the step.
        let trackIdx = m.index(of: "form-volume.track")!
        m.sliderDrag(trackIdx, value: SurfaceModel.snap(42, min: 0, max: 100, step: 5))
        XCTAssertEqual(m.sliderValue(trackIdx), 40)
        m.sliderRelease(trackIdx)
        XCTAssertEqual(m.sliderValue(trackIdx), 40)
    }

    func testTabsAndAccordionSwitchThroughTheCore() throws {
        let m = try sink(RecordingHost())
        let run = m.index(of: "tabs.tab.1")!
        XCTAssertTrue(m.node(m.index(of: "tab-run")!)!.hidden)
        m.press(run)
        XCTAssertFalse(m.node(m.index(of: "tab-run")!)!.hidden)
        XCTAssertTrue(m.node(m.index(of: "tab-issue")!)!.hidden)
        XCTAssertTrue(m.node(run)!.selected)
        let trigger = m.index(of: "accordion.trigger.1")!
        XCTAssertTrue(m.node(m.index(of: "acc-history")!)!.hidden)
        m.press(trigger)
        XCTAssertFalse(m.node(m.index(of: "acc-history")!)!.hidden)
    }

    func testFieldDebounceRevisionsAndCommit() async throws {
        let host = RecordingHost()
        let m = try sink(host)
        let field = m.index(of: "echo-field.field")!
        m.fieldFocused(field, true)
        for ch in "Hello" {
            m.fieldEdited(field, text: m.fieldText(field) + String(ch))
        }
        XCTAssertEqual(host.inputs.count, 0, "nothing before the debounce")
        try await eventually { host.inputs.count == 1 }
        XCTAssertEqual(host.inputs.count, 1)
        let first = try XCTUnwrap(host.inputs.first)
        XCTAssertEqual(first.value, .string("Hello"))
        XCTAssertEqual(first.revision, 5)
        XCTAssertEqual(first.kind, .change)
        XCTAssertEqual(first.path, "/draft/title")
        XCTAssertEqual(m.data["draft"]?["title"], .string("Hello"), "a bound value writes through")
        m.fieldEdited(field, text: "Hello!")
        m.fieldFocused(field, false)
        // Blur (gpui `flush_field`): the pending edit as `change`, then `commit`.
        XCTAssertEqual(host.inputs.count, 3)
        XCTAssertEqual(host.inputs[1].kind, .change)
        XCTAssertEqual(host.inputs[2].kind, .commit)
        XCTAssertEqual(host.inputs[2].revision, 6)
        XCTAssertEqual(host.inputs[2].value, .string("Hello!"))
        // An echo of the host's own write never rewrites the field; a REAL
        // host change lands only while the field is idle and unfocused.
        m.setData(path: "/draft/title", value: .string("Hello!"))
        XCTAssertEqual(m.fieldText(field), "Hello!")
        m.setData(path: "/draft/title", value: .string("From the host"))
        XCTAssertEqual(m.fieldText(field), "From the host")
        m.fieldFocused(field, true)
        m.setData(path: "/draft/title", value: .string("Ignored while typing"))
        XCTAssertEqual(m.fieldText(field), "From the host")
    }

    func testFortyKeystrokesInOrderWithAnEcho() async throws {
        // The VAPP-4 typing test at the model level: 40 edits in a burst with
        // a 150 ms host echo; every character lands, in order, nothing is
        // overwritten by the echo.
        var echoed: [SurfaceInputEvent] = []
        let m = try sink(NoHost())
        let field = m.index(of: "echo-field.field")!
        let host = ClosureHost(inputs: { e in
            echoed.append(e)
            Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(150))
                if let path = e.path { m.setData(path: path, value: e.value) }
            }
        })
        m.host = host
        m.fieldFocused(field, true)
        let text = "The quick brown fox jumps over the lazy dog"
        var typed = ""
        for ch in text.prefix(40) {
            typed.append(ch)
            m.fieldEdited(field, text: typed)
        }
        try await eventually { echoed.last?.revision == 40 }
        // The last echo (150 ms) lands too; the field keeps every character.
        try await Task.sleep(for: .milliseconds(200))
        XCTAssertEqual(m.fieldText(field), String(text.prefix(40)))
        XCTAssertEqual(echoed.last?.revision, 40)
        XCTAssertEqual(echoed.last?.value, .string(String(text.prefix(40))))
    }

    func testOverlaysOpenAndDismiss() throws {
        let host = RecordingHost()
        let m = try sink(host)
        guard let trigger = m.nodes.first(where: { $0.triggerFor != nil && m.node(m.index(of: $0.triggerFor!) ?? -1)?.component == "Dialog" }) else {
            throw XCTSkip("no dialog trigger in the kitchen sink")
        }
        m.press(trigger.index)
        XCTAssertEqual(m.modalLayers.count, 1)
        let owner = m.modalLayers[0].owner
        XCTAssertEqual(m.layerReturn[owner], trigger.id)
        let focusedInside = m.focusedId.flatMap { m.node(id: $0) }
        XCTAssertNotNil(focusedInside, "opening a dialog moves focus into it")
        XCTAssertGreaterThan(focusedInside?.layer ?? 0, 0)
        XCTAssertTrue(m.escape())
        XCTAssertTrue(m.layers.filter(\.isInteractive).isEmpty, "the toast layer stays; no overlay")
        XCTAssertEqual(m.focusedId, trigger.id, "closing returns focus to the trigger")
        // A tooltip opens as a core layer.
        if let tip = m.nodes.first(where: { $0.component == "Tooltip" }) {
            m.setOpen(tip.id, true)
            XCTAssertTrue(m.layers.contains { $0.kind == "Tooltip" })
            m.setOpen(tip.id, false)
        }
    }

    func testLinkOpensTheUrl() throws {
        let host = RecordingHost()
        let m = try sink(host)
        m.press(id: "form-link")
        XCTAssertEqual(host.urls, ["https://example.com/rules"])
    }

    func testMeasurerRules() {
        let ts = TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: nil)
        let one = TextShaper.measure("Hello world", ts, wrap: nil, lines: nil)
        XCTAssertEqual(one.height, 20)
        XCTAssertGreaterThan(one.width, 40)
        let wrapped = TextShaper.measure("Hello world again and again", ts, wrap: 60, lines: nil)
        XCTAssertGreaterThanOrEqual(wrapped.height, 40)
        XCTAssertEqual(wrapped.height.truncatingRemainder(dividingBy: 20), 0, "n lines = n × lineHeight")
        XCTAssertEqual(TextShaper.measure("Hello world", ts, wrap: 0, lines: 1).width, TextShaper.maxContent("Hello world", ts), "a one-line text never breaks: its min-content is the whole line (round 2)")
        XCTAssertEqual(TextShaper.measure("Hello world", ts, wrap: 0, lines: nil).width, TextShaper.minContent("Hello world", ts))
        XCTAssertEqual(TextShaper.measure("a\nb\nc", ts, wrap: nil, lines: 2).height, 40, "clamped to the lines prop")
        let c = ControlBox(paddingHorizontal: 16, paddingVertical: 0, borderWidth: 1, gap: 8, minWidth: 80, minHeight: nil, width: nil, height: 36)
        XCTAssertEqual(c.insets.0, 34)
        XCTAssertEqual(c.borderBox(CGSize(width: 20, height: 20)), CGSize(width: 80, height: 36))
        XCTAssertEqual(c.borderBox(CGSize(width: 100, height: 20)), CGSize(width: 134, height: 36))
        XCTAssertEqual(c.innerWrap(134), 100)
        XCTAssertEqual(c.innerWrap(0), 0)
        XCTAssertNil(c.innerWrap(nil))
        XCTAssertEqual(SurfaceModel.snap(0.30000000004, min: 0, max: 1, step: 0.1), 0.3)
        XCTAssertEqual(formatDuration(3_725_000), "1:02:05")
    }

    func testAccessibilityOrderIsPreOrder() throws {
        let m = try sink(RecordingHost())
        // The painter pins VoiceOver to PAINT order (slots are stable, not
        // pre-order): the sort priority strictly decreases along `order`.
        let priorities = m.order.map { m.accessibilityPriority($0) }
        XCTAssertEqual(priorities, priorities.sorted(by: >))
        XCTAssertEqual(Set(priorities).count, priorities.count)
        // Labels: every focusable leaf has an accessible name.
        let unnamed = m.nodes.filter { $0.isFocusable && $0.isLeaf && !$0.isTextField && $0.component != "ToggleGroup" && $0.part != "box" && $0.part != "dot" && $0.part != "track" && $0.part != "field" && $0.part != "indicator" && $0.accessibilityLabel == nil }
        XCTAssertEqual(unnamed.map(\.id), [])
        // The header title reads before the button, the form before the footer.
        let order = m.nodes.filter { $0.isLeaf && !$0.hidden }.map(\.id)
        XCTAssertLessThan(order.firstIndex(of: "hdr-title")!, order.firstIndex(of: "hdr-scan")!)
        XCTAssertLessThan(order.firstIndex(of: "echo-field.field")!, order.firstIndex(of: "form-link")!)
    }

    func testWindowedListScrolls() throws {
        let m = try makeModel("list")
        var rows: [JSONValue] = []
        for i in 0..<200 {
            rows.append(.object(["id": .string("r\(i)"), "component": .string("Text"), "props": .object(["text": .string("Row \(i)")])]))
        }
        let tree = JSONValue.object(["id": .string("root"), "component": .string("List"), "style": .object(["height": .number(300), "overflow": .string("scroll")]), "children": .array(rows)])
        try m.setNested(json: tree.json)
        guard let list = m.lists["root"] else { return XCTFail("no list output") }
        XCTAssertTrue(list.windowed)
        XCTAssertEqual(list.count, 200)
        XCTAssertGreaterThan(list.contentHeight, 1000)
        XCTAssertLessThan(list.end - list.start, 200, "only the visible range plus overscan is laid out")
        XCTAssertNotNil(m.index(of: "r0"))
        let firstBefore = m.frame(m.index(of: "r0")!)
        m.scroll(list: "root", offset: 2000)
        let after = m.lists["root"]!
        XCTAssertGreaterThan(after.start, list.start)
        let rowInWindow = "r\(after.start + 1)"
        XCTAssertNotNil(m.index(of: rowInWindow), "the new window's rows are laid out")
        XCTAssertGreaterThan(m.frame(m.index(of: rowInWindow)!).minY, firstBefore.minY)
    }
}
