import XCTest
import SwiftUI
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-91: the SwiftUI painter's Exponential UI conformance runner. Every
/// suite of `packages/exponential-ui/conformance/manifest.json`: the painted
/// ones (catalog, extension, control-geometry, layout, overlay, replay)
/// through `SurfaceModel` + `ExponentialSurface` (hosted, so every
/// `NodeView` body runs) and the painter's TextKit measurer; the pure ones
/// through this package's bindings (`ExponentialUICore`). Writes the report
/// (`conformance/report.schema.json`) to `$EXPONENTIAL_UI_CONFORMANCE_REPORT`,
/// default `<repo>/.conformance/exponential-ui-swift.json`; check it with
/// `bun run --filter @exponential-at/ui conformance:check <report>`.
@MainActor
final class ConformanceTests: XCTestCase {
    typealias Case = (name: String, error: String?)

    /// One case; a thrown error is a failure, never the end of the run.
    func run(_ name: String, _ body: () throws -> String?) -> Case {
        do { return (name, try body()) } catch { return (name, "threw \(error)") }
    }

    /// nil when the JSON texts are equal (the core's comparison).
    func same(_ got: String, _ want: JSONValue) -> String? {
        let w = want.json
        return jsonEqual(a: got, b: w) ? nil : String(jsonDiff(a: got, b: w).prefix(300))
    }

    var repo: URL { Fixtures.dir.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent() }

    var catalog: JSONValue {
        get throws { JSONValue.parse(try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("catalog/core.catalog.json"), encoding: .utf8)) }
    }

    func model(_ id: String, theme: String? = defaultThemeId(), mode: Mode = .light, catalogId: String = coreCatalogId(), overlays: OverlayPresentation = .native) throws -> SurfaceModel {
        var o = SurfaceOptions(catalogId: catalogId)
        o.theme = theme.flatMap { ThemeHandle.builtin($0) }
        o.mode = mode
        o.overlays = overlays
        return try SurfaceModel(id: id, options: o, host: NoHost())
    }

    /// Paint the model in a hosting view at `width`: every `NodeView` body
    /// evaluated; returns the node indices painted.
    func paint(_ m: SurfaceModel, width: CGFloat, height: CGFloat = 800) -> Set<Int> {
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

    /// No Unknown; every visible main-tree node (no hidden ancestor, not a
    /// windowed-out list row) painted.
    func checkPainted(_ m: SurfaceModel, _ painted: Set<Int>) -> String? {
        if let u = m.nodes.first(where: { $0.component == "Unknown" }) { return "Unknown placeholder at \(u.id)" }
        if m.nodes.isEmpty { return "no nodes" }
        if m.frames.count != m.nodes.count { return "frames \(m.frames.count) ≠ nodes \(m.nodes.count)" }
        var visible = [Bool](repeating: false, count: m.nodes.count)
        for n in m.nodes {
            let parentVisible = n.parent.map { visible[$0] } ?? true
            visible[n.index] = parentVisible && !n.hidden && n.layer == 0
        }
        let missing = m.nodes.filter { visible[$0.index] && !painted.contains($0.index) }
        return missing.isEmpty ? nil : "not painted: \(missing.prefix(5).map(\.id))"
    }

    // MARK: - suites

    func catalogSuite() throws -> [Case] {
        try (Fixtures.json("catalog-components.json")["cases"]?.array ?? []).map { c in
            run(c["name"]?.string ?? "?") {
                let reduced = JSONValue.parse(try reduceNestedJson(nestedJson: c["node"]!.json, catalogId: coreCatalogId(), extensionsJson: nil))
                if let issues = reduced["issues"]?.array, !issues.isEmpty { return "issues \(issues)" }
                let m = try model("catalog")
                let out = try m.setNested(json: c["node"]!.json)
                if out.issuesJson != "[]" { return "surface issues \(out.issuesJson)" }
                return checkPainted(m, paint(m, width: 400))
            }
        }
    }

    func macrosSuite() throws -> [Case] {
        try (Fixtures.json("catalog-macros.json")["cases"]?.array ?? []).map { c in
            run(c["name"]?.string ?? "?") {
                same(try reduceNestedJson(nestedJson: c["input"]!.json, catalogId: coreCatalogId(), extensionsJson: nil), .object(["root": c["expected"]!, "issues": .array([])]))
            }
        }
    }

    func basicMapSuite() throws -> [Case] {
        try (Fixtures.json("catalog-basic-map.json")["cases"]?.array ?? []).map { c in
            run(c["name"]?.string ?? "?") {
                same(try reduceSurfaceJson(componentsJson: c["components"]!.json, catalogId: basicCatalogId(), extensionsJson: nil), c["expected"]!)
            }
        }
    }

    /// The extension's natives reach a painter: measured by it, painted by it.
    final class ProbePainter: ExtensionPainter {
        var measured = 0
        var painted = 0
        func measure(_ leaf: ExtensionLeaf, wrap: CGFloat?) -> CGSize? {
            measured += 1
            return CGSize(width: 120, height: 40)
        }
        func paint(_ context: ExtensionContext) -> AnyView {
            painted += 1
            return AnyView(Color.red)
        }
    }

    func extensionSuite() throws -> [Case] {
        let f = try Fixtures.json("catalog-extension.json")
        let ext = f["extension"]!
        let kinds = (ext["components"]?.object ?? [:]).filter { $0.value["kind"]?.string == "native" }.map(\.key)
        return (f["cases"]?.array ?? []).map { c in
            run(c["name"]?.string ?? "?") {
                if let d = same(try reduceSurfaceJson(componentsJson: c["components"]!.json, catalogId: c["catalogId"]!.string!, extensionsJson: JSONValue.array([ext]).json), c["expected"]!) { return d }
                guard c["expected"]!.json.contains(#""component":"Extension""#) else { return nil }
                let probe = ProbePainter()
                let m = try model("ext", catalogId: c["catalogId"]!.string!)
                try m.register(extension: ext.json, painters: Dictionary(uniqueKeysWithValues: kinds.map { ($0, probe as ExtensionPainter) }))
                _ = try m.setComponents(json: c["components"]!.json)
                let painted = paint(m, width: 400)
                if let d = checkPainted(m, painted) { return d }
                let leaves = m.nodes.filter { $0.component == "Extension" }
                if leaves.isEmpty { return "no Extension leaf" }
                if probe.measured == 0 { return "the painter measured nothing" }
                if probe.painted == 0 { return "the painter painted nothing" }
                if let l = leaves.first(where: { m.frame($0.index).height != 40 }) { return "\(l.id) not at the painter's size" }
                return nil
            }
        }
    }

    func themeResolvedSuite() throws -> [Case] {
        try (Fixtures.json("theme-resolved.json")["themes"]?.object ?? [:]).sorted { $0.key < $1.key }.map { id, expected in
            run(id) {
                guard let json = builtinThemeJson(id: id) else { return "not a built-in" }
                // The painter's handle loads the same theme.
                guard ThemeHandle.builtin(id)?.id == id else { return "ThemeHandle.builtin(\(id)) failed" }
                return same(json, expected)
            }
        }
    }

    func themeRecipesSuite() throws -> [Case] {
        var themes: [String: String] = [:]
        for id in builtinThemeIds() { themes[id] = builtinThemeJson(id: id) }
        return try (Fixtures.json("theme-recipes.json")["cases"]?.array ?? []).map { c in
            let component = c["component"]!.string!, part = c["part"]!.string!
            return run("\(component)/\(part) \(c["props"]!.json)") {
                for (themeId, byMode) in c["visuals"]?.object ?? [:] {
                    guard let theme = themes[themeId] else { return "\(themeId): not a built-in" }
                    for (mode, byState) in byMode.object ?? [:] {
                        for (state, style) in byState.object ?? [:] {
                            let got = try resolveRecipeJson(themeJson: theme, component: component, part: part, propsJson: c["props"]!.json, states: state == "default" ? [] : [state], mode: mode)
                            if let d = same(got, style) { return "\(themeId)/\(mode)/\(state): \(d)" }
                        }
                    }
                }
                return nil
            }
        }
    }

    func themeExtendsSuite() throws -> [Case] {
        try (Fixtures.json("theme-extends.json")["cases"]?.array ?? []).map { c in
            run(c["name"]?.string ?? "?") {
                let resolved = try loadThemeJson(themeJson: c["theme"]!.json, parentsJson: nil)
                if let d = same(JSONValue.parse(resolved)["chain"]?.json ?? "null", c["expected"]!["chain"]!) { return "chain: \(d)" }
                // The painter's handle loads it too.
                _ = try ThemeHandle.load(json: c["theme"]!.json)
                for p in c["expected"]!["probes"]?.array ?? [] {
                    let got = try resolveRecipeJson(themeJson: resolved, component: p["component"]!.string!, part: p["part"]!.string!, propsJson: (p["props"] ?? .object([:])).json, states: (p["states"]?.array ?? []).compactMap(\.string), mode: p["mode"]!.string!)
                    if let d = same(got, p["style"]!) { return "\(p["component"]!.string!)/\(p["part"]!.string!): \(d)" }
                }
                return nil
            }
        }
    }

    func themeInvalidSuite() throws -> [Case] {
        try (Fixtures.json("theme-invalid.json")["cases"]?.array ?? []).map { c in
            run(c["name"]?.string ?? "?") {
                if (try? ThemeHandle.load(json: c["theme"]!.json)) != nil { return "loaded" }
                return same(themeIssuesJson(themeJson: c["theme"]!.json, parentsJson: nil), c["issues"]!)
            }
        }
    }

    /// The border box the PAINTER gives a control's sizing part: its node's
    /// frame (the measurer's answer) + the box it paints with, or the box
    /// it draws a sub-part with (Slider thumb, ToggleGroup item).
    func measuredBox(_ m: SurfaceModel, component: String, part: String) -> [String: Double]? {
        func box(_ i: Int) -> [String: Double] {
            let f = m.frame(i), s = m.boxStyle(i)
            return ["width": f.width, "height": f.height, "paddingHorizontal": s.paddingHorizontal, "paddingVertical": s.paddingVertical, "gap": s.gap, "borderWidth": s.borderWidth, "borderRadius": min(s.radius, 9999)].mapValues { Double($0) }
        }
        func drawn(_ d: DrawnBox) -> [String: Double] {
            var out: [String: Double] = ["height": d.height, "paddingHorizontal": d.paddingHorizontal, "paddingVertical": d.paddingVertical, "gap": d.gap, "borderWidth": d.borderWidth, "borderRadius": min(d.borderRadius, 9999)].mapValues { Double($0) }
            if let w = d.width { out["width"] = Double(w) }
            return out
        }
        switch (component, part) {
        case ("Slider", "thumb"):
            guard let track = m.nodes.first(where: { $0.component == "Slider" && $0.part == "track" }) else { return nil }
            return drawn(m.part("Slider", "thumb", props: m.ownerProps(track.index)).sliderThumbBox)
        case ("ToggleGroup", "item"):
            guard let g = m.nodes.first(where: { $0.component == "ToggleGroup" }) else { return nil }
            var d = m.part("ToggleGroup", "item", props: g.props).toggleItemBox
            // The leaf's frame holds the items at their drawn height.
            d.height = min(d.height, m.frame(g.index).height - 2 * m.boxStyle(g.index).paddingVertical)
            return drawn(d)
        case (_, "root"):
            return m.nodes.first.map { box($0.index) }
        case ("Select", "trigger"), ("DatePicker", "trigger"):
            return m.nodes.first(where: { $0.component == component && $0.part == "field" }).map { box($0.index) }
        case ("Radio", "item"):
            return m.nodes.first(where: { $0.component == "Radio" && $0.part == "dot" }).map { box($0.index) }
        default:
            return m.nodes.first(where: { $0.recipeComponent == component && $0.part == part }).map { box($0.index) }
        }
    }

    /// `check_geometry`: every key the fixture fixes within 0.5, except
    /// `height` when the fixture has `minHeight` (then only the floor).
    func checkGeometry(_ want: [String: JSONValue], _ got: [String: Double]) -> String? {
        var issues: [String] = []
        for key in ["width", "height", "paddingHorizontal", "paddingVertical", "borderWidth", "borderRadius"] {
            guard let w = want[key]?.number else { continue }
            if key == "height", want["minHeight"] != nil { continue }
            guard let a = got[key] else { issues.append("\(key) \(w)≠nil"); continue }
            if abs(a - w) > 0.5 { issues.append("\(key) \(w)≠\(a)") }
        }
        if let floor = want["minHeight"]?.number, (got["height"] ?? 0) + 0.5 < floor { issues.append("minHeight \(floor)>\(got["height"] ?? 0)") }
        return issues.isEmpty ? nil : issues.joined(separator: ", ")
    }

    func controlGeometrySuite() throws -> [Case] {
        let f = try Fixtures.json("control-geometry.json")
        let components = try catalog["components"]?.object ?? [:]
        var out: [Case] = []
        for (themeId, byComponent) in (f["themes"]?.object ?? [:]).sorted(by: { $0.key < $1.key }) {
            for (component, entry) in (byComponent.object ?? [:]).sorted(by: { $0.key < $1.key }) {
                let part = entry["part"]!.string!
                for (name, c) in (entry["cases"]?.object ?? [:]).sorted(by: { $0.key < $1.key }) {
                    out.append(run("\(themeId) \(component) \(name)") {
                        var props = components[component]?["example"]?.object ?? [:]
                        for (k, v) in c["props"]?.object ?? [:] { props[k] = v }
                        let m = try model("cg", theme: themeId)
                        try m.setNested(json: JSONValue.object(["id": .string("root"), "component": .string(component), "props": .object(props)]).json)
                        m.setViewport(width: 360, height: 800)
                        guard let box = measuredBox(m, component: component, part: part) else { return "part \(part) not painted" }
                        return checkGeometry(c["geometry"]?.object ?? [:], box)
                    })
                }
            }
        }
        return out
    }

    func layoutSuite() throws -> [Case] {
        let fx = try Fixtures.json("layout-geometry.json")
        return (fx["cases"]?.object ?? [:]).sorted { $0.key < $1.key }.map { name, c in
            run(name) {
                var tree = fx["surface"]!.object!
                var style = tree["style"]?.object ?? [:]
                style["direction"] = c["direction"]
                tree["style"] = .object(style)
                let m = try model("geometry", theme: nil)
                m.fixedMeasure = true
                let outcome = try m.setNested(json: JSONValue.object(tree).json)
                if outcome.issuesJson != "[]" { return "issues \(outcome.issuesJson)" }
                let width = c["width"]!.number!
                _ = m.surface.setViewport(width: Float(width), height: 0, maxHeight: nil)
                let out = try m.surface.layoutFixed(sizesJson: fx["measures"]!.json, wrap: false)
                m.setViewport(width: CGFloat(width), height: 0)
                let expected = c["frames"]?.array ?? []
                if out.frames.count != expected.count { return "\(out.frames.count) frames, expected \(expected.count)" }
                for (i, want) in expected.enumerated() {
                    let f = out.frames[i]
                    let rect = CGRect(f)
                    let id = m.nodes[Int(f.index)].id
                    if id != want["id"]?.string { return "order at \(i): \(id)" }
                    for (k, v) in [("x", rect.minX), ("y", rect.minY), ("w", rect.width), ("h", rect.height)] {
                        if abs(Double(v) - want[k]!.number!) > 0.001 { return "\(id).\(k) = \(v), expected \(want[k]!.number!)" }
                    }
                }
                return nil
            }
        }
    }

    func overlaySuite() throws -> [Case] {
        let fx = try Fixtures.json("overlay-geometry.json")
        let tolerance = fx["tolerancePx"]?.number ?? 0
        return (fx["cases"]?.array ?? []).map { c in
            run(c["name"]?.string ?? "?") {
                let a = c["anchor"]!, s = c["size"]!, v = c["viewport"]!, e = c["expected"]!
                let vw = v["width"]!.number!, vh = v["height"]!.number!
                // The painter's popover: anchored at the core's placement.
                let tree: JSONValue = .object([
                    "id": .string("root"), "component": .string("Box"),
                    "style": .object(["position": .string("relative"), "width": .number(vw), "height": .number(vh), "overflow": .string("hidden")]),
                    "children": .array([.object([
                        "id": .string("pop"), "component": .string("Popover"),
                        "props": .object(["open": .bool(true), "side": c["side"]!]),
                        "style": .object(["position": .string("absolute"), "left": a["x"]!, "top": a["y"]!, "width": a["width"]!, "height": a["height"]!]),
                        "slots": .object(["trigger": .object(["id": .string("anchor"), "component": .string("Box"), "style": .object(["width": a["width"]!, "height": a["height"]!])])]),
                        "children": .array([.object(["id": .string("content"), "component": .string("Box"), "style": .object(["width": s["width"]!, "height": s["height"]!])])]),
                    ])]),
                ])
                let m = try model("ov", theme: "neutral", overlays: .painted)
                try m.setNested(json: tree.json)
                m.setViewport(width: CGFloat(vw), height: CGFloat(vh))
                _ = paint(m, width: CGFloat(vw), height: CGFloat(vh))
                guard let layer = m.layers.first(where: { $0.kind == "Popover" }) else { return "no popover layer" }
                let got = layer.frame
                let want = try placeOverlay(anchorX: a["x"]!.number!, anchorY: a["y"]!.number!, anchorW: a["width"]!.number!, anchorH: a["height"]!.number!, sizeW: Double(got.width), sizeH: Double(got.height), viewportW: vw, viewportH: vh, side: c["side"]!.string!)
                if layer.placementSide != e["side"]?.string { return "side \(layer.placementSide ?? "nil")" }
                if want.side != e["side"]?.string { return "placeOverlay side \(want.side)" }
                if layer.flipped != (e["flipped"]?.bool ?? false) { return "flipped \(layer.flipped)" }
                if abs(Double(got.minX) - want.x) > tolerance || abs(Double(got.minY) - want.y) > tolerance { return "frame \(got) vs \(want.x),\(want.y)" }
                return nil
            }
        }
    }

    func replaySuite() throws -> [Case] {
        let sink = try Fixtures.json("kitchen-sink.json")
        var expected = try Fixtures.json("kitchen-sink.expanded.json").object ?? [:]
        expected["$comment"] = nil
        // The expanded tree's pre-order.
        var order: [String] = []
        func walk(_ n: JSONValue) {
            if let id = n["id"]?.string { order.append(id) }
            for (_, slot) in (n["slots"]?.object ?? [:]).sorted(by: { $0.key < $1.key }) { walk(slot) }
            for child in n["children"]?.array ?? [] { walk(child) }
        }
        walk(expected["root"] ?? .null)
        var out: [Case] = []
        for themeId in builtinThemeIds() {
            for mode in Mode.allCases {
                out.append(run("\(themeId)/\(mode.rawValue)") {
                    if let d = same(try reduceNestedJson(nestedJson: sink.json, catalogId: coreCatalogId(), extensionsJson: nil), .object(expected)) { return "reduced tree differs: \(d)" }
                    let m = try model("ks", theme: themeId, mode: mode)
                    let outcome = try m.setNested(json: sink.json)
                    if outcome.issuesJson != "[]" { return "issues \(outcome.issuesJson)" }
                    try m.apply(json: #"{"version":"v0.9","updateDataModel":{"surfaceId":"ks","value":{"posts":[{"title":"One"},{"title":"Two"}],"ui":{"confirmOpen":false}}}}"#)
                    for width in [390.0, 900.0] {
                        let painted = paint(m, width: width)
                        if let d = checkPainted(m, painted) { return "\(width): \(d)" }
                        // Accessibility order = pre-order: the painter sorts
                        // VoiceOver by node index, which must follow the
                        // expanded tree's pre-order for every painted node.
                        let idx = order.compactMap { m.index(of: $0) }.filter { painted.contains($0) }
                        if idx.count < 50 { return "\(width): only \(idx.count) nodes painted" }
                        for (i, v) in idx.enumerated() where i > 0 && v <= idx[i - 1] { return "\(width): a11y order is not the pre-order at \(m.nodes[v].id)" }
                        for n in m.nodes { if let p = n.parent, p >= n.index { return "\(n.id) before its parent" } }
                    }
                    return nil
                })
            }
        }
        return out
    }

    func feed(_ decoder: (String) -> String, _ end: () -> String, _ chunks: [JSONValue]) -> JSONValue {
        var messages: [JSONValue] = [], issues: [JSONValue] = []
        for out in chunks.map({ decoder($0.string ?? "") }) + [end()] {
            let v = JSONValue.parse(out)
            messages += v["messages"]?.array ?? []
            issues += v["issues"]?.array ?? []
        }
        return .object(["messages": .array(messages), "issues": .array(issues)])
    }

    func transportSuite() throws -> [Case] {
        let f = try Fixtures.json("host-transport.json")
        var out: [Case] = []
        for c in f["jsonl"]?.array ?? [] {
            out.append(run("jsonl: \(c["name"]?.string ?? "")") {
                let d = JsonlDecoder()
                return same(feed({ d.push(chunk: $0) }, { d.end() }, c["chunks"]?.array ?? []).json, c["expected"]!)
            })
        }
        for c in f["sse"]?.array ?? [] {
            out.append(run("sse: \(c["name"]?.string ?? "")") {
                let d = SseDecoder()
                return same(feed({ d.push(chunk: $0) }, { d.end() }, c["chunks"]?.array ?? []).json, c["expected"]!)
            })
        }
        for c in f["mcp"]?.array ?? [] {
            out.append(run("mcp: \(c["name"]?.string ?? "")") { same(try messagesFromMcpResultJson(resultJson: c["result"]!.json), c["expected"]!) })
        }
        for c in f["mcpAction"]?.array ?? [] {
            out.append(run("mcpAction: \(c["name"]?.string ?? "")") { same(try mcpActionCallJson(messageJson: c["message"]!.json, tool: c["tool"]?.string), c["expected"]!) })
        }
        return out
    }

    func policySuite() throws -> [Case] {
        let f = try Fixtures.json("host-policy.json")
        var out: [Case] = []
        for c in f["functions"]?.array ?? [] {
            out.append(run("function: \(c["name"]?.string ?? "")") {
                same(JSONValue.string(try decideFunction(policyJson: c["policy"]?.json, name: c["fn"]!.string!, registered: c["registered"]!.bool!)).json, c["expected"]!)
            })
        }
        for c in f["combine"]?.array ?? [] {
            out.append(run("combine: \(c["a"]?.string ?? "") + \(c["b"]?.string ?? "")") {
                same(JSONValue.string(try combineDecisions(a: c["a"]!.string!, b: c["b"]!.string!)).json, c["expected"]!)
            })
        }
        for c in f["urls"]?.array ?? [] {
            out.append(run("url: \(c["name"]?.string ?? "")") { same(try decideUrlJson(policyJson: c["policy"]?.json, url: c["url"]!.string!), c["expected"]!) })
        }
        for c in f["media"]?.array ?? [] {
            out.append(run("media: \(c["name"]?.string ?? "")") {
                let got = try mediaRequestJson(url: c["url"]!.string!, optionsJson: c["options"]!.json) ?? "null"
                if let d = same(got, c["expected"]!) { return d }
                // The host's URLRequest carries the same url + headers.
                let host = ExponentialHost(HostOptions(policy: HostPolicy(media: try JSONDecoder().decode(MediaOptions.self, from: Data(c["options"]!.json.utf8)))))
                let r = host.mediaRequest(c["url"]!.string!)
                guard let want = c["expected"], !want.isNull else { return r == nil ? nil : "host built a request" }
                if r?.url?.absoluteString != want["url"]?.string { return "host url \(r?.url?.absoluteString ?? "nil")" }
                for (k, v) in want["headers"]?.object ?? [:] where r?.value(forHTTPHeaderField: k) != v.string { return "host header \(k)" }
                return nil
            })
        }
        for c in f["sources"]?.array ?? [] {
            out.append(run("source: \(c["uri"]?.string ?? "")") { same(parseSourceJson(uri: c["uri"]!.string!) ?? "null", c["expected"]!) })
        }
        for c in f["negotiation"]?.array ?? [] {
            let ids = (c["extensionIds"]?.array ?? []).compactMap(\.string)
            out.append(run("negotiation: \(ids.count) extension ids") {
                if let d = same(JSONValue.array(supportedCatalogIdsFor(extensionIds: ids).map { .string($0) }).json, c["expected"]!["supportedCatalogIds"]!) { return d }
                return same(clientCapabilitiesJson(extensionIds: ids), c["expected"]!["clientCapabilities"]!)
            })
        }
        return out
    }

    func routerSuite() throws -> [Case] {
        let f = try Fixtures.json("host-router.json")
        let packages = f["packages"]!
        var out: [Case] = []
        for v in f["validation"]?.array ?? [] {
            let id = v["package"]!.string!
            out.append(run("validation: \(id)") { same(try validatePackageJson(packageJson: packages[id]!.json, catalogIds: nil), v["expected"]!) })
        }
        for flow in f["flows"]?.array ?? [] {
            out.append(run("flow: \(flow["name"]?.string ?? "")") {
                let router = HostRouter(extensionIds: (flow["extensionIds"]?.array ?? []).compactMap(\.string))
                for id in (flow["packages"]?.array ?? []).compactMap(\.string) {
                    if let d = same(try router.installPackage(packageJson: packages[id]!.json), flow["installIssues"]![id]!) { return "install \(id): \(d)" }
                }
                for (i, step) in (flow["steps"]?.array ?? []).enumerated() {
                    if let d = same(router.route(messageJson: step["message"]!.json), step["expected"]!) { return "step \(i): \(d)" }
                }
                return nil
            })
        }
        return out
    }

    func suite(_ id: String) throws -> [Case] {
        switch id {
        case "catalog": try catalogSuite()
        case "macros": try macrosSuite()
        case "basic-map": try basicMapSuite()
        case "extension": try extensionSuite()
        case "theme-resolved": try themeResolvedSuite()
        case "theme-recipes": try themeRecipesSuite()
        case "theme-extends": try themeExtendsSuite()
        case "theme-invalid": try themeInvalidSuite()
        case "control-geometry": try controlGeometrySuite()
        case "layout": try layoutSuite()
        case "overlay": try overlaySuite()
        case "replay": try replaySuite()
        case "host-transport": try transportSuite()
        case "host-policy": try policySuite()
        case "host-router": try routerSuite()
        default: [("suite \(id)", "the runner does not know the suite \(id): add it")]
        }
    }

    func testTheSwiftUIPainterPassesEveryConformanceSuiteAndWritesItsReport() throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
        let manifest = JSONValue.parse(try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("conformance/manifest.json"), encoding: .utf8))
        var suites: [String: JSONValue] = [:]
        var problems: [String] = []
        for s in manifest["suites"]?.array ?? [] {
            let id = s["id"]!.string!
            let cases = try suite(id)
            let failed = cases.compactMap { c in c.error.map { "\(c.name): \($0)" } }
            let want = Int(s["cases"]!.number!)
            if cases.count != want { problems.append("\(id): ran \(cases.count) of \(want) cases") }
            if !failed.isEmpty { problems.append("\(id): \(failed.count) failed\n  \(failed.prefix(20).joined(separator: "\n  "))") }
            suites[id] = .object(["cases": .number(Double(cases.count)), "passed": .number(Double(cases.count - failed.count)), "failed": .array(failed.map { .string($0) })])
        }
        let report: JSONValue = .object([
            "renderer": .string("ExponentialUI"),
            "platform": .string("ios"),
            "version": .string(ExponentialUI.coreVersion),
            "conformanceVersion": manifest["version"] ?? .number(1),
            "suites": .object(suites),
        ])
        let path = ProcessInfo.processInfo.environment["EXPONENTIAL_UI_CONFORMANCE_REPORT"].map { URL(fileURLWithPath: $0) } ?? repo.appendingPathComponent(".conformance/exponential-ui-swift.json")
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
        let data = try JSONSerialization.data(withJSONObject: report.any, options: [.prettyPrinted, .sortedKeys])
        try (String(decoding: data, as: UTF8.self) + "\n").write(to: path, atomically: true, encoding: .utf8)
        print("conformance report: \(path.path)")
        XCTAssertEqual(problems, [], "not conformant:\n\(problems.joined(separator: "\n"))")
    }
}
