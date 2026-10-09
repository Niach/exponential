// VAPP-86: the Swift binding suite. Replays the shared fixtures through the
// UniFFI facade (what the SwiftUI painter links) and drives a surface with a
// Swift-side `Measurer`, counting the upcalls per pass.
//   bash apps/desktop/crates/exponential-ui-ffi/run-binding-tests.sh swift
import Foundation
// The generated binding is compiled into this module (see run-binding-tests.sh).

let fixtures = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "../../../../packages/exponential-ui/fixtures"
var failures = 0
var checks = 0

func check(_ ok: Bool, _ message: @autoclosure () -> String) {
    checks += 1
    if !ok {
        failures += 1
        print("FAIL: \(message())")
    }
}

func load(_ name: String) -> Any {
    let data = FileManager.default.contents(atPath: "\(fixtures)/\(name)")!
    return try! JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])
}

func text(_ value: Any) -> String {
    if let s = value as? String, !JSONSerialization.isValidJSONObject([s]) { return "\"\(s)\"" }
    let data = try! JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
    return String(data: data, encoding: .utf8)!
}

func equal(_ got: String, _ expected: Any, _ label: String) {
    let e = text(expected)
    check(jsonEqual(a: got, b: e), "\(label): \(jsonDiff(a: got, b: e))")
}

// --- catalog fixtures -------------------------------------------------------
let macros = load("catalog-macros.json") as! [String: Any]
for c in macros["cases"] as! [[String: Any]] {
    let got = try! reduceNestedJson(nestedJson: text(c["input"]!), catalogId: coreCatalogId(), extensionsJson: nil)
    equal(got, ["root": c["expected"]!, "issues": []], "macros \(c["name"]!)")
}
let basic = load("catalog-basic-map.json") as! [String: Any]
for c in basic["cases"] as! [[String: Any]] {
    let got = try! reduceSurfaceJson(componentsJson: text(c["components"]!), catalogId: basicCatalogId(), extensionsJson: nil)
    equal(got, c["expected"]!, "basic \(c["name"]!)")
}
let ext = load("catalog-extension.json") as! [String: Any]
check(try! extensionErrors(extensionJson: text(ext["extension"]!)).isEmpty, "extension validates")
for c in ext["cases"] as! [[String: Any]] {
    let got = try! reduceSurfaceJson(componentsJson: text(c["components"]!), catalogId: c["catalogId"] as! String, extensionsJson: text([ext["extension"]!]))
    equal(got, c["expected"]!, "extension \(c["name"]!)")
}
let components = load("catalog-components.json") as! [String: Any]
for c in components["cases"] as! [[String: Any]] {
    let got = try! reduceNestedJson(nestedJson: text(c["node"]!), catalogId: coreCatalogId(), extensionsJson: nil)
    let parsed = try! JSONSerialization.jsonObject(with: got.data(using: .utf8)!) as! [String: Any]
    check((parsed["issues"] as! [Any]).isEmpty, "components \(c["name"]!): issues \(parsed["issues"]!)")
}
print("catalog fixtures: \(checks) checks")

// --- theme fixtures ---------------------------------------------------------
let resolved = load("theme-resolved.json") as! [String: Any]
var themes: [String: String] = [:]
for (id, expected) in resolved["themes"] as! [String: Any] {
    let got = builtinThemeJson(id: id)!
    themes[id] = got
    equal(got, expected, "theme-resolved \(id)")
}
let recipes = load("theme-recipes.json") as! [String: Any]
for c in recipes["cases"] as! [[String: Any]] {
    for (themeId, byMode) in c["visuals"] as! [String: Any] {
        for (mode, byState) in byMode as! [String: Any] {
            for (state, style) in byState as! [String: Any] {
                let states = state == "default" ? [] : [state]
                let got = try! resolveRecipeJson(themeJson: themes[themeId]!, component: c["component"] as! String, part: c["part"] as! String, propsJson: text(c["props"]!), states: states, mode: mode)
                equal(got, style, "theme-recipes \(themeId)/\(mode)/\(state) \(c["component"]!)/\(c["part"]!)")
            }
        }
    }
}
let extends = load("theme-extends.json") as! [String: Any]
for c in extends["cases"] as! [[String: Any]] {
    let theme = try! loadThemeJson(themeJson: text(c["theme"]!), parentsJson: nil)
    let parsed = try! JSONSerialization.jsonObject(with: theme.data(using: .utf8)!) as! [String: Any]
    let expected = c["expected"] as! [String: Any]
    check((parsed["chain"] as! [String]) == (expected["chain"] as! [String]), "theme-extends \(c["name"]!): chain")
    for p in expected["probes"] as! [[String: Any]] {
        let got = try! resolveRecipeJson(themeJson: theme, component: p["component"] as! String, part: p["part"] as! String, propsJson: text(p["props"]!), states: (p["states"] as? [String]) ?? [], mode: p["mode"] as! String)
        equal(got, p["style"]!, "theme-extends \(c["name"]!) \(p["component"]!)/\(p["part"]!)")
    }
}
// --- raw JSON (order-preserving) --------------------------------------------
// The validator reports issues in the theme's KEY order and
// `JSONSerialization` dictionaries do not keep it: the theme-invalid cases
// go in as the fixture's own text.

/// Advance `i` past the JSON value at `i` (after whitespace).
func skipJSON(_ s: [UInt8], _ i: inout Int) {
    func ws() { while i < s.count, [0x20, 0x0A, 0x0D, 0x09].contains(s[i]) { i += 1 } }
    ws()
    guard i < s.count else { return }
    switch s[i] {
    case 0x22:
        i += 1
        while i < s.count, s[i] != 0x22 { i += s[i] == 0x5C ? 2 : 1 }
        i += 1
    case 0x7B, 0x5B:
        i += 1
        ws()
        while i < s.count, s[i] != 0x7D, s[i] != 0x5D {
            skipJSON(s, &i)
            ws()
            if s[i] == 0x3A { i += 1; skipJSON(s, &i); ws() }
            if s[i] == 0x2C { i += 1; ws() }
        }
        i += 1
    default:
        while i < s.count, ![0x2C, 0x7D, 0x5D, 0x20, 0x0A, 0x0D, 0x09].contains(s[i]) { i += 1 }
    }
}

/// The raw members of the object `json` is, or the raw elements of the
/// array: `(key or nil, raw value)` in file order.
func rawChildren(_ json: String) -> [(String?, String)] {
    let s = Array(json.utf8)
    var i = 0
    while i < s.count, s[i] != 0x7B, s[i] != 0x5B { i += 1 }
    guard i < s.count else { return [] }
    let object = s[i] == 0x7B
    i += 1
    var out: [(String?, String)] = []
    while true {
        while i < s.count, [0x20, 0x0A, 0x0D, 0x09, 0x2C].contains(s[i]) { i += 1 }
        if i >= s.count || s[i] == 0x7D || s[i] == 0x5D { break }
        var key: String?
        if object {
            let ks = i
            skipJSON(s, &i)
            key = try? JSONSerialization.jsonObject(with: Data(s[ks..<i]), options: [.fragmentsAllowed]) as? String
            while i < s.count, s[i] != 0x3A { i += 1 }
            i += 1
            while i < s.count, [0x20, 0x0A, 0x0D, 0x09].contains(s[i]) { i += 1 }
        }
        let vs = i
        skipJSON(s, &i)
        out.append((key, String(decoding: s[vs..<i], as: UTF8.self)))
    }
    return out
}

/// `cases[i].theme` of a fixture file, raw, in file order.
func rawCaseThemes(_ name: String) -> [String] {
    let file = String(decoding: FileManager.default.contents(atPath: "\(fixtures)/\(name)")!, as: UTF8.self)
    guard let cases = rawChildren(file).first(where: { $0.0 == "cases" })?.1 else { return [] }
    return rawChildren(cases).map { c in rawChildren(c.1).first { $0.0 == "theme" }?.1 ?? "null" }
}

let invalid = load("theme-invalid.json") as! [String: Any]
let invalidThemes = rawCaseThemes("theme-invalid.json")
check(invalidThemes.count == (invalid["cases"] as! [Any]).count, "theme-invalid: \(invalidThemes.count) raw themes")
for (n, c) in (invalid["cases"] as! [[String: Any]]).enumerated() {
    let issues = themeIssuesJson(themeJson: n < invalidThemes.count ? invalidThemes[n] : text(c["theme"]!), parentsJson: nil)
    equal(issues, c["issues"]!, "theme-invalid \(c["name"]!)")
}
let geometry = load("control-geometry.json") as! [String: Any]
for (themeId, byComponent) in geometry["themes"] as! [String: Any] {
    for (component, entry) in byComponent as! [String: Any] {
        for (name, cs) in (entry as! [String: Any])["cases"] as! [String: Any] {
            let cs = cs as! [String: Any]
            let got = try! controlGeometryJson(themeJson: themes[themeId]!, component: component, propsJson: text(cs["props"]!))
            equal(got, cs["geometry"]!, "control-geometry \(themeId) \(component) \(name)")
        }
    }
}
print("theme fixtures: \(checks) checks so far")

// --- overlay + layout fixtures ------------------------------------------------
let overlay = load("overlay-geometry.json") as! [String: Any]
for c in overlay["cases"] as! [[String: Any]] {
    let a = c["anchor"] as! [String: Double], s = c["size"] as! [String: Double], v = c["viewport"] as! [String: Double]
    let p = try! placeOverlay(anchorX: a["x"]!, anchorY: a["y"]!, anchorW: a["width"]!, anchorH: a["height"]!, sizeW: s["width"]!, sizeH: s["height"]!, viewportW: v["width"]!, viewportH: v["height"]!, side: c["side"] as! String)
    let e = c["expected"] as! [String: Any]
    check(p.x == e["x"] as! Double && p.y == e["y"] as! Double && p.side == e["side"] as! String && p.flipped == e["flipped"] as! Bool, "overlay \(c["name"]!): \(p)")
}
let layout = load("layout-geometry.json") as! [String: Any]
for (name, cs) in layout["cases"] as! [String: Any] {
    let cs = cs as! [String: Any]
    var tree = layout["surface"] as! [String: Any]
    var style = tree["style"] as! [String: Any]
    style["direction"] = cs["direction"]
    tree["style"] = style
    let surface = try! Surface(surfaceId: "geometry", catalogId: coreCatalogId(), themeId: "", mode: "light")
    check(try! surface.setNested(nestedJson: text(tree)).issuesJson == "[]", "layout \(name): issues")
    _ = surface.setViewport(width: Float(cs["width"] as! Double), height: 0, maxHeight: nil)
    let out = try! surface.layoutFixed(sizesJson: text(layout["measures"]!), wrap: false)
    let nodes = surface.nodes()
    let expected = cs["frames"] as! [[String: Any]]
    check(out.frames.count == expected.count, "layout \(name): \(out.frames.count) frames")
    for (i, want) in expected.enumerated() where i < out.frames.count {
        let f = out.frames[i]
        check(nodes[Int(f.index)].id == want["id"] as! String, "layout \(name): order at \(i)")
        for (k, v) in [("x", f.x), ("y", f.y), ("w", f.w), ("h", f.h)] {
            check(abs(v - Float(want[k] as! Double)) <= 0.001, "layout \(name): \(want["id"]!).\(k) = \(v) vs \(want[k]!)")
        }
    }
    check(out.upcalls == 1, "layout \(name): \(out.upcalls) upcalls")
}
print("layout fixtures: \(checks) checks so far")

// --- a Swift measurer through the facade ----------------------------------------
final class CountingMeasurer: Measurer, @unchecked Sendable {
    var intrinsics = 0
    var heights = 0
    func measureId() -> UInt64 { 5 }
    func measureIntrinsics(leaves: [FfiLeaf]) -> [FfiIntrinsics] {
        intrinsics += 1
        return leaves.map { l in
            let chars = Float(l.text.count)
            let w = l.control.width ?? (8 * chars + 2 * l.control.paddingHorizontal)
            let h = l.control.height ?? (l.textStyle.lineHeight + 2 * l.control.paddingVertical)
            let minW = l.component == "Text" ? (l.text.split(separator: " ").map { Float($0.count) * 8 }.max() ?? 0) : w
            return FfiIntrinsics(minContentWidth: minW, maxContentWidth: w, heightAtMaxContent: h)
        }
    }
    func measureHeights(leaves: [FfiLeaf], requests: [FfiHeightRequest]) -> [Float] {
        heights += 1
        return requests.map { r in
            let l = leaves.first { $0.index == r.index }!
            let perLine = max(1, (r.width / 8).rounded(.down))
            return max(1, (Float(l.text.count) / perLine).rounded(.up)) * l.textStyle.lineHeight
        }
    }
}
let bench = try! Surface(surfaceId: "bench", catalogId: coreCatalogId(), themeId: nil, mode: "light")
check(try! bench.setNested(nestedJson: benchTreeJson(n: 200)).issuesJson == "[]", "bench: issues")
_ = bench.setViewport(width: 390, height: 0, maxHeight: nil)
let measurer = CountingMeasurer()
let first = bench.layout(measurer: measurer)
check(bench.nodeCount() >= 200, "bench: \(bench.nodeCount()) nodes")
check(first.upcalls <= 3 && first.upcalls == UInt32(measurer.intrinsics + measurer.heights), "bench: \(first.upcalls) upcalls")
check(first.frames.count == Int(bench.nodeCount()), "bench: frames")
let warm = bench.layout(measurer: measurer)
check(warm.upcalls == 0, "bench: warm pass upcalls \(warm.upcalls)")
// Timing: alternate two widths so every pass does real work; the intrinsics are memoized, heights at the second width cost ONE upcall once.
var best = Double.infinity
for i in 0..<30 {
    _ = bench.setViewport(width: i % 2 == 0 ? 390 : 900, height: 0, maxHeight: nil)
    let t0 = DispatchTime.now().uptimeNanoseconds
    _ = bench.layout(measurer: measurer)
    _ = bench.nodes().count
    let dt = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1_000_000
    if i >= 2 { best = min(best, dt) }
}
var bestLayout = Double.infinity
for i in 0..<30 {
    _ = bench.setViewport(width: i % 2 == 0 ? 390 : 900, height: 0, maxHeight: nil)
    let t0 = DispatchTime.now().uptimeNanoseconds
    _ = bench.layout(measurer: measurer)
    bestLayout = min(bestLayout, Double(DispatchTime.now().uptimeNanoseconds - t0) / 1_000_000)
}
print(String(format: "bench 200 nodes through Swift: %d layout nodes, best warm layout() %.3f ms, layout() + nodes() %.3f ms, upcalls so far %d", bench.nodeCount(), bestLayout, best, measurer.intrinsics + measurer.heights))
check(bestLayout < 2.0, "bench: warm pass \(bestLayout) ms")

// --- events + overlays ---------------------------------------------------------
let s = try! Surface(surfaceId: "s", catalogId: coreCatalogId(), themeId: nil, mode: "dark")
_ = try! s.apply(messageJson: text(["version": "v0.9", "updateComponents": ["surfaceId": "s", "components": [
    ["id": "root", "component": "Box", "style": ["display": "flex", "flexDirection": "column"], "children": ["dlg", "t"]],
    ["id": "t", "component": "Text", "text": ["path": "/title"]],
    ["id": "dlg", "component": "Dialog", "title": "Hi", "open": ["path": "/open"], "slots": ["trigger": "b"], "children": ["body"]],
    ["id": "b", "component": "Button", "label": "Open", "on": ["press": ["event": ["name": "opened"]]]],
    ["id": "body", "component": "Text", "text": "Body"],
]]]))
try! s.setData(path: "/title", valueJson: "\"Hello\"")
_ = s.setViewport(width: 400, height: 700, maxHeight: nil)
check(try! s.layoutFixed(sizesJson: nil, wrap: true).layers.isEmpty, "dialog starts closed")
let b = s.indexOf(id: "b")!
let events = try! s.event(index: b, name: "press", payloadJson: nil).map { $0.kind }
check(events.contains("action") && events.contains("dataChanged") && events.contains("relayout"), "press events \(events)")
let opened = try! s.layoutFixed(sizesJson: nil, wrap: true)
check(opened.layers.count == 1 && opened.layers[0].position == "centered", "dialog layer")
check(s.nodes().contains { $0.id == "dlg.title" && $0.ownerComponent == "Dialog" }, "title part")
check(s.visuals().count == s.nodes().count, "visuals per node")

// --- round 1: the bind pass, the tokenizer, settings, deltas, baselines -----
func findNode(_ n: [String: Any], _ id: String) -> [String: Any]? {
    if n["id"] as? String == id { return n }
    for (_, slot) in (n["slots"] as? [String: Any]) ?? [:] { if let hit = findNode(slot as! [String: Any], id) { return hit } }
    for child in (n["children"] as? [Any]) ?? [] { if let hit = findNode(child as! [String: Any], id) { return hit } }
    return nil
}
let bind = load("bind-time.json") as! [String: Any]
for c in bind["cases"] as! [[String: Any]] {
    for d in c["datasets"] as! [[String: Any]] {
        let got = (try! bindTreeJson(nodeJson: text(c["expanded"]!), dataJson: text(d["data"]!), scope: "", stringsJson: nil)) ?? "null"
        equal(got, d["bound"]!, "bind-time \(c["name"]!)")
        for p in d["presses"] as! [[String: Any]] {
            let node = findNode(c["expanded"] as! [String: Any], p["id"] as! String)!
            let on = node["on"] as! [String: Any]
            equal(try! runActionJson(actionJson: text(on["press"]!), dataJson: text(d["data"]!), scope: "", stringsJson: nil), p["outcome"]!, "bind-time press \(c["name"]!) \(p["id"]!)")
        }
    }
}
let codeTokens = load("code-tokens.json") as! [String: Any]
for c in codeTokens["cases"] as! [[String: Any]] {
    equal(tokenizeCodeJson(code: c["code"] as! String, language: c["language"] as! String), c["expected"]!, "code-tokens \(c["name"]!)")
}
check(weekStart(locale: "de-DE") == 1 && textDirection(locale: "ar") == "rtl", "locale")

let r1 = try! Surface(surfaceId: "r1", catalogId: coreCatalogId(), themeId: nil, mode: "light")
let rows: [[String: Any]] = (0..<200).map { ["id": "r\($0)", "component": "Text", "props": ["text": "Row \($0)"]] }
_ = try! r1.setNested(nestedJson: text(["id": "root", "component": "Box", "style": ["display": "flex", "flexDirection": "column", "width": "100%"], "children": [
    ["id": "list", "component": "List", "style": ["height": 400], "children": rows]]]))
_ = r1.setViewport(width: 390, height: 844, maxHeight: nil)
var st = r1.settings()
st.mode = "system"
st.systemDark = true
st.insetBottom = 34
st.fontScale = 1.25
try! r1.setSettings(settings: st)
check(r1.settings().mode == "system", "settings round trip")
_ = try! r1.layoutFixed(sizesJson: nil, wrap: true)
_ = try! r1.layoutFixed(sizesJson: nil, wrap: true)
_ = r1.scroll(listId: "list", offset: 3000)
let sc = try! r1.layoutFixed(sizesJson: nil, wrap: true)
check(!sc.delta.added.isEmpty && r1.nodesAt(indices: sc.delta.added).count == sc.delta.added.count, "delta \(sc.delta)")
check(sc.scrolls.contains { $0.scrollY && $0.offsetY == 3000 }, "scrolls")

final class BaselineMeasurer: Measurer, @unchecked Sendable {
    func measureId() -> UInt64 { 6 }
    func measureIntrinsics(leaves: [FfiLeaf]) -> [FfiIntrinsics] {
        leaves.map { l in
            let button = l.component == "Button"
            return FfiIntrinsics(minContentWidth: 8 * Float(l.text.count), maxContentWidth: 8 * Float(l.text.count), heightAtMaxContent: button ? 40 : 20, baseline: button ? 25 : 15)
        }
    }
    func measureHeights(leaves: [FfiLeaf], requests: [FfiHeightRequest]) -> [Float] { requests.map { _ in 20 } }
}
let bl = try! Surface(surfaceId: "bl", catalogId: coreCatalogId(), themeId: "", mode: "light")
_ = try! bl.setNested(nestedJson: text(["id": "root", "component": "Box", "style": ["display": "flex", "flexDirection": "row", "alignItems": "baseline"], "children": [
    ["id": "b", "component": "Button", "props": ["label": "Go"]], ["id": "t", "component": "Text", "props": ["text": "Label"]]]]))
_ = bl.setViewport(width: 300, height: 0, maxHeight: nil)
let blOut = bl.layout(measurer: BaselineMeasurer())
let tIndex = bl.indexOf(id: "t")!
check(blOut.frames.first { $0.index == tIndex }!.y == 10, "baseline alignment")
print("round 1: \(checks) checks so far")

// --- host API (VAPP-91) -----------------------------------------------------------
func decodeFeed(_ push: (String) -> String, _ end: () -> String, _ chunks: [String]) -> [String: Any] {
    var messages: [Any] = []
    var issues: [Any] = []
    for out in chunks.map(push) + [end()] {
        let v = try! JSONSerialization.jsonObject(with: out.data(using: .utf8)!) as! [String: Any]
        messages += v["messages"] as! [Any]
        issues += v["issues"] as! [Any]
    }
    return ["messages": messages, "issues": issues]
}
let transport = load("host-transport.json") as! [String: Any]
for c in transport["jsonl"] as! [[String: Any]] {
    let d = JsonlDecoder()
    equal(text(decodeFeed({ d.push(chunk: $0) }, { d.end() }, c["chunks"] as! [String])), c["expected"]!, "jsonl \(c["name"]!)")
}
for c in transport["sse"] as! [[String: Any]] {
    let d = SseDecoder()
    equal(text(decodeFeed({ d.push(chunk: $0) }, { d.end() }, c["chunks"] as! [String])), c["expected"]!, "sse \(c["name"]!)")
}
for c in transport["mcp"] as! [[String: Any]] {
    equal(try! messagesFromMcpResultJson(resultJson: text(c["result"]!)), c["expected"]!, "mcp \(c["name"]!)")
}
for c in transport["mcpAction"] as! [[String: Any]] {
    equal(try! mcpActionCallJson(messageJson: text(c["message"]!), tool: c["tool"] as? String), c["expected"]!, "mcpAction \(c["name"]!)")
}
let policy = load("host-policy.json") as! [String: Any]
for c in policy["functions"] as! [[String: Any]] {
    let got = try! decideFunction(policyJson: c["policy"].map(text), name: c["fn"] as! String, registered: c["registered"] as! Bool)
    check(got == c["expected"] as! String, "function \(c["name"]!): \(got)")
}
for c in policy["combine"] as! [[String: Any]] {
    check(try! combineDecisions(a: c["a"] as! String, b: c["b"] as! String) == c["expected"] as! String, "combine \(c)")
}
for c in policy["urls"] as! [[String: Any]] {
    equal(try! decideUrlJson(policyJson: c["policy"].map(text), url: c["url"] as! String), c["expected"]!, "url \(c["name"]!)")
}
for c in policy["media"] as! [[String: Any]] {
    equal(try! mediaRequestJson(url: c["url"] as! String, optionsJson: text(c["options"]!)) ?? "null", c["expected"]!, "media \(c["name"]!)")
}
for c in policy["sources"] as! [[String: Any]] {
    equal(parseSourceJson(uri: c["uri"] as! String) ?? "null", c["expected"]!, "source \(c["uri"]!)")
}
for c in policy["negotiation"] as! [[String: Any]] {
    let ids = c["extensionIds"] as! [String]
    let expected = c["expected"] as! [String: Any]
    check(supportedCatalogIdsFor(extensionIds: ids) == expected["supportedCatalogIds"] as! [String], "negotiation \(ids)")
    equal(clientCapabilitiesJson(extensionIds: ids), expected["clientCapabilities"]!, "capabilities \(ids)")
}
let routerFixture = load("host-router.json") as! [String: Any]
let hostPackages = routerFixture["packages"] as! [String: Any]
for v in routerFixture["validation"] as! [[String: Any]] {
    let id = v["package"] as! String
    equal(try! validatePackageJson(packageJson: text(hostPackages[id]!), catalogIds: nil), v["expected"]!, "validation \(id)")
}
for flow in routerFixture["flows"] as! [[String: Any]] {
    let name = flow["name"] as! String
    let router = HostRouter(extensionIds: flow["extensionIds"] as? [String] ?? [])
    for id in flow["packages"] as? [String] ?? [] {
        equal(try! router.installPackage(packageJson: text(hostPackages[id]!)), (flow["installIssues"] as! [String: Any])[id]!, "\(name): install \(id)")
    }
    for (i, step) in (flow["steps"] as! [[String: Any]]).enumerated() {
        equal(router.route(messageJson: text(step["message"]!)), step["expected"]!, "\(name): step \(i)")
    }
}
let hostRouter = HostRouter(extensionIds: [])
_ = try! hostRouter.installPackage(packageJson: text(hostPackages["acme.devices"]!))
_ = hostRouter.route(messageJson: text(["version": "v0.9", "applyTemplate": ["surfaceId": "d", "templateId": "list"]]))
check(hostRouter.surfaceIds() == ["d"] && hostRouter.packageIdOf(surfaceId: "d") == "acme.devices", "router state")
equal(errorMessageJson(code: "FUNCTION_DENIED", surfaceId: "s", message: "m", path: nil), ["version": "v0.9", "error": ["code": "FUNCTION_DENIED", "surfaceId": "s", "message": "m"]], "errorMessage")
equal(try! actionMessageJson(surfaceId: "s", componentId: "b", name: "go", contextJson: "{}", payloadJson: nil, timestamp: "t"), ["version": "v0.9", "action": ["name": "go", "surfaceId": "s", "sourceComponentId": "b", "timestamp": "t", "context": [String: Any]()]], "actionMessage")
check(try! templateMessagesJson(packageJson: text(hostPackages["acme.devices"]!), templateId: "nope", surfaceId: "x", dataJson: nil) == nil, "missing template")
let fnSurface = try! Surface(surfaceId: "f", catalogId: coreCatalogId(), themeId: nil, mode: "light")
_ = try! fnSurface.setNested(nestedJson: text(["id": "root", "component": "Box", "children": [["id": "b", "component": "Button", "props": ["label": "Go"], "on": ["press": ["functionCall": ["call": "harness.toast", "args": ["message": ["path": "/m"]]]]]]]]))
try! fnSurface.setData(path: "/m", valueJson: "\"hi\"")
_ = fnSurface.setViewport(width: 400, height: 0, maxHeight: nil)
_ = try! fnSurface.layoutFixed(sizesJson: nil, wrap: false)
let fnEvents = try! fnSurface.event(index: fnSurface.indexOf(id: "b")!, name: "press", payloadJson: nil)
let fnCall = fnEvents.first { $0.kind == "functionCall" }
check(fnCall != nil, "functionCall event \(fnEvents.map { $0.kind })")
if let fnCall { equal(fnCall.json, ["kind": "functionCall", "componentId": "b", "name": "harness.toast", "args": ["message": "hi"]], "functionCall json") }
print("host fixtures: \(checks) checks so far")

print("swift binding suite: \(checks) checks, \(failures) failures")
exit(failures == 0 ? 0 : 1)
