import XCTest
import SwiftUI
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-100: the round-1 natives and parts the SwiftUI painter draws
/// (contract §3, §9 SwiftUI 4–5): every specimen paints with no Unknown,
/// the new controls measure to `control-geometry.json`, Table sorts /
/// selects / windows through the core, CodeBlock lines carry the core's
/// tokens (`code-tokens.json`), Chart reads the core's ticks, the inline
/// fields, FileUpload, menus and the built-in tables.
@MainActor
final class Round1NativesTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    func model(_ id: String = "r1", theme: String? = "exponential", mode: Mode = .light) throws -> SurfaceModel {
        var o = SurfaceOptions()
        o.theme = theme.flatMap { ThemeHandle.builtin($0) }
        o.mode = mode
        return try SurfaceModel(id: id, options: o, host: NoHost())
    }

    /// Paint the model hosted at `width` (every `NodeView` body evaluated,
    /// layers included); the node indices painted.
    @discardableResult
    func paint(_ m: SurfaceModel, width: CGFloat = 390, height: CGFloat = 900) -> Set<Int> {
        m.setViewport(width: width, height: height)
        var painted = Set<Int>()
        m.paintProbe = { painted.insert($0) }
        defer { m.paintProbe = nil }
        let root = ExponentialSurface(model: m).frame(width: width)
        #if canImport(AppKit)
        let view = NSHostingView(rootView: root)
        view.frame = CGRect(x: 0, y: 0, width: width, height: max(m.surfaceSize.height, height))
        view.layoutSubtreeIfNeeded()
        _ = view.fittingSize
        #else
        let view = UIHostingController(rootView: root).view!
        view.frame = CGRect(x: 0, y: 0, width: width, height: max(m.surfaceSize.height, height))
        view.layoutIfNeeded()
        #endif
        return painted
    }

    func surface(_ node: JSONValue, theme: String? = "exponential", width: CGFloat = 390) throws -> SurfaceModel {
        let m = try model(theme: theme)
        try m.setNested(json: node.json)
        m.setViewport(width: width, height: 900)
        return m
    }

    func node(_ id: String, _ component: String, _ props: [String: JSONValue] = [:]) -> JSONValue {
        .object(["id": .string(id), "component": .string(component), "props": .object(props)])
    }

    // MARK: - specimens

    /// Every specimen (one surface per visible component, `specimens.json`)
    /// lays out and paints hosted with no Unknown and every visible
    /// main-tree node painted.
    func testEverySpecimenPaintsWithoutUnknown() throws {
        let specimens = try Fixtures.json("specimens.json")["specimens"]?.array ?? []
        // Round 3: 71 (the 17 folded components left, Row/Section/Chip/Segmented/Menu joined).
        XCTAssertGreaterThan(specimens.count, 65)
        var failures: [String] = []
        for spec in specimens {
            let id = spec["id"]?.string ?? "?"
            guard let tree = spec["node"] else { continue }
            do {
                let m = try model(id)
                try m.setNested(json: tree.json)
                let painted = paint(m)
                if let u = m.nodes.first(where: { $0.component == "Unknown" }), spec["component"]?.string != "Unknown" {
                    failures.append("\(id): Unknown at \(u.id)")
                }
                var visible = [Bool](repeating: false, count: m.nodes.count)
                for n in m.nodes {
                    visible[n.index] = (n.parent.map { visible[$0] } ?? true) && !n.hidden && n.layer == 0
                }
                let missing = m.nodes.filter { visible[$0.index] && !painted.contains($0.index) }
                if !missing.isEmpty { failures.append("\(id): not painted \(missing.prefix(4).map(\.id))") }
            }
        }
        XCTAssertEqual(failures, [])
    }

    /// The macros the round adds over existing leaves paint as leaves the
    /// painter knows (no bare boxes where content belongs).
    func testRound1MacrosExpandToKnownLeaves() throws {
        let specimens = try Fixtures.json("specimens.json")["specimens"]?.array ?? []
        for name in ["Rating", "Stepper", "Breadcrumb", "Row", "Section", "Chip", "AppBar", "Kbd", "Label", "Sparkline"] {
            guard let spec = specimens.first(where: { $0["component"]?.string == name }), let tree = spec["node"] else { XCTFail("no \(name) specimen"); continue }
            let m = try model(name)
            try m.setNested(json: tree.json)
            m.setViewport(width: 390, height: 900)
            XCTAssertGreaterThan(m.nodes.count, 2, name)
            XCTAssertNil(m.nodes.first { $0.component == "Unknown" }, name)
            let leaves = m.nodes.filter { $0.isLeaf && !$0.hidden && $0.layer == 0 }
            XCTAssertFalse(leaves.isEmpty, name)
            for leaf in leaves {
                XCTAssertGreaterThan(m.frame(leaf.index).width + m.frame(leaf.index).height, 0, "\(name) \(leaf.id) has a frame")
            }
            if name == "Sparkline" {
                XCTAssertTrue(m.nodes.contains { $0.component == "Chart" && $0.props.str("kind") == "sparkline" }, "Sparkline renders through Chart kind sparkline")
            }
        }
    }

    // MARK: - geometry

    /// The round-1 controls measure to `control-geometry.json` in every
    /// built-in theme: the sizing PART's frame (the measurer's border box)
    /// and padding / border / radius (a leaf's visual; a container part's
    /// padding is the recipe's, which the core lays its children out with).
    func testNewControlsMeasureToControlGeometry() throws {
        let f = try Fixtures.json("control-geometry.json")
        let catalog = JSONValue.parse(try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("catalog/core.catalog.json"), encoding: .utf8))
        let components = ["NumberField", "ChipInput", "Select", "DatePicker", "TimePicker", "DateRangePicker", "Input", "Textarea", "Checkbox", "Switch"]
        var issues: [String] = []
        var checked = 0
        for (themeId, byComponent) in f["themes"]?.object ?? [:] {
            for component in components {
                guard let entry = byComponent[component], let part = entry["part"]?.string else { continue }
                for (name, c) in entry["cases"]?.object ?? [:] {
                    var props = catalog["components"]?[component]?["example"]?.object ?? [:]
                    for (k, v) in c["props"]?.object ?? [:] { props[k] = v }
                    let m = try model("cg", theme: themeId)
                    try m.setNested(json: node("root", component, props).json)
                    m.setViewport(width: 360, height: 800)
                    guard let n = m.nodes.first(where: { $0.recipeComponent == component && $0.part == part }) else {
                        issues.append("\(themeId) \(component) \(name): no \(part)")
                        continue
                    }
                    let frame = m.frame(n.index)
                    let style = m.style(n.index)
                    let recipe = m.part(component, part, props: props)
                    let padH = n.isLeaf ? style.paddingHorizontal : (recipe.px("paddingHorizontal") ?? recipe.px("padding") ?? 0)
                    let padV = n.isLeaf ? style.paddingVertical : (recipe.px("paddingVertical") ?? recipe.px("padding") ?? 0)
                    let got: [String: CGFloat] = ["width": frame.width, "height": frame.height, "paddingHorizontal": padH, "paddingVertical": padV, "borderWidth": style.borderWidth, "borderRadius": min(style.radius, 9999)]
                    let want = c["geometry"]?.object ?? [:]
                    for (key, value) in got {
                        guard let w = want[key]?.number else { continue }
                        if key == "height", want["minHeight"] != nil { continue }
                        if abs(Double(value) - w) > 0.5 { issues.append("\(themeId) \(component) \(name): \(key) \(w)≠\(value)") }
                    }
                    if let floor = want["minHeight"]?.number, Double(frame.height) + 0.5 < floor { issues.append("\(themeId) \(component) \(name): minHeight \(floor)>\(frame.height)") }
                    checked += 1
                }
            }
        }
        XCTAssertGreaterThan(checked, 20)
        XCTAssertEqual(issues.sorted(), [])
    }

    /// Per-side paddings, the baseline and the round-1 leaf rules.
    func testMeasurerPortsTheRound1Rules() throws {
        let c = ControlBox(FfiControlBox(paddingHorizontal: 6, paddingVertical: 4, paddingTop: 2, paddingRight: 4, paddingBottom: 6, paddingLeft: 8, borderWidth: 1, gap: 0, minWidth: nil, minHeight: nil, width: nil, height: nil))
        XCTAssertEqual(c.insets.0, 14)
        XCTAssertEqual(c.insets.1, 10)
        XCTAssertEqual(c.topInset, 3)
        let measurer = SurfaceMeasurer(theme: nil, mode: .light, extensions: ExtensionRegistry(), kinds: [:], generation: 0)
        let ts = TextStyle.body
        let text = measurer.measureLeaf(LeafRequest(component: "Text", props: ["text": .string("Hello")], control: c), wrap: nil)
        XCTAssertEqual(text.height, 20 + 10)
        XCTAssertNotNil(text.baseline)
        XCTAssertEqual(text.baseline!, 3 + SurfaceMeasurer.baseline(ts), accuracy: 0.01, "a text's first baseline sits under its top inset")
        XCTAssertGreaterThan(SurfaceMeasurer.baseline(ts), 10)
        XCTAssertLessThan(SurfaceMeasurer.baseline(ts), 20)
        // A picker trigger is its text + `gap: sm` + the glyph inside the box
        // (no floor: the web's trigger is as wide as that) and centres its line.
        let box = ControlBox(paddingHorizontal: 12, borderWidth: 1, height: 36)
        let trigger = measurer.measureLeaf(LeafRequest(component: "Select", part: "trigger", props: ["text": .string("A")], control: box), wrap: nil)
        XCTAssertEqual(trigger.width, TextShaper.width("A", ts) + GeometrySpacing.value("sm") + 16 + 26, accuracy: 0.01)
        // A press Menu's own trigger is an outline button: icon + label, no
        // chevron (round-2 contract §7).
        let menuBox = ControlBox(paddingHorizontal: 12, borderWidth: 1, gap: 4, height: 32)
        let menu = measurer.measureLeaf(LeafRequest(component: "Button", part: "trigger", props: ["label": .string("More")], control: menuBox), wrap: nil)
        let button = measurer.measureLeaf(LeafRequest(component: "Button", props: ["label": .string("More")], control: menuBox), wrap: nil)
        XCTAssertEqual(menu.width, button.width, accuracy: 0.01)
        XCTAssertEqual(trigger.height, 36)
        XCTAssertEqual(trigger.baseline!, 8 + SurfaceMeasurer.baseline(ts), accuracy: 0.01)
        // Inline fields: text or placeholder + caret, the search glyph.
        let search = measurer.measureLeaf(LeafRequest(component: "Select", part: "search", props: ["placeholder": .string(""), "icon": .string("search")]), wrap: nil)
        XCTAssertEqual(search.width, 24 + 24)
        // Table chrome: a sortable header reserves the arrow; boolean cells 16.
        let header = measurer.measureLeaf(LeafRequest(component: "Text", part: "headerCell", props: ["text": .string("Runs"), "sortable": .bool(true)]), wrap: nil)
        let plain = measurer.measureLeaf(LeafRequest(component: "Text", props: ["text": .string("Runs")]), wrap: nil)
        XCTAssertEqual(header.width, plain.width + 20)
        XCTAssertEqual(measurer.measureLeaf(LeafRequest(component: "Text", part: "cell", props: ["cellType": .string("boolean"), "text": .string("✓")]), wrap: nil).width, 16)
        // Select options reserve the check slot; TimePicker times do not.
        let option = measurer.measureLeaf(LeafRequest(component: "Text", part: "item", props: ["text": .string("Runs"), "disabled": .bool(false)]), wrap: nil)
        XCTAssertEqual(option.width, plain.width + 24)
        XCTAssertEqual(measurer.measureLeaf(LeafRequest(component: "Text", part: "item", props: ["text": .string("Runs")]), wrap: nil).width, plain.width)
        // Textarea autosize: rows...maxRows.
        let long = String(repeating: "word ", count: 80)
        let auto = measurer.measureLeaf(LeafRequest(component: "Textarea", part: "field", props: ["rows": .number(2), "maxRows": .number(4), "autosize": .bool(true), "value": .string(long)]), wrap: nil)
        XCTAssertEqual(auto.height, 80)
        let short = measurer.measureLeaf(LeafRequest(component: "Textarea", part: "field", props: ["rows": .number(2), "autosize": .bool(true), "value": .string("hi")]), wrap: nil)
        XCTAssertEqual(short.height, 40)
        // Markdown `lines` clamps; a sparkline is 120 × 32.
        let md = measurer.measureLeaf(LeafRequest(component: "Markdown", props: ["text": .string("a\n\nb\n\nc\n\nd"), "lines": .number(2)]), wrap: 300)
        XCTAssertEqual(md.height, 40)
        let spark = measurer.measureLeaf(LeafRequest(component: "Chart", props: ["kind": .string("sparkline")]), wrap: nil)
        XCTAssertEqual(spark.width, 120)
        XCTAssertEqual(spark.height, 32)
    }

    // MARK: - Table

    func tableSurface(rows count: Int, bound: Bool = false) throws -> SurfaceModel {
        let rows: [JSONValue] = (0..<count).map { i in .object(["id": .string("r\(i)"), "name": .string("Name \(count - i)"), "n": .number(Double((i * 7) % count)), "ok": .bool(i % 2 == 0)]) }
        let columns: [JSONValue] = [
            .object(["key": .string("name"), "label": .string("Name"), "sortable": .bool(true)]),
            .object(["key": .string("n"), "label": .string("N"), "type": .string("number"), "sortable": .bool(true), "align": .string("end")]),
            .object(["key": .string("ok"), "label": .string("OK"), "type": .string("boolean")]),
        ]
        var tree = node("t", "Table", ["columns": .array(columns), "rows": .array(rows), "selectable": .string("multiple"), "striped": .bool(true)]).object!
        tree["style"] = .object(["height": .number(400)])
        return try surface(.object(tree))
    }

    /// The rows the core laid out, top to bottom (their `key`s).
    func rowKeys(_ m: SurfaceModel) -> [String] {
        m.liveParts("t", "row").sorted { m.frame($0.index).minY < m.frame($1.index).minY }.map { $0.props.str("key") }
    }

    func testTableSortsSelectsAndWindows() throws {
        let m = try tableSurface(rows: 8)
        paint(m)
        XCTAssertEqual(rowKeys(m).count, 8)
        // Unbound sort = the core's local sorted copy; the arrow is the core's glyph.
        m.press(id: "t.headerCell.1")
        let asc = rowKeys(m)
        let nOf = { (k: String) in m.nodes.first { $0.id == "t.cell.\(k).1" }?.props["value"]?.number ?? -1 }
        XCTAssertEqual(asc.map(nOf), asc.map(nOf).sorted())
        XCTAssertEqual(m.node(m.index(of: "t.headerCell.1")!)?.props.str("sortIcon"), BuiltinIcons.name("Table.sortIcon.asc"))
        m.press(id: "t.headerCell.1")
        let desc = rowKeys(m)
        XCTAssertEqual(desc.map(nOf), desc.map(nOf).sorted(by: >))
        XCTAssertEqual(m.node(m.index(of: "t.headerCell.1")!)?.props.str("sortIcon"), BuiltinIcons.name("Table.sortIcon.desc"))
        // Multiple selection: the select-all header checks every row.
        guard let all = m.nodes.first(where: { $0.recipeComponent == "Table" && $0.part == "checkbox" && $0.props.flag("header") }) else { return XCTFail("no select-all") }
        m.press(all.index)
        let boxes = m.liveParts("t", "checkbox").filter { !$0.props.flag("header") }
        XCTAssertEqual(boxes.count, 8)
        XCTAssertTrue(boxes.allSatisfy { $0.checked || $0.props.flag("checked") })
        paint(m)
        // Cells: numbers in the surface locale, booleans a tick.
        XCTAssertEqual(m.formatNumber(1234.5, precision: nil), "1,234.5")
        // Windowing past 50 rows through the core's list.
        let big = try tableSurface(rows: 200)
        paint(big)
        XCTAssertTrue(big.lists.values.contains { $0.windowed && $0.count == 200 })
        XCTAssertLessThan(rowKeys(big).count, 200)
    }

    // MARK: - CodeBlock, Chart

    /// The core tokenizer (through the FFI) matches `code-tokens.json`, a
    /// CodeBlock's lines carry exactly those tokens and the painted line is
    /// their text.
    func testCodeBlockLinesCarryTheCoreTokens() throws {
        let cases = try Fixtures.json("code-tokens.json")["cases"]?.array ?? []
        XCTAssertGreaterThanOrEqual(cases.count, 17)
        for c in cases {
            let got = JSONValue.parse(tokenizeCodeJson(code: c["code"]!.string!, language: c["language"]!.string!))
            XCTAssertEqual(got, c["expected"]!, c["name"]?.string ?? "")
        }
        let ts = cases[0]
        let m = try surface(node("cb", "CodeBlock", ["code": ts["code"]!, "language": ts["language"]!, "lineNumbers": .bool(true)]))
        paint(m)
        let lines = m.nodes.filter { $0.recipeComponent == "CodeBlock" && $0.part == "code" }
        XCTAssertEqual(lines.count, ts["expected"]?.array?.count)
        for (i, line) in lines.enumerated() {
            XCTAssertEqual(line.props["tokens"], ts["expected"]?.array?[i])
            let text = (line.props["tokens"]?.array ?? []).map { $0["text"]?.string ?? "" }.joined()
            XCTAssertEqual(line.props.str("text"), text)
        }
        XCTAssertEqual(m.node(m.index(of: "cb.copy")!)?.props.str("icon"), BuiltinIcons.name("CodeBlock.copy"))
    }

    func testChartReadsTheCoreNumbers() throws {
        let series: JSONValue = .array([.object(["name": .string("Runs"), "values": .array([3, 17, 9].map { .number($0) })]), .object(["name": .string("Fails"), "values": .array([1, 2, 4].map { .number($0) })])])
        let m = try surface(node("ch", "Chart", ["kind": .string("bar"), "categories": .array(["a", "b", "c"].map { .string($0) }), "series": series]))
        paint(m)
        let chart = ChartModel(m.node(m.index(of: "ch")!)!.props)
        let extent = JSONValue.parse(try chartExtentJson(kind: "bar", seriesJson: series.json, min: nil, max: nil))
        let ticks = JSONValue.parse(niceTicksJson(min: extent["min"]!.number!, max: extent["max"]!.number!, target: 5))
        XCTAssertEqual(chart.min, extent["min"]!.number!)
        XCTAssertEqual(chart.max, extent["max"]!.number!)
        XCTAssertEqual(chart.ticks, ticks["ticks"]?.array?.compactMap(\.number))
        XCTAssertEqual(chart.colorName(0), "chart1")
        XCTAssertEqual(chart.colorName(1), "chart2")
        XCTAssertEqual(ChartModel.legend(m.node(m.index(of: "ch")!)!.props), ["Runs", "Fails"])
        XCTAssertEqual(ChartModel.legend(["kind": .string("bar"), "series": .array([.object(["name": .string("one")])])]), [], "the legend needs 2+ series")
        XCTAssertEqual(ChartModel.legend(["kind": .string("sparkline"), "series": series]), [])
        XCTAssertEqual(ChartModel(["kind": .string("donut")]).hole, 0.6)
        XCTAssertEqual(ChartModel.format(2.5), "2.5")
        XCTAssertEqual(ChartModel.format(3), "3")
        XCTAssertEqual(ChartModel.category(at: 99, width: 100, count: 4), 3)
        XCTAssertEqual(ChartModel.slice(dx: 10, dy: 0, radius: 50, hole: 0, values: [1, 1]), 0)
        XCTAssertEqual(ChartModel.slice(dx: 10, dy: 0, radius: 50, hole: 0.6, values: [1, 1]), nil, "inside the donut hole")
    }

    // MARK: - fields

    func testNumberFieldStepsClampsAndFormatsInTheLocale() throws {
        let m = try surface(node("nf", "NumberField", ["label": .string("Seats"), "value": .number(3), "min": .number(1), "max": .number(5)]))
        paint(m)
        guard let input = m.index(of: "nf.input") else { return XCTFail("no input") }
        XCTAssertEqual(m.fieldText(input), "3")
        m.fieldEdited(input, text: "4")
        m.fieldFocused(input, false)
        XCTAssertEqual(m.node(m.index(of: "nf.input")!)?.props["value"]?.number, 4)
        XCTAssertTrue(m.fieldKey(m.index(of: "nf.input")!, key: "up"))
        XCTAssertEqual(m.node(m.index(of: "nf.input")!)?.props["value"]?.number, 5)
        XCTAssertTrue(m.fieldKey(m.index(of: "nf.input")!, key: "up", shift: true))
        XCTAssertEqual(m.node(m.index(of: "nf.input")!)?.props["value"]?.number, 5, "clamped at max")
        // A typed value beyond max clamps on commit.
        let i2 = m.index(of: "nf.input")!
        m.fieldEdited(i2, text: "42")
        m.fieldFocused(i2, false)
        XCTAssertEqual(m.node(m.index(of: "nf.input")!)?.props["value"]?.number, 5)
        XCTAssertEqual(SurfaceModel.numberPrecision(["step": .number(0.25)]), 2)
        XCTAssertEqual(SurfaceModel.numberPrecision(["step": .number(1), "precision": .number(3)]), 3)
        // The surface locale: German grouping and decimals.
        let de = try model("de")
        de.setLocale("de-DE")
        XCTAssertEqual(de.formatNumber(1234.5, precision: 1), "1.234,5")
        XCTAssertEqual(de.parseNumber("1.234,5"), 1234.5)
        XCTAssertEqual(de.surfaceWeekStart, 1)
        // A German field shows and sends the locale's notation.
        try de.setNested(json: node("nf", "NumberField", ["value": .number(1234.5), "precision": .number(1)]).json)
        de.setViewport(width: 390, height: 800)
        let dInput = try XCTUnwrap(de.index(of: "nf.input"))
        XCTAssertEqual(de.fieldText(dInput), "1.234,5")
        de.fieldEdited(dInput, text: "2.000,5")
        de.fieldFocused(dInput, false)
        XCTAssertEqual(de.node(de.index(of: "nf.input")!)?.props["value"]?.number, 2000.5)
        de.setLocale("en-US")
        XCTAssertEqual(de.surfaceWeekStart, 0, "week start from the core's CLDR table")
    }

    func testChipInputAddsAndRemovesChips() throws {
        let m = try surface(node("ci", "ChipInput", ["label": .string("Labels"), "values": .array([.string("bug")])]))
        paint(m)
        var input = m.index(of: "ci.input")!
        m.fieldEdited(input, text: "ios,")
        m.flushPending(input)
        XCTAssertEqual(m.liveParts("ci", "chipLabel").map { $0.props.str("text") }, ["bug", "ios"])
        input = m.index(of: "ci.input")!
        XCTAssertEqual(m.fieldText(input), "", "the field empties after a chip")
        m.fieldEdited(input, text: "ux")
        m.fieldCommitted(input)
        XCTAssertEqual(m.liveParts("ci", "chipLabel").count, 3)
        input = m.index(of: "ci.input")!
        XCTAssertTrue(m.fieldKey(input, key: "backspace"), "Backspace in an empty field removes the last chip")
        XCTAssertEqual(m.liveParts("ci", "chipLabel").map { $0.props.str("text") }, ["bug", "ios"])
    }

    func testSelectSearchFiltersThroughTheCore() throws {
        let options: JSONValue = .array(["Backlog", "Sprint", "Done"].map { .object(["label": .string($0), "value": .string($0.lowercased())]) })
        let m = try surface(node("sel", "Select", ["options": options, "searchable": .bool(true)]))
        m.setOpen("sel", true)
        paint(m)
        guard let search = m.index(of: "sel.search") else { return XCTFail("no search field") }
        XCTAssertEqual(m.liveParts("sel", "item").count, 3)
        m.fieldEdited(search, text: "spr")
        m.flushPending(search)
        XCTAssertEqual(m.liveParts("sel", "item").map { $0.props.str("text") }, ["Sprint"])
        paint(m)
    }

    func testDateRangePickerSelectsInTwoSteps() throws {
        let m = try surface(node("dr", "DateRangePicker", ["label": .string("Sprint"), "start": .string("2026-10-07")]))
        m.setOpen("dr", true)
        paint(m)
        let day = { (date: String) in m.nodes.first { $0.owner == "dr" && $0.part == "day" && $0.props.str("date") == date }?.index }
        guard let first = day("2026-10-20") else { return XCTFail("no calendar") }
        m.press(first)
        XCTAssertTrue(m.layers.contains { $0.owner == "dr" }, "the first pick only anchors")
        m.press(day("2026-10-12")!)
        XCTAssertFalse(m.layers.contains { $0.owner == "dr" })
        let trigger = m.node(m.index(of: "dr.trigger")!)!
        XCTAssertEqual(trigger.props.str("start"), "2026-10-12")
        XCTAssertEqual(trigger.props.str("end"), "2026-10-20")
        paint(m)
    }

    func testFileUploadTakesPlatformFiles() throws {
        let host = RecordingHost()
        let m = try surface(node("fu", "FileUpload", ["label": .string("Files"), "multiple": .bool(true)]))
        m.host = host
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("r1-upload.txt")
        try "hello".write(to: url, atomically: true, encoding: .utf8)
        m.setDragover(id: "fu.dropzone", true)
        XCTAssertTrue(m.states(of: "fu.dropzone").contains("dragover"))
        m.filesDropped(on: "fu.dropzone", urls: [url])
        XCTAssertFalse(m.states(of: "fu.dropzone").contains("dragover"))
        XCTAssertEqual(m.liveParts("fu", "fileName").map { $0.props.str("text") }, ["r1-upload.txt"])
        let upload = try XCTUnwrap(host.uploads.last, "the host gets the files (onUpload)")
        XCTAssertEqual(upload.files.map(\.url), [url])
        XCTAssertEqual(upload.files.first?.size, 5)
        XCTAssertEqual(upload.files.first?.type, "text/plain")
        paint(m)
    }

    // MARK: - menus, toasts, tables of built-ins

    func testMenusToastsAndBuiltins() throws {
        // A context Menu's region: every leaf inside finds its menu.
        let items: JSONValue = .array([
            .object(["label": .string("Copy"), "value": .string("copy"), "shortcut": .string("⌘C")]),
            .object(["kind": .string("checkbox"), "label": .string("Pinned"), "value": .string("pin"), "checked": .bool(true)]),
            .object(["kind": .string("separator")]),
            .object(["kind": .string("submenu"), "label": .string("Move"), "items": .array([.object(["label": .string("Top"), "value": .string("top")])])]),
        ])
        var tree = node("cm", "Menu", ["items": items, "openOn": .string("contextmenu")]).object!
        tree["children"] = .array([node("cm-child", "Text", ["text": .string("Child")])])
        let m = try surface(.object(tree))
        paint(m)
        m.contextMenu(m.index(of: "cm")!, at: CGPoint(x: 40, y: 10))
        XCTAssertTrue(m.layers.contains { $0.owner == "cm" })
        let painted = paint(m)
        let labels = m.liveParts("cm", "itemLabel")
        XCTAssertEqual(labels.map { $0.props.str("text") }, ["Copy", "Pinned", "Move"])
        XCTAssertTrue(labels.allSatisfy { m.node($0.index) != nil })
        XCTAssertFalse(painted.isEmpty)
        // A toast's content: the core's icon per type, title, close.
        let t = try surface(node("toast", "Toast", ["title": .string("Saved"), "type": .string("error"), "open": .bool(true)]))
        paint(t)
        XCTAssertEqual(t.node(t.index(of: "toast.icon")!)?.props.str("name"), BuiltinIcons.name("Toast.icon.error"))
        XCTAssertNotNil(t.index(of: "toast.close"))
        // Built-in strings through the core's table.
        XCTAssertEqual(m.builtinString("pageOf", ["page": .number(2), "total": .number(5)]), "Page 2 of 5")
        XCTAssertEqual(m.builtinString("copy"), "Copy")
        // Every built-in glyph name has a glyph.
        for name in Set(BuiltinIcons.slots.values) where name != "ui-icon-placeholder" {
            XCTAssertTrue(BuiltinIcons.isKnown(name), name)
        }
    }

    /// The mirrored built-in tables match their generated source and
    /// `catalog/locale.json`.
    func testBuiltinTablesMatchTheCatalog() throws {
        let generated = try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("generated/ExponentialUICatalog.generated.swift"), encoding: .utf8)
        func array(_ name: String) -> [String] {
            guard let line = generated.split(separator: "\n").first(where: { $0.contains("static let \(name):") }), let eq = line.firstIndex(of: "=") else { return [] }
            return (JSONValue.parse(String(line[line.index(after: eq)...]).trimmingCharacters(in: .whitespaces)).array ?? []).compactMap(\.string)
        }
        let slots = array("builtinIconSlots"), names = array("builtinIconNames")
        XCTAssertEqual(slots.count, names.count)
        XCTAssertFalse(slots.isEmpty)
        XCTAssertEqual(Dictionary(uniqueKeysWithValues: zip(slots, names)), BuiltinIcons.slots)
        let locale = JSONValue.parse(try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("catalog/locale.json"), encoding: .utf8))
        XCTAssertEqual(Set((locale["rtlMirroredIcons"]?.array ?? []).compactMap(\.string)), RTLGlyphs.mirrored)
    }

    // MARK: - round 3 (VAPP-102): Segmented bar, context Menu, tree guides

    /// `Segmented variant: bar` (the old TabBar): full width,
    /// `$control.tabBar` tall, each item a column; a press selects; the
    /// `TabBar` alias expands to the same native.
    func testSegmentedBarIsAFullWidthColumnRow() throws {
        let items: JSONValue = .array([
            .object(["label": .string("Inbox"), "value": .string("inbox"), "icon": .string("inbox")]),
            .object(["label": .string("Boards"), "value": .string("boards"), "icon": .string("board")]),
            .object(["label": .string("Settings"), "value": .string("settings"), "icon": .string("settings")]),
        ])
        var stack = node("col", "Stack").object!
        stack["children"] = .array([node("bar", "Segmented", ["items": items, "value": .string("inbox"), "variant": .string("bar")])])
        let m = try surface(.object(stack))
        let painted = paint(m)
        let bar = try XCTUnwrap(m.index(of: "bar"))
        XCTAssertTrue(painted.contains(bar))
        XCTAssertEqual(m.frame(bar).width, 390, accuracy: 0.5, "a bar always fills")
        XCTAssertEqual(m.frame(bar).height, m.control("tabBar", 56), accuracy: 0.5)
        XCTAssertEqual(m.segmentedValues(bar), ["inbox"])
        XCTAssertTrue(m.node(bar)!.isFocusable)
        XCTAssertTrue(A11yInfo.of(m.node(bar)!, model: m, style: m.style(bar)).passthrough, "the leaf owns its item elements")
        // The measurer: per item the wider of icon (`iconMd`) and caption, `xs` either side.
        let measurer = SurfaceMeasurer(theme: nil, mode: .light, extensions: ExtensionRegistry(), kinds: [:], generation: 0)
        let size = measurer.measureLeaf(LeafRequest(component: "Segmented", props: ["items": items, "variant": .string("bar")]), wrap: nil)
        XCTAssertEqual(size.height, 56)
        let caption = SurfaceMeasurer.barCaption(.empty, base: TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: nil))
        let expected = ["Inbox", "Boards", "Settings"].reduce(CGFloat(0)) { $0 + max(TextShaper.maxContent($1, caption), 20) + 2 * GeometrySpacing.value("xs") }
        XCTAssertEqual(size.width, expected, accuracy: 0.5)
        // The alias rides the same native.
        var aliasStack = node("col", "Stack").object!
        aliasStack["children"] = .array([node("tb", "TabBar", ["items": items, "value": .string("boards")])])
        let alias = try surface(.object(aliasStack))
        paint(alias)
        let seg = try XCTUnwrap(alias.nodes.first { $0.component == "Segmented" && !$0.removed })
        XCTAssertEqual(seg.props.str("variant"), "bar")
        XCTAssertEqual(alias.segmentedValues(seg.index), ["boards"])
    }

    /// `Menu openOn: contextmenu`: the ONE child is the region; a secondary
    /// click / long press there opens the core's menu layer at the pointer;
    /// a press Menu with no child synthesizes its outline Button trigger.
    func testMenuContextmenuAndPressTrigger() throws {
        let items: JSONValue = .array([
            .object(["label": .string("Copy"), "value": .string("copy")]),
            .object(["label": .string("Archive"), "value": .string("archive")]),
        ])
        var tree = node("cm", "Menu", ["items": items, "openOn": .string("contextmenu")]).object!
        tree["children"] = .array([node("cm-child", "Text", ["text": .string("Right-click me")])])
        let m = try surface(.object(tree))
        paint(m)
        let cm = try XCTUnwrap(m.node(m.index(of: "cm")!))
        XCTAssertEqual(cm.component, "Menu")
        XCTAssertEqual(cm.props.str("openOn"), "contextmenu", "NativeHooks keys the gesture on this")
        XCTAssertFalse(m.layers.contains { $0.owner == "cm" })
        m.contextMenu(m.index(of: "cm-child")!, at: CGPoint(x: 30, y: 8))
        let layer = try XCTUnwrap(m.layers.first { $0.owner == "cm" })
        XCTAssertEqual(layer.kind, "Menu")
        paint(m)
        XCTAssertEqual(m.liveParts("cm", "itemLabel").map { $0.props.str("text") }, ["Copy", "Archive"])
        let content = try XCTUnwrap(m.nodes.first { !$0.removed && $0.owner == "cm" && $0.part == "content" })
        XCTAssertEqual(A11yInfo.role(of: content, model: m), "menu")
        // A press Menu with no child: the default outline Button trigger.
        let p = try surface(node("pm", "Menu", ["items": items, "label": .string("More")]))
        paint(p)
        let trigger = try XCTUnwrap(p.nodes.first { !$0.removed && $0.owner == "pm" && $0.part == "trigger" })
        XCTAssertEqual(trigger.component, "Button")
        p.press(trigger.index)
        XCTAssertTrue(p.layers.contains { $0.owner == "pm" })
        // The ContextMenu alias still opens as a context Menu.
        var alias = node("ctx", "ContextMenu", ["items": items]).object!
        alias["children"] = .array([node("ctx-child", "Text", ["text": .string("Region")])])
        let a = try surface(.object(alias))
        paint(a)
        a.contextMenu(a.index(of: "ctx-child")!)
        XCTAssertTrue(a.layers.contains { $0.owner == "ctx" })
    }

    /// The round-3 guide geometry: 14 px columns, the line's LEFT edge at
    /// `i·14 + 7`, a 3 px rounded elbow, verticals bridged 1 px above the
    /// part; the constants match `layout.json` (generated); a tree Section's
    /// rows get their guides from the core.
    func testTreeGuidesGeometry() throws {
        let generated = try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("generated/ExponentialUICatalog.generated.swift"), encoding: .utf8)
        func array(_ name: String) -> [String] {
            guard let line = generated.split(separator: "\n").first(where: { $0.contains("static let \(name):") }), let eq = line.firstIndex(of: "=") else { return [] }
            return (JSONValue.parse(String(line[line.index(after: eq)...]).trimmingCharacters(in: .whitespaces)).array ?? []).compactMap(\.string)
        }
        let layout = Dictionary(uniqueKeysWithValues: zip(array("layoutConstantNames"), array("layoutConstantValues")))
        XCTAssertEqual(layout["treeGuideColumn"].flatMap(Double.init).map { CGFloat($0) }, SurfaceMeasurer.treeGuideColumn)
        XCTAssertEqual(layout["treeGuideRadius"].flatMap(Double.init).map { CGFloat($0) }, SurfaceMeasurer.treeGuideRadius)
        XCTAssertEqual(layout["treeGuideBridge"].flatMap(Double.init).map { CGFloat($0) }, SurfaceMeasurer.treeGuideBridge)
        XCTAssertEqual([SurfaceMeasurer.treeGuideColumn, SurfaceMeasurer.treeGuideRadius, SurfaceMeasurer.treeGuideBridge], [14, 3, 1])
        // Depth 2, elbow in column 1, column 0 passes through, 32 px row.
        XCTAssertEqual(TreeGuideGeometry.segments(depth: 2, elbowAt: 1, tee: false, passThrough: [0], height: 32), [
            .vertical(x: 7, from: -1, to: 32),
            .vertical(x: 21, from: -1, to: 13),
            .elbow(x: 21, mid: 16, stubEnd: 28),
        ])
        // A tee continues to the bottom.
        XCTAssertEqual(TreeGuideGeometry.segments(depth: 1, elbowAt: 0, tee: true, passThrough: [], height: 28), [
            .vertical(x: 7, from: -1, to: 28),
            .elbow(x: 7, mid: 14, stubEnd: 14),
        ])
        let measurer = SurfaceMeasurer(theme: nil, mode: .light, extensions: ExtensionRegistry(), kinds: [:], generation: 0)
        XCTAssertEqual(measurer.measureLeaf(LeafRequest(component: "TreeGuides", props: ["depth": .number(2)]), wrap: nil).width, 28)
        // The core fills a tree Section's guides from consecutive depths.
        func row(_ id: String, _ depth: Double) -> JSONValue { node(id, "Row", ["title": .string(id), "depth": .number(depth)]) }
        var section = node("sec", "Section", ["title": .string("Runs"), "tree": .bool(true)]).object!
        section["children"] = .array([row("a", 0), row("b", 1), row("c", 2), row("d", 1)])
        let m = try surface(.object(section))
        let painted = paint(m)
        let guides = m.nodes.filter { !$0.removed && $0.component == "TreeGuides" }
        XCTAssertEqual(guides.count, 3)
        XCTAssertTrue(guides.allSatisfy { painted.contains($0.index) })
        let byDepth = guides.map { ($0.props.num("depth") ?? 0, $0.props.flag("tee"), $0.props.list("passThrough").compactMap(\.number)) }
        XCTAssertEqual(byDepth.map(\.0), [1, 2, 1])
        XCTAssertEqual(byDepth.map(\.1), [true, false, false], "b tees on to its sibling d")
        XCTAssertEqual(byDepth.map(\.2), [[], [0], []], "c's column 0 passes through to d")
        for g in guides {
            XCTAssertEqual(m.frame(g.index).width, CGFloat(g.props.num("depth") ?? 0) * 14, accuracy: 0.5)
        }
    }

    /// A debugging aid: `R1_SHOTS=<dir>` renders the round-1 specimens
    /// (macOS) to PNGs there; `R1_ONLY=<Component,…>` narrows the set.
    func testRenderSpecimenShots() throws {
        guard let dir = ProcessInfo.processInfo.environment["R1_SHOTS"] else { throw XCTSkip("R1_SHOTS unset") }
        #if canImport(AppKit)
        let only = ProcessInfo.processInfo.environment["R1_ONLY"].map { Set($0.split(separator: ",").map(String.init)) }
        let specimens = try Fixtures.json("specimens.json")["specimens"]?.array ?? []
        for spec in specimens {
            let component = spec["component"]?.string ?? ""
            if let only, !only.contains(component) { continue }
            for mode in [Mode.light, .dark] {
                let m = try model(component, mode: mode)
                try m.setNested(json: spec["node"]!.json)
                m.setViewport(width: 390, height: 900)
                let height = max(m.surfaceSize.height, 200)
                let root = ExponentialSurface(model: m).frame(width: 390, height: height).background(m.color("background") ?? .white)
                let view = NSHostingView(rootView: root)
                view.frame = CGRect(x: 0, y: 0, width: 390, height: height)
                view.layoutSubtreeIfNeeded()
                guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { continue }
                view.cacheDisplay(in: view.bounds, to: rep)
                try rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: dir).appendingPathComponent("\(component)-\(mode.rawValue).png"))
            }
        }
        #endif
    }

    /// A debugging aid: `R1_DUMP=<path>` writes every specimen's nodes there.
    func testDumpSpecimenNodes() throws {
        guard let path = ProcessInfo.processInfo.environment["R1_DUMP"] else { throw XCTSkip("R1_DUMP unset") }
        let specimens = try Fixtures.json("specimens.json")["specimens"]?.array ?? []
        var out = ""
        for spec in specimens {
            let m = try model(spec["id"]?.string ?? "?")
            try m.setNested(json: spec["node"]!.json)
            m.setViewport(width: 390, height: 900)
            out += "== \(spec["component"]?.string ?? "")\n"
            for n in m.nodes {
                let f = m.frame(n.index)
                out += "\(String(repeating: "  ", count: n.depth))\(n.id) [\(n.component)\(n.part.map { "." + $0 } ?? "")] owner=\(n.ownerComponent ?? "-") leaf=\(n.isLeaf) L\(n.layer) \(Int(f.minX)),\(Int(f.minY)) \(Int(f.width))x\(Int(f.height)) \(n.props.json.prefix(160))\n"
            }
        }
        try out.write(toFile: path, atomically: true, encoding: .utf8)
    }
}
