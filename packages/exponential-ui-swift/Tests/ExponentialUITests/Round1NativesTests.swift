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
        renderInWindow(root, size: CGSize(width: width, height: max(m.surfaceSize.height, height)))
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
        for name in ["Rating", "Stepper", "Breadcrumb", "Row", "Section", "Chip", "AppBar", "Kbd", "KbdGroup", "Label", "Sparkline"] {
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

    /// N-12: THE DockCard (`exponential-ui-dock-card`, the kitchen sink's
    /// "dock-card"): the macro expands to the pressable trigger row over a
    /// bottom Drawer, the `footer` slot rides the sheet, and the painter
    /// draws the open sheet's body and footer in the Drawer's layer.
    func testDockCardExpandsToTriggerOverABottomDrawerWithItsFooter() throws {
        let specimens = try Fixtures.json("specimens.json")["specimens"]?.array ?? []
        let spec = try XCTUnwrap(specimens.first { $0["id"]?.string == "exponential-ui-dock-card" })
        let m = try model("dock-card")
        try m.setNested(json: try XCTUnwrap(spec["node"]).json)
        let painted = paint(m)
        XCTAssertNil(m.nodes.first { $0.component == "Unknown" })
        let trigger = try XCTUnwrap(m.nodes.first { $0.id == "c0_dock_card.trigger" })
        XCTAssertTrue(painted.contains(trigger.index), "the collapsed card paints")
        XCTAssertEqual(trigger.layer, 0)
        let drawer = try XCTUnwrap(m.layers.first { $0.kind == "Drawer" }, "the open card's sheet is a Drawer layer")
        XCTAssertEqual(drawer.owner, "c0_dock_card.sheet")
        XCTAssertEqual(drawer.position, "bottom")
        // The `footer` slot lands in the Drawer's own footer part.
        let footerPart = try XCTUnwrap(m.nodes.first { $0.id == "c0_dock_card.sheet.footer" })
        XCTAssertEqual(footerPart.part, "footer")
        XCTAssertEqual(m.nodes.first { $0.id == "c0_footer" }?.parent, footerPart.index)
        XCTAssertTrue(drawer.modal)
        for id in ["c0_p", "c0_no", "c0_yes"] {
            let n = try XCTUnwrap(m.nodes.first { $0.id == id }, id)
            XCTAssertGreaterThan(n.layer, 0, "\(id) rides the sheet")
            XCTAssertTrue(painted.contains(n.index), "\(id) paints")
        }
        // The footer sits under the body inside the sheet.
        let body = try XCTUnwrap(m.nodes.first { $0.id == "c0_p" })
        let yes = try XCTUnwrap(m.nodes.first { $0.id == "c0_yes" })
        XCTAssertGreaterThan(m.frame(yes.index).minY, m.frame(body.index).minY)
        XCTAssertGreaterThanOrEqual(m.frame(yes.index).minY, drawer.frame.minY)
    }

    /// The kitchen sink's "dock-card" opens its Drawer from the trigger.
    func testKitchenSinkDockCardOpensItsDrawer() throws {
        let m = try model("dock-sink")
        try m.setNested(json: Fixtures.text("kitchen-sink.json"))
        m.setViewport(width: 390, height: 900)
        XCTAssertNotNil(m.nodes.first { $0.id == "dock_card.trigger" })
        XCTAssertFalse(m.layers.contains { $0.kind == "Drawer" }, "closed until pressed")
        let sheet = try XCTUnwrap(m.nodes.first { $0.id == "dock_card.sheet" && $0.component == "Drawer" })
        m.setOpen(sheet.id, true)
        let drawer = try XCTUnwrap(m.layers.first { $0.kind == "Drawer" })
        XCTAssertEqual(drawer.position, "bottom")
        let footer = try XCTUnwrap(m.nodes.first { $0.id == "dock_card_yes" })
        XCTAssertGreaterThan(footer.layer, 0)
        XCTAssertGreaterThan(m.frame(footer.index).height, 0)
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
        // #EXP-1629: a 12k-char line without a break opportunity (a long URL,
        // a hash) wraps by character at the field's laid-out width (the core
        // asks an autosize field at that width): rows 10 grow to maxRows 40.
        let hash = String(repeating: "a", count: 12_000)
        let grown = measurer.measureLeaf(LeafRequest(component: "Textarea", part: "field", props: ["rows": .number(10), "maxRows": .number(40), "autosize": .bool(true), "value": .string(hash)]), wrap: 360)
        XCTAssertEqual(grown.height, 800)
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

    /// #EXP-1629: a `row` Select's `description` stacks under its label in
    /// the `labels` column; the value and chevron sit in the `control`
    /// column; the row grows past its 40.82 to hold the hint (10.5 around).
    func testSelectRowDescription() throws {
        let options: JSONValue = .array(["English", "Deutsch"].map { .object(["label": .string($0), "value": .string($0)]) })
        let m = try surface(node("lang", "Select", ["label": .string("Language"), "description": .string("Spoken replies and dictation use it"), "triggerVariant": .string("row"), "value": .string("English"), "options": options]))
        paint(m)
        let trigger = try XCTUnwrap(m.index(of: "lang.trigger"))
        XCTAssertEqual(m.nodes.filter { $0.parent == trigger && !$0.removed }.map(\.id), ["lang.labels", "lang.control"])
        let labels = try XCTUnwrap(m.index(of: "lang.labels"))
        XCTAssertEqual(m.nodes.filter { $0.parent == labels && !$0.removed }.map(\.id), ["lang.label", "lang.description"])
        let hint = m.part("Select", "description", props: m.ownerProps(trigger))
        XCTAssertEqual(hint.style.opacity ?? 1, 0.5, accuracy: 0.001, "the IDE's 50 % hint")
        let (t, l, d) = (m.frame(trigger), m.frame(m.index(of: "lang.label")!), m.frame(m.index(of: "lang.description")!))
        XCTAssertGreaterThanOrEqual(d.minY, l.maxY)
        XCTAssertEqual(t.height, 21 + (d.maxY - l.minY), accuracy: 0.01)
    }

    /// #EXP-1629: a `row` Select's trigger is the core's container of parts
    /// (label, spacer, value, chevron); opening puts the cursor (the ONE
    /// `hover`) on the FIRST enabled option and focus in the search field;
    /// the pointer and the arrows move it, `selected` stays the chosen one;
    /// ↑/↓ in a searchable Select's field move it too and Enter picks it.
    func testSelectRowTriggerAndTheCursor() throws {
        let options: JSONValue = .array(["Monday", "Tuesday", "Sunday"].map { .object(["label": .string($0), "value": .string($0.lowercased())]) })
        let m = try surface(node("week", "Select", ["label": .string("Week starts on"), "triggerVariant": .string("row"), "value": .string("tuesday"), "options": options]))
        paint(m)
        let trigger = try XCTUnwrap(m.index(of: "week.trigger"))
        XCTAssertEqual(m.nodes.filter { $0.parent == trigger && !$0.removed }.map(\.id), ["week.label", "week.spacer", "week.button"])
        // #EXP-1629: the IDE picker's ghost value button holds the value and the chevron.
        let button = try XCTUnwrap(m.index(of: "week.button"))
        XCTAssertEqual(m.nodes.filter { $0.parent == button && !$0.removed }.map(\.id), ["week.value", "week.chevron"])
        XCTAssertEqual(m.node(m.index(of: "week.label")!)?.props.str("text"), "Week starts on")
        XCTAssertEqual(m.node(m.index(of: "week.value")!)?.props.str("text"), "Tuesday")
        let chevron = m.part("Select", "chevron", props: m.ownerProps(trigger))
        XCTAssertEqual(SelectTrigger.glyphSize(chevron), 14, "the row recipe's chevron")
        XCTAssertEqual(chevron.style.opacity ?? 1, 0.7, accuracy: 0.001)
        XCTAssertEqual(m.part("Select", "value", props: m.ownerProps(trigger)).style.opacity ?? 1, 0.7, accuracy: 0.001)
        XCTAssertNil(m.part("Select", "chevron", props: [:]).style.opacity, "the field glyph keeps its look")

        m.setOpen("week", true)
        paint(m)
        let hovered = { m.liveParts("week", "item").filter { m.states(of: $0.id).contains("hover") }.map(\.id) }
        XCTAssertEqual(hovered(), ["week.item.0"], "the cursor starts on the first option: a fresh pick")
        m.setHover(id: "week.item.2", true)
        XCTAssertEqual(hovered(), ["week.item.2"], "the pointer moves the one cursor")
        m.setHover(id: "week.item.2", false)
        XCTAssertEqual(hovered(), ["week.item.2"])
        XCTAssertEqual(m.liveParts("week", "item").filter(\.selected).map(\.id), ["week.item.1"], "the cursor is never `selected`")
        m.requestFocus("week.item.2", keyboard: true)
        m.handleKey(key: "up")
        XCTAssertEqual(hovered(), ["week.item.1"], "the arrows step from the cursor")
        m.handleKey(key: "enter")
        XCTAssertFalse(m.layers.contains { $0.owner == "week" })

        let zones: JSONValue = .array(["Europe/Berlin", "Europe/London", "Europe/Vienna", "UTC"].map { .object(["label": .string($0), "value": .string($0)]) })
        let s = try surface(node("tz", "Select", ["label": .string("Timezone"), "triggerVariant": .string("row"), "searchable": .bool(true), "value": .string("Europe/Vienna"), "options": zones]))
        // No `paint` here: the open popup's focus is in its search FIELD, and
        // on iOS `paint` renders into a throwaway UIWindow whose teardown
        // takes the first responder back: the field blurs and the popup
        // closes (the release suite's iOS Simulator leg). `setOpen` already
        // ran the pass the assertions read.
        s.setOpen("tz", true)
        let tzHovered = { s.liveParts("tz", "item").filter { s.states(of: $0.id).contains("hover") }.map(\.id) }
        XCTAssertEqual(tzHovered(), ["tz.item.0"])
        XCTAssertEqual(s.focusedId, "tz.search", "focus in the search field")
        let search = try XCTUnwrap(s.index(of: "tz.search"))
        for _ in 0..<2 { s.fieldKey(search, key: "down") }
        XCTAssertTrue(s.fieldKey(search, key: "down"))
        XCTAssertEqual(tzHovered(), ["tz.item.3"])
        s.fieldKey(search, key: "down")
        XCTAssertEqual(tzHovered(), ["tz.item.3"], "no wrap")
        s.fieldCommitted(search)
        XCTAssertEqual(s.node(s.index(of: "tz.trigger")!)?.props.str("text"), "UTC")
    }

    // MARK: - #EXP-1765: the `pill` Select, `loading`, `mono` options

    func pillSelect(_ extra: [String: JSONValue] = [:]) -> JSONValue {
        var props: [String: JSONValue] = [
            "label": .string("Default branch"), "triggerVariant": .string("pill"), "value": .string("main"),
            "options": .array([
                .object(["label": .string("main"), "value": .string("main"), "mono": .bool(true), "trailing": .string("default")]),
                .object(["label": .string("develop"), "value": .string("develop"), "mono": .bool(true)]),
                .object(["label": .string("plain"), "value": .string("plain"), "trailing": .string("· x")]),
            ]),
        ]
        for (k, v) in extra { props[k] = v }
        return node("sel", "Select", props)
    }

    /// The pill trigger: the field trigger's parts content-sized, no label
    /// part, the `Select.dropdown` glyph at the chevron part's size and
    /// colour at full opacity, the label as its accessible name; the
    /// trigger's `disabled` reaches the value part.
    func testSelectPillTriggerPartsAndName() throws {
        let m = try surface(pillSelect())
        paint(m)
        let trigger = try XCTUnwrap(m.index(of: "sel.trigger"))
        let t = try XCTUnwrap(m.node(trigger))
        XCTAssertTrue(t.isLeaf, "the field trigger's leaf, never the row's parts")
        XCTAssertNil(m.index(of: "sel.label"), "no label part")
        XCTAssertNil(m.index(of: "sel.description"))
        XCTAssertEqual(t.props.str("icon"), BuiltinIcons.name("Select.dropdown"))
        XCTAssertEqual(BuiltinIcons.name("Select.dropdown"), "ui-chevron-down")
        XCTAssertLessThan(m.frame(trigger).width, 390 - 32, "content-sized, never stretched")
        XCTAssertEqual(m.frame(trigger).height, 24, accuracy: 0.01)
        XCTAssertTrue(SelectTrigger.isPill(t.props, owner: m.ownerProps(trigger)))
        let chevron = m.part("Select", "chevron", props: m.ownerProps(trigger))
        XCTAssertEqual(SelectTrigger.glyphSize(chevron), 10.5)
        XCTAssertNotNil(chevron.color)
        XCTAssertEqual(chevron.style.opacity ?? 1, 1, accuracy: 0.001, "the pill's glyph paints at 1 (the field's default 0.6 stays the field's)")
        XCTAssertEqual(m.part("Select", "value", props: m.ownerProps(trigger)).style.opacity ?? 1, 0.7, accuracy: 0.001)
        let off = try surface(pillSelect(["disabled": .bool(true)]))
        let offTrigger = try XCTUnwrap(off.index(of: "sel.trigger"))
        XCTAssertTrue(off.isDisabled(offTrigger))
        XCTAssertEqual(off.part("Select", "value", props: off.ownerProps(offTrigger), states: ["disabled"]).style.opacity ?? 1, 0.5, accuracy: 0.001, "the theme dims a disabled pill's value")
        let a = A11yInfo.of(t, model: m, style: m.style(trigger))
        XCTAssertEqual(a.role, "combobox")
        XCTAssertEqual(a.label, "Default branch")

        // A field Select keeps its stretched trigger and its label part.
        let f = try surface(node("f", "Select", ["label": .string("Branch"), "options": .array([])]))
        XCTAssertNotNil(f.index(of: "f.label"))
        XCTAssertFalse(SelectTrigger.isPill(f.node(f.index(of: "f.trigger")!)!.props))
    }

    /// The pill popup: `selectRowPopupWidth` wide, its border box
    /// `selectPillPopupOffset` under the pill and `selectPillPopupInset`
    /// before its start edge (align start; `end` = past its right edge).
    func testSelectPillPopupGeometry() throws {
        let m = try surface(.object(["id": .string("root"), "component": .string("Box"), "style": .object(["display": .string("flex"), "flexDirection": .string("column"), "padding": .number(16)]), "children": .array([pillSelect(["searchable": .bool(true)])])]))
        m.setOpen("sel", true)
        let t = m.frame(try XCTUnwrap(m.index(of: "sel.trigger")))
        let c = m.frame(try XCTUnwrap(m.index(of: "sel.content")))
        XCTAssertEqual(c.width, 262, accuracy: 0.01)
        XCTAssertEqual(c.minX, t.minX - 1, accuracy: 0.01)
        XCTAssertEqual(c.minY, t.maxY + 3, accuracy: 0.01)

        let e = try surface(.object(["id": .string("root"), "component": .string("Box"), "style": .object(["display": .string("flex"), "flexDirection": .string("column"), "alignItems": .string("flex-end"), "padding": .number(16)]), "children": .array([pillSelect(["align": .string("end"), "popupWidth": .number(200)])])]))
        e.setOpen("sel", true)
        let et = e.frame(try XCTUnwrap(e.index(of: "sel.trigger")))
        let ec = e.frame(try XCTUnwrap(e.index(of: "sel.content")))
        XCTAssertEqual(ec.width, 200, accuracy: 0.01)
        // #EXP-1765: inset, clamp, then the content's offset from the pill
        // rounded to whole points (half away from zero): a content-sized pill
        // leaves the box within half a point of flush.
        let at = et.maxX + 1 - 200
        XCTAssertEqual(ec.minX, et.minX + (at + 1 - et.minX).rounded() - 1, accuracy: 0.01)
        XCTAssertEqual(ec.maxX, et.maxX + 1, accuracy: 0.5)
        XCTAssertLessThanOrEqual(ec.maxX, 390 - 7 + 0.01, "selectRowPopupMargin from the view's edge")
    }

    /// Opened by the POINTER a pill shows no cursor until the pointer moves
    /// over an option or a key goes (↓ = the first enabled option); opened
    /// from the KEYBOARD it keeps the cursor; a field Select is unchanged.
    func testSelectPillPointerOpenHasNoCursor() throws {
        let hovered = { (m: SurfaceModel) in m.liveParts("sel", "item").filter { m.states(of: $0.id).contains("hover") }.map(\.id) }
        let m = try surface(pillSelect())
        m.pressDown("sel.trigger")
        m.pressUp("sel.trigger")
        m.setOpen("sel", true)
        XCTAssertEqual(hovered(m), [], "no cursor on a pointer open")
        m.setHover(id: "sel.item.1", true)
        XCTAssertEqual(hovered(m), ["sel.item.1"], "the pointer brings it")

        let k = try surface(pillSelect())
        k.setOpen("sel", true)
        XCTAssertEqual(hovered(k), [])
        k.requestFocus("sel.trigger", keyboard: false)
        XCTAssertTrue(k.handleKey(key: "down"))
        XCTAssertEqual(hovered(k), ["sel.item.0"], "↓ lands on the first enabled option")
        XCTAssertEqual(k.focusedId, "sel.item.0", "no search field: the keys go on from the option")
        k.handleKey(key: "down")
        XCTAssertEqual(hovered(k), ["sel.item.1"])

        // The search field: its first ↓ lands on the first option.
        let s = try surface(pillSelect(["searchable": .bool(true)]))
        s.setOpen("sel", true)
        XCTAssertEqual(s.focusedId, "sel.search")
        XCTAssertEqual(hovered(s), [])
        let search = try XCTUnwrap(s.index(of: "sel.search"))
        XCTAssertTrue(s.fieldKey(search, key: "down"))
        XCTAssertEqual(hovered(s), ["sel.item.0"])

        // From the keyboard: today's cursor.
        let kb = try surface(pillSelect())
        kb.requestFocus("sel.trigger", keyboard: true)
        kb.setOpen("sel", true)
        XCTAssertEqual(hovered(kb), ["sel.item.0"])

        // A field Select: the cursor on a pointer open, as before.
        let f = try surface(pillSelect(["triggerVariant": .string("field")]))
        f.pressDown("sel.trigger")
        f.setOpen("sel", true)
        XCTAssertEqual(hovered(f), ["sel.item.0"])
    }

    /// `loading`: the list holds a Skeleton (`<id>.skeleton`, its
    /// `selectLoadingRows` rows `$control.row` tall) in place of the options
    /// and of `empty`; the search still shows; a loading Select is not inert.
    func testSelectLoadingSkeletonAndStillOpens() throws {
        let m = try surface(pillSelect(["searchable": .bool(true), "loading": .bool(true), "options": .array([])]))
        paint(m)
        let trigger = try XCTUnwrap(m.index(of: "sel.trigger"))
        XCTAssertFalse(m.isDisabled(trigger), "loading never freezes a Select")
        m.press(trigger)
        XCTAssertTrue(m.layers.contains { $0.owner == "sel" }, "a loading Select still opens")
        let skeleton = try XCTUnwrap(m.index(of: "sel.skeleton"))
        XCTAssertEqual(m.node(skeleton)?.component, "Skeleton")
        let rows = m.nodes.filter { !$0.removed && $0.owner == "sel.skeleton" && $0.part == "row" }
        XCTAssertEqual(rows.count, 3)
        for r in rows { XCTAssertEqual(m.frame(r.index).height, m.control("row", 32), accuracy: 0.01) }
        XCTAssertNil(m.index(of: "sel.empty"))
        XCTAssertTrue(m.liveParts("sel", "item").isEmpty)
        XCTAssertNotNil(m.index(of: "sel.search"))
        paint(m)
        // A loading Button still freezes.
        let b = try surface(node("b", "Button", ["label": .string("Save"), "loading": .bool(true)]))
        XCTAssertTrue(b.isDisabled(try XCTUnwrap(b.index(of: "b"))))
    }

    /// A `mono` option: the label alone in the mono type, the `trailing`
    /// its own run (measured in the sans type before the check slot) and
    /// out of the accessible name; a plain option reads it on.
    func testSelectMonoOptionTrailing() throws {
        let m = try surface(pillSelect())
        m.setOpen("sel", true)
        paint(m)
        let main = try XCTUnwrap(m.node(m.index(of: "sel.item.0")!))
        XCTAssertEqual(main.props.str("text"), "main")
        XCTAssertEqual(main.props.str("trailing"), "default")
        let ts = m.textStyle(main.index)
        XCTAssertNotEqual(ts.fontFamily, m.theme?.sansFamily, "the mono type")
        XCTAssertEqual(SelectTrigger.trailingStyle(ts, sans: m.theme?.sansFamily).fontFamily, m.theme?.sansFamily)
        XCTAssertEqual(A11yInfo.of(main, model: m, style: m.style(main.index)).label, "main", "the trailing stays out of the name")
        let plain = try XCTUnwrap(m.node(m.index(of: "sel.item.2")!))
        XCTAssertEqual(plain.props.str("text"), "plain · x")
        XCTAssertNil(plain.props["trailing"])
        // The trailing's room before the check slot (the measurer's chrome).
        let gap: CGFloat = 7
        let (_, trail) = SurfaceMeasurer.textChrome("Select", "item", ["trailing": .string("default"), "disabled": .bool(false)], gap: gap, countWidth: 30)
        XCTAssertEqual(trail, 16 + 8 + 7 + 30, accuracy: 0.01)
        XCTAssertGreaterThan(m.frame(main.index).width, 0)
    }

    /// `searchPlaceholder` names the search field; the `searchIcon` part
    /// sizes its glyph; the pill's `empty` text centres (`textAlign`).
    func testSelectSearchPlaceholderIconAndEmpty() throws {
        let m = try surface(pillSelect(["searchable": .bool(true), "searchPlaceholder": .string("Search branches..."), "options": .array([])]))
        m.setOpen("sel", true)
        paint(m)
        let search = try XCTUnwrap(m.node(m.index(of: "sel.search")!))
        XCTAssertEqual(search.props.str("placeholder"), "Search branches...")
        XCTAssertEqual(m.part("Select", "searchIcon", props: m.ownerProps(search.index)).width, 14)
        let f = m.frame(search.index), st = m.style(search.index)
        XCTAssertEqual(f.height, 32.5, accuracy: 0.01)
        XCTAssertEqual(st.insets.bottom, 3.5 + 1, accuracy: 0.01, "paddingBottom + the hairline")
        XCTAssertEqual(st.insets.top, 0, accuracy: 0.01)
        let empty = try XCTUnwrap(m.index(of: "sel.empty"))
        XCTAssertEqual(m.style(empty).textAlign, "center")

        let d = try surface(node("d", "Select", ["searchable": .bool(true), "options": .array([])]))
        d.setOpen("d", true)
        XCTAssertEqual(d.node(d.index(of: "d.search")!)?.props.str("placeholder"), d.builtinString("search"))
    }

    /// The pill search's clear button: only while it holds text, a button
    /// named "Clear search"; pressing it empties the field (the core hears
    /// "", the list unfiltered) and keeps the focus in it; never on a field.
    func testSelectPillSearchClearButton() throws {
        let m = try surface(pillSelect(["searchable": .bool(true)]))
        m.setOpen("sel", true)
        let search = try XCTUnwrap(m.index(of: "sel.search"))
        XCTAssertFalse(m.selectSearchClearable(search), "no clear button while empty")
        m.fieldEdited(search, text: "dev")
        XCTAssertTrue(m.selectSearchClearable(search))
        m.flushPending(search)
        XCTAssertEqual(m.liveParts("sel", "item").map { $0.props.str("text") }, ["develop"])
        XCTAssertEqual(m.builtinString("clearSearch"), "Clear search")
        XCTAssertEqual(BuiltinIcons.name("Select.clear"), "ui-clear")
        XCTAssertTrue(BuiltinIcons.isKnown("ui-clear"))
        XCTAssertEqual(14 * SelectTrigger.clearBox, 17.5, accuracy: 0.001)
        XCTAssertEqual(14 * SelectTrigger.clearGlyph, 10.5, accuracy: 0.001)
        m.selectSearchClear(search)
        XCTAssertEqual(m.fieldText(search), "")
        XCTAssertFalse(m.selectSearchClearable(search))
        XCTAssertEqual(m.liveParts("sel", "item").count, 3, "the list unfiltered at once")
        XCTAssertEqual(m.focusedId, "sel.search", "the focus stays in the field")

        let f = try surface(node("f", "Select", ["searchable": .bool(true), "options": .array([.object(["label": .string("a"), "value": .string("a")])])]))
        f.setOpen("f", true)
        let fs = try XCTUnwrap(f.index(of: "f.search"))
        f.fieldEdited(fs, text: "a")
        XCTAssertFalse(f.selectSearchClearable(fs), "a field Select's search has none")
    }

    /// #EXP-1629: a `row` Input IS a Group row: no label above, the field
    /// carries the label (its row-weight `label` part) and the IDE metrics,
    /// measures label + gap + field; the default Input keeps its label.
    func testInputRowCarriesItsLabelInsideTheField() throws {
        let m = try surface(node("mail", "Input", ["label": .string("Email"), "placeholder": .string("you@example.com"), "variant": .string("row")]))
        let painted = paint(m)
        XCTAssertNil(m.index(of: "mail.label"), "a row has no label above")
        let field = try XCTUnwrap(m.index(of: "mail.field"))
        XCTAssertTrue(painted.contains(field))
        let props = m.node(field)!.props
        XCTAssertTrue(InputRow.isRow(props))
        XCTAssertEqual(props.str("label"), "Email")
        XCTAssertEqual(m.frame(field).height, 40.82, accuracy: 0.01)
        XCTAssertEqual(m.style(field).gap, 10.5, accuracy: 0.01)
        XCTAssertEqual(m.style(field).paddingHorizontal, 14, accuracy: 0.01)
        let look = m.part("Input", "label", props: m.ownerProps(field))
        let labelStyle = InputRow.labelStyle(m.textStyle(field), look)
        XCTAssertEqual(labelStyle.fontSize, 12.25, accuracy: 0.001)
        XCTAssertEqual(labelStyle.lineHeight, 19.82, accuracy: 0.001)
        XCTAssertEqual(look.style.fontWeight, 400, "the row label is regular")
        XCTAssertEqual(InputRow.labelWidth("Email", labelStyle, width: 4), 4, "at most the row")
        XCTAssertEqual(InputRow.valueOpacity, 0.7, accuracy: 0.001)

        let measurer = SurfaceMeasurer(theme: m.theme, mode: m.mode, extensions: ExtensionRegistry(), kinds: [:], generation: 0)
        let ts = TextStyle.body
        let rowStyle = InputRow.labelStyle(ts, m.part("Input", "label", props: ["variant": .string("row")]))
        let row = measurer.measureLeaf(LeafRequest(component: "Input", part: "field", props: ["label": .string("Email"), "variant": .string("row")], textStyle: ts, control: ControlBox(gap: 12)), wrap: nil)
        XCTAssertEqual(row.width, TextShaper.width("Email", rowStyle) + 12 + 160, accuracy: 0.01)
        XCTAssertEqual(row.height, max(ts.lineHeight, rowStyle.lineHeight), accuracy: 0.01)
        let plain = measurer.measureLeaf(LeafRequest(component: "Input", part: "field", props: ["label": .string("Email")], textStyle: ts, control: ControlBox(gap: 12)), wrap: nil)
        XCTAssertEqual(plain.width, 160, accuracy: 0.01, "the default field is unchanged")

        let d = try surface(node("mail", "Input", ["label": .string("Email"), "placeholder": .string("you@example.com")]))
        paint(d)
        XCTAssertNotNil(d.index(of: "mail.label"), "the default Input keeps its label")
        XCTAssertFalse(InputRow.isRow(d.node(d.index(of: "mail.field")!)!.props))
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
        // #EXP-1764: the own-colour marks.
        XCTAssertEqual(Dictionary(uniqueKeysWithValues: zip(array("iconFillNames"), array("iconFillColors"))), BuiltinIcons.fills)
        let locale = JSONValue.parse(try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("catalog/locale.json"), encoding: .utf8))
        XCTAssertEqual(Set((locale["rtlMirroredIcons"]?.array ?? []).compactMap(\.string)), RTLGlyphs.mirrored)
    }

    // MARK: - round 3 (VAPP-102): Segmented bar, context Menu, tree guides

    /// `Segmented variant: bar` (the old TabBar): full width,
    /// `$control.tabBar` tall, each item a column; a press selects.
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
    }

    /// VAPP-164: a `bar` caption is measured at the weight its SELECTED
    /// state paints (`Segmented/label` selected: medium in neutral, bold in
    /// playful), so selecting an item never needs more room than measured.
    func testSegmentedBarMeasuresCaptionsAtTheSelectedWeight() throws {
        let items: JSONValue = .array([
            .object(["label": .string("Overview"), "value": .string("a")]),
            .object(["label": .string("Settings"), "value": .string("b")]),
        ])
        let props: Props = ["items": items, "variant": .string("bar")]
        for (id, weight) in [("neutral", 500), ("playful", 700)] {
            let theme = try XCTUnwrap(ThemeHandle.builtin(id))
            let selected = theme.part("Segmented", "label", props: props, states: ["selected"], mode: .light)
            XCTAssertEqual(SurfaceMeasurer.barLabelWeight(base: 400, label: theme.part("Segmented", "label", props: props, mode: .light), selected: selected), weight, id)
            let measurer = SurfaceMeasurer(theme: theme, mode: .light, extensions: ExtensionRegistry(), kinds: [:], generation: 0)
            let size = measurer.measureLeaf(LeafRequest(component: "Segmented", props: props, textStyle: TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: theme.sansFamily)), wrap: nil)
            var caption = SurfaceMeasurer.barCaption(theme.part("Text", "root", props: ["variant": .string("caption")], mode: .light), base: TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: theme.sansFamily))
            caption.fontWeight = weight
            let xs = theme.spacing("xs")
            let expected = ["Overview", "Settings"].reduce(CGFloat(0)) { $0 + TextShaper.maxContent($1, caption) + 2 * xs }
            XCTAssertEqual(size.width, expected, accuracy: 0.01, id)
            caption.fontWeight = 400
            let regular = ["Overview", "Settings"].reduce(CGFloat(0)) { $0 + TextShaper.maxContent($1, caption) + 2 * xs }
            // (The test fonts may shape 500 as wide as 400; bold is wider.)
            XCTAssertGreaterThanOrEqual(size.width, regular, "\(id): never narrower than the regular-weight captions")
            if weight >= 700 { XCTAssertGreaterThan(size.width, regular, id) }
        }
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
