import Foundation
import ExponentialUICore
@testable import ExponentialUI

/// The manifest-v2 suites (round 1's bind / style / code fixtures and the
/// round-2 contract fixtures) replayed case by case THROUGH this painter's
/// inputs: the facade bindings for the core's arithmetic, the painter's
/// Foundation `FoundationFormatter` for `format`, a live `Surface`'s
/// visuals for the animation timings and the leaves' direction. Each case =
/// (name, nil) or (name, the first difference). The Rust core's
/// `tests/support/round2.rs` and Compose's `Round2Suites.kt` are the twins;
/// the counts follow `manifest.json` `unit`.
enum Round2Suites {
    typealias Case = (name: String, error: String?)

    static func json(_ name: String) throws -> JSONValue { try Fixtures.json(name) }
    static func list(_ v: JSONValue?, _ key: String) -> [JSONValue] { v?[key]?.array ?? [] }
    static func s(_ v: JSONValue?, _ key: String) -> String { v?[key]?.string ?? "" }
    /// A JSON number only (`JSONValue.number` also reads numeric strings).
    static func num(_ v: JSONValue?) -> Double? { if case let .number(n)? = v { return n } else { return nil } }

    static func run(_ name: String, _ check: () throws -> String?) -> Case {
        do { return (name, try check()) } catch { return (name, "threw \(error)") }
    }

    static func same(_ got: String, _ expected: JSONValue) -> String? {
        jsonEqual(a: got, b: expected.json) ? nil : "differs at \(jsonDiff(a: got, b: expected.json).isEmpty ? "/" : jsonDiff(a: got, b: expected.json)): got \(got.prefix(300))"
    }

    static func near(_ got: [Double], _ want: [Double], _ eps: Double) -> String? {
        got.count == want.count && zip(got, want).allSatisfy { abs($0 - $1) <= eps } ? nil : "got \(got) / expected \(want)"
    }

    static func nums(_ v: String) -> [Double] { JSONValue.parse(v).array?.map { num($0) ?? .nan } ?? [] }

    static func merged(_ v: JSONValue, _ extra: [String: JSONValue], keep: Set<String>? = nil) -> JSONValue {
        var o = v.object ?? [:]
        if let keep { o = o.filter { keep.contains($0.key) } }
        for (k, x) in extra { o[k] = x }
        return .object(o)
    }

    /// A node of an expanded tree by id (children, then slot values).
    static func find(_ node: JSONValue?, _ id: String) -> JSONValue? {
        guard let node else { return nil }
        if node["id"]?.string == id { return node }
        for c in list(node, "children") { if let hit = find(c, id) { return hit } }
        for c in (node["slots"]?.object ?? [:]).values { if let hit = find(c, id) { return hit } }
        return nil
    }

    // MARK: round 1 fixtures (counted since manifest v2)

    static func bind() throws -> [Case] {
        let f = try json("bind-time.json")
        return (list(f, "cases") + list(f, "extra")).map { c in
            run(s(c, "name")) {
                let reduced = JSONValue.parse(try reduceNestedJson(nestedJson: c["input"]!.json, catalogId: coreCatalogId(), extensionsJson: nil))
                if let d = same((reduced["issues"] ?? .array([])).json, c["issues"] ?? .array([])) { return "issues \(d)" }
                if let d = same(reduced["root"]!.json, c["expanded"]!) { return "expanded \(d)" }
                let expanded = c["expanded"]!
                for d in list(c, "datasets") {
                    let data = d["data"] ?? .object([:])
                    let bound = try bindTreeJson(nodeJson: expanded.json, dataJson: data.json, scope: "", stringsJson: nil)
                    if let x = same(bound ?? "null", d["bound"] ?? .null) { return "bound \(data.json): \(x)" }
                    for p in list(d, "presses") {
                        let id = s(p, "id")
                        guard let node = find(expanded, id) else { return "no \(id)" }
                        guard let action = node["on"]?["press"] else { return "no on.press on \(id)" }
                        if let x = same(try runActionJson(actionJson: action.json, dataJson: data.json, scope: "", stringsJson: nil), p["outcome"]!) { return "press \(id): \(x)" }
                    }
                    for t in list(d, "rowSlots") {
                        let id = s(t, "id")
                        guard let boundNode = find(bound.map(JSONValue.parse), id) else { return "no bound \(id)" }
                        let rowsProp = find(expanded, id)?["props"]?["rows"] ?? .null
                        let rows = boundNode["props"]?["rows"] ?? .array([])
                        for r in list(t, "rows") {
                            let index = Int(num(r["index"]) ?? 0)
                            for (slotName, expected) in r["slots"]?.object ?? [:] {
                                guard let slot = boundNode["slots"]?[slotName] else { return "no slot \(slotName)" }
                                let got = try bindRowSlotJson(slotJson: slot.json, rowsPropJson: rowsProp.json, rowsJson: rows.json, index: UInt32(index), dataJson: data.json, optionsJson: nil)
                                if let x = same(got ?? "null", expected) { return "row \(index) \(slotName): \(x)" }
                            }
                        }
                    }
                }
                return nil
            }
        }
    }

    static func styleConditions() throws -> [Case] {
        let f = try json("style-conditions.json")
        let defaults = f["breakpoints"]
        // The style goes in as the fixture's own text: blocks apply in KEY
        // order (`JSONValue` objects are unordered).
        let raw = RawJSON.arrayElements(in: RawJSON.value(of: "cases", inObject: try Fixtures.text("style-conditions.json")) ?? "[]")
        return list(f, "cases").enumerated().map { i, c in
            run(s(c, "name")) {
                let style = i < raw.count ? (RawJSON.value(of: "style", inObject: raw[i]) ?? c["style"]!.json) : c["style"]!.json
                for (ctx, expected) in zip(list(c, "contexts"), list(c, "expected")) {
                    var context = ctx.object ?? [:]
                    if context["breakpoints"] == nil, let defaults { context["breakpoints"] = defaults }
                    if let d = same(try resolveConditionsJson(styleJson: style, contextJson: JSONValue.object(context).json), expected) { return "\(s(ctx, "label")): \(d)" }
                }
                return nil
            }
        }
    }

    static func codeTokens() throws -> [Case] {
        list(try json("code-tokens.json"), "cases").map { c in
            run(s(c, "name")) { same(tokenizeCodeJson(code: s(c, "code"), language: s(c, "language")), c["expected"]!) }
        }
    }

    // MARK: round 2

    /// Every call through the PAINTER's formatter (Foundation, en-US / UTC:
    /// the fixture's), every zoned call through the core's English fallback
    /// at the case's offset and then Foundation in the case's IANA zone
    /// (U+202F / U+00A0 read as spaces on both sides), every display value.
    static func format() throws -> [Case] {
        let f = try json("format.json")
        let locale = s(f, "locale").isEmpty ? "en-US" : s(f, "locale")
        let formatter = FoundationFormatter(locale: locale, timeZone: s(f, "timeZone").isEmpty ? "UTC" : s(f, "timeZone"))
        func spaces(_ t: String) -> String { t.replacingOccurrences(of: "\u{202F}", with: " ").replacingOccurrences(of: "\u{00A0}", with: " ") }
        func spaced(_ v: JSONValue) -> JSONValue { v.string.map { .string(spaces($0)) } ?? v }
        var out: [Case] = list(f, "calls").map { c in
            run("call \(s(c, "name"))") { same(spaces(try formatCallJson(callJson: c["call"]!.json, formatter: formatter, now: nil) ?? "null"), spaced(c["expected"]!)) }
        }
        out += list(f, "zoned").map { c in
            run("zoned \(s(c, "name"))") {
                let offset = Int32(num(c["offsetMinutes"]) ?? 0)
                if let d = same(try formatCallJson(callJson: c["call"]!.json, formatter: nil, now: nil, offsetMinutes: offset) ?? "null", c["expected"]!) { return "fallback \(d)" }
                let zoned = FoundationFormatter(locale: "en-US", timeZone: s(c, "timeZone"))
                let got = spaces(try formatCallJson(callJson: c["call"]!.json, formatter: zoned, now: nil) ?? "null")
                return same(got, spaced(c["expected"]!)).map { "foundation \($0)" }
            }
        }
        // Display values go in as the fixture's own text (a 21-digit integer
        // would not survive a Double round trip through `JSONValue`).
        let raw = RawJSON.arrayElements(in: RawJSON.value(of: "display", inObject: try Fixtures.text("format.json")) ?? "[]")
        out += list(f, "display").enumerated().map { i, d in
            run("display \(s(d, "name"))") {
                let value = i < raw.count ? (RawJSON.value(of: "value", inObject: raw[i]) ?? d["value"]!.json) : d["value"]!.json
                return same(JSONValue.string(try displayStringJson(valueJson: value)).json, d["expected"]!)
            }
        }
        return out
    }

    /// A key as one instance-suffix segment (`instanceSegment`): `~` → `~0`, `.` → `~1`.
    static func instanceSegment(_ key: String) -> String { key.replacingOccurrences(of: "~", with: "~0").replacingOccurrences(of: ".", with: "~1") }

    static func readPointer(_ v: JSONValue, _ pointer: String) -> JSONValue? {
        if pointer.isEmpty { return v }
        var cur: JSONValue? = v
        for raw in pointer.dropFirst(pointer.hasPrefix("/") ? 1 : 0).split(separator: "/", omittingEmptySubsequences: false) {
            let seg = raw.replacingOccurrences(of: "~1", with: "/").replacingOccurrences(of: "~0", with: "~")
            switch cur {
            case let .object(o)?: cur = o[seg]
            case let .array(a)?: cur = Int(seg).flatMap { $0 >= 0 && $0 < a.count ? a[$0] : nil }
            default: return nil
            }
            if cur == nil { return nil }
        }
        return cur
    }

    /// `templateInstances` (src/list.ts): the items at the template's path, keyed by the core's `keys`.
    static func instances(_ data: JSONValue, _ template: JSONValue, _ scope: String, _ instance: String) throws -> [JSONValue] {
        let p = s(template, "path")
        let path = p.hasPrefix("/") ? p : (p.isEmpty ? scope : "\(scope)/\(p)")
        guard let items = readPointer(data, path)?.array else { return [] }
        var req: [String: JSONValue] = ["op": .string("keys"), "items": .array(items)]
        if let k = template["key"]?.string { req["key"] = .string(k) }
        let keys = (JSONValue.parse(try listJson(requestJson: JSONValue.object(req).json)).array ?? []).map(\.displayText)
        return items.indices.map { i in
            .object(["key": .string(keys[i]), "path": .string("\(path)/\(i)"), "index": .number(Double(i)), "instance": .string("\(instance).\(instanceSegment(keys[i]))")])
        }
    }

    static func templateItems() throws -> [Case] {
        let f = try json("template-items.json")
        var out: [Case] = []
        for c in list(f, "keys") {
            out.append(run("keys \(s(c, "name"))") { same(try listJson(requestJson: merged(c, ["op": .string("keys")], keep: ["items", "key"]).json), c["expected"]!) })
        }
        for c in list(f, "rowKeys") {
            out.append(run("rowKeys \(s(c, "name"))") { same(try listJson(requestJson: merged(c, ["op": .string("rowKeys")], keep: ["rows", "rowKey"]).json), c["expected"]!) })
        }
        for c in list(f, "instances") {
            out.append(run("instances \(s(c, "name"))") {
                let data = c["data"]!
                let outer = try instances(data, c["template"]!, s(c, "scope"), s(c, "instance"))
                guard let inner = c["inner"] else { return same(JSONValue.array(outer).json, c["expected"]!) }
                var nested: [JSONValue] = []
                for o in outer {
                    for i in try instances(data, inner, s(o, "path"), s(o, "instance")) {
                        let inst = s(i, "instance")
                        nested.append(.object([
                            "outer": o["key"]!, "key": i["key"]!, "path": i["path"]!, "index": i["index"]!, "instance": .string(inst),
                            "ids": .object(["issue": .string("issue\(inst)"), "title": .string("issue.title\(inst)")]),
                        ]))
                    }
                }
                return same(JSONValue.array(nested).json, c["expected"]!)
            })
        }
        for c in list(f, "reduce") {
            out.append(run("reduce \(s(c, "name"))") {
                let catalog = s(c, "catalogId").isEmpty ? coreCatalogId() : s(c, "catalogId")
                let got = c["nested"] != nil
                    ? try reduceNestedJson(nestedJson: c["nested"]!.json, catalogId: catalog, extensionsJson: nil)
                    : try reduceSurfaceJson(componentsJson: c["components"]!.json, catalogId: catalog, extensionsJson: nil)
                if let d = same(got, c["expected"]!) { return d }
                // A lifted template never also renders in place.
                let parsed = JSONValue.parse(got)
                for id in (parsed["templates"]?.object ?? [:]).keys where find(parsed["root"], id) != nil { return "\(id) in place" }
                return nil
            })
        }
        return out
    }

    /// `text-direction.json`: every node's direction and physical alignment,
    /// and the LEAVES' as a surface hands them to this painter (the visual's
    /// `direction` + `textAlign`).
    static func textDirection() throws -> [Case] {
        list(try json("text-direction.json"), "cases").map { c in
            run(s(c, "name")) {
                let surfaceDir = s(c, "surface").isEmpty ? "ltr" : s(c, "surface")
                let root = JSONValue.parse(try reduceNestedJson(nestedJson: c["tree"]!.json, catalogId: coreCatalogId(), extensionsJson: nil))["root"]!
                var got: [String: JSONValue] = [:]
                func physical(_ align: String, _ dir: String) -> String {
                    switch align {
                    case "start": dir == "rtl" ? "right" : "left"
                    case "end": dir == "rtl" ? "left" : "right"
                    default: align
                    }
                }
                func walk(_ n: JSONValue, _ parentDir: String) {
                    let dir = n["style"]?["direction"]?.string ?? parentDir
                    let align = n["style"]?["textAlign"]?.string ?? (n["component"]?.string == "Text" ? n["props"]?["align"]?.string : nil) ?? "start"
                    got[s(n, "id")] = .object(["direction": .string(dir), "textAlign": .string(physical(align, dir))])
                    for k in list(n, "children") { walk(k, dir) }
                    for k in (n["slots"]?.object ?? [:]).values { walk(k, dir) }
                }
                walk(root, surfaceDir)
                if let d = same(JSONValue.object(got).json, c["expected"]!) { return d }
                // What the painter receives: the leaves' resolved direction + align.
                var tree = c["tree"]!.object ?? [:]
                if tree["style"]?["direction"] == nil {
                    var style = tree["style"]?.object ?? [:]
                    style["direction"] = .string(surfaceDir)
                    tree["style"] = .object(style)
                }
                let surface = try Surface.withTheme(surfaceId: "dir", catalogId: coreCatalogId(), theme: try Theme.builtin(id: "neutral"), mode: "light")
                _ = try surface.setNested(nestedJson: JSONValue.object(tree).json)
                _ = surface.setViewport(width: 390, height: 800, maxHeight: nil)
                _ = try surface.layoutFixed(sizesJson: nil, wrap: true)
                let visuals = surface.visuals()
                for n in surface.nodes() where n.isLeaf {
                    guard let want = c["expected"]?[n.id] else { continue }
                    guard Int(n.index) < visuals.count, let v = visuals[Int(n.index)] as FfiVisual? else { return "no visual for \(n.id)" }
                    if v.direction != s(want, "direction") { return "\(n.id): visual direction \(v.direction ?? "nil") ≠ \(s(want, "direction"))" }
                    if v.textAlign != s(want, "textAlign") { return "\(n.id): visual textAlign \(v.textAlign ?? "nil") ≠ \(s(want, "textAlign"))" }
                }
                return nil
            }
        }
    }

    static func resizable() throws -> [Case] {
        let f = try json("resizable.json")
        var out: [Case] = []
        func sum100(_ v: [Double]) -> String? { abs(v.reduce(0, +) - 100) <= 1e-5 ? nil : "sum \(v)" }
        for (op, key) in [("normalize", "normalize"), ("resize", "resize"), ("key", "keys"), ("extents", "extents")] {
            for c in list(f, key) {
                out.append(run("\(key) \(s(c, "name"))") {
                    let got = nums(try resizableJson(requestJson: merged(c, ["op": .string(op)]).json))
                    return near(got, (c["expected"]?.array ?? []).map { num($0) ?? .nan }, 1e-6) ?? (op == "extents" ? nil : sum100(got))
                })
            }
        }
        for c in list(f, "drag") {
            out.append(run("drag \(s(c, "name"))") {
                let got = num(JSONValue.parse(try resizableJson(requestJson: merged(c, ["op": .string("drag")]).json))) ?? .nan
                return near([got], [num(c["expected"]) ?? .nan], 1e-6)
            })
        }
        return out
    }

    static func extents(_ v: JSONValue?) -> JSONValue {
        if case .array? = v { return v! }
        return .array(Array(repeating: v?["extent"] ?? .number(0), count: Int(num(v?["count"]) ?? 0)))
    }

    static func virtualList() throws -> [Case] {
        let f = try json("virtual-list.json")
        var out: [Case] = []
        for c in list(f, "windows") {
            out.append(run("window \(s(c, "name"))") {
                same(try listJson(requestJson: merged(c, ["extents": extents(c["extents"]), "op": .string("window")], keep: ["gap", "scroll", "viewport", "overscan"]).json), c["expected"]!)
            })
        }
        for c in list(f, "scrollTo") {
            out.append(run("scrollTo \(s(c, "name"))") {
                same(try listJson(requestJson: merged(c, ["extents": extents(c["extents"]), "op": .string("scrollTo")], keep: ["gap", "index", "viewport", "scroll", "align", "inset"]).json), c["expected"]!)
            })
        }
        for c in list(f, "sections") {
            out.append(run("sections \(s(c, "name"))") {
                same(try listJson(requestJson: merged(c, ["op": .string("sections")], keep: ["items", "sectionBy"]).json), c["expected"]!)
            })
        }
        for c in list(f, "sectionedScrollTo") {
            out.append(run("sectionedScrollTo \(s(c, "name"))") {
                let sections = JSONValue.parse(try listJson(requestJson: JSONValue.object(["op": .string("sections"), "items": c["items"]!, "sectionBy": c["sectionBy"]!]).json))
                let rows = sections["rows"]?.array ?? []
                let header = num(c["headerExtent"]) ?? 0, item = num(c["itemExtent"]) ?? 0
                let ext = JSONValue.array(rows.map { .number($0["header"] != nil ? header : item) })
                let req = merged(c, ["op": .string("scrollToItem"), "rows": .array(rows), "rowExtents": ext], keep: ["gap", "index", "viewport", "scroll", "align", "stickyHeaders"])
                return same(try listJson(requestJson: req.json), c["expected"]!)
            })
        }
        for c in list(f, "sticky") {
            out.append(run("sticky \(s(c, "name"))") {
                let got = try (c["scrolls"]?.array ?? []).map { sc in
                    JSONValue.parse(try listJson(requestJson: JSONValue.object(["op": .string("sticky"), "rowExtents": c["rowExtents"]!, "headerRows": c["headerRows"]!, "scroll": sc]).json))
                }
                return same(JSONValue.array(got).json, c["expected"]!)
            })
        }
        return out
    }

    /// A node's animation timing as the surface resolves it for this painter (the visual's `animation.timing`).
    static func timing(_ theme: String, _ name: String, _ duration: String?) throws -> JSONValue? {
        var style: [String: JSONValue] = ["animation": .string(name)]
        if let duration { style["animationDuration"] = .string(duration) }
        let surface = try Surface.withTheme(surfaceId: "anim", catalogId: coreCatalogId(), theme: try Theme.builtin(id: theme), mode: "light")
        _ = try surface.setNested(nestedJson: JSONValue.object(["id": .string("a"), "component": .string("Box"), "style": .object(style)]).json)
        _ = surface.setViewport(width: 100, height: 100, maxHeight: nil)
        _ = try surface.layoutFixed(sizesJson: nil, wrap: true)
        guard let v = surface.visuals().first?.animationJson else { return nil }
        return JSONValue.parse(v)["timing"]
    }

    static func frameNear(_ got: JSONValue, _ want: JSONValue) -> String? {
        for (k, v) in want.object ?? [:] {
            let g = got[k]
            let ok: Bool
            if let a = num(v), let b = num(g) { ok = abs(a - b) <= 1e-3 } else { ok = v == .null && (g == nil || g == .null) }
            if !ok { return "\(k): got \(g.map(\.json) ?? "nil") / expected \(v.json)" }
        }
        return nil
    }

    /// `style.json` `animations` → `@keyframes xui-<name>` (src/animation.ts `keyframesCss`).
    static func keyframesCss(_ name: String) throws -> String {
        let style = JSONValue.parse(try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("catalog/style.json"), encoding: .utf8))
        guard let def = style["animations"]?[name] else { return "" }
        func css(_ n: Double) throws -> String { try displayStringJson(valueJson: JSONValue.number((n * 10000 + 0.5).rounded(.down) / 10000).json) }
        let steps = try list(def, "keyframes").map { k -> String in
            var decl: [String] = []
            // Opacity moves `--xui-a-opacity` (the node's own opacity multiplies it).
            if let o = num(k["opacity"]) { decl.append("--xui-a-opacity:\(try css(o))") }
            if k["translateX"] != nil || k["translateY"] != nil { decl.append("translate:\(try css(num(k["translateX"]) ?? 0))px \(try css(num(k["translateY"]) ?? 0))px") }
            if let r = num(k["rotate"]) { decl.append("rotate:\(try css(r))deg") }
            if let sc = num(k["scale"]) { decl.append("scale:\(try css(sc))") }
            if let b = num(k["band"]) { decl.append("--xui-band:\(try css(b))") }
            return "\(try css((num(k["offset"]) ?? 0) * 100))%{\(decl.joined(separator: ";"))}"
        }
        return "@keyframes xui-\(name){\(steps.joined())}"
    }

    static func animations() throws -> [Case] {
        let f = try json("animations.json")
        var out: [Case] = []
        for (id, entries) in (f["themes"]?.object ?? [:]).sorted(by: { $0.key < $1.key }) {
            for (name, e) in (entries.object ?? [:]).sorted(by: { $0.key < $1.key }) {
                out.append(run("\(id)/\(name)") {
                    guard let t = try timing(id, name, nil) else { return "no animation on the visual" }
                    if let d = same(t.json, e["timing"]!) { return "timing \(d)" }
                    for fr in list(e, "frames") {
                        let at = num(fr["t"]) ?? 0
                        guard let frame = try animationFrameJson(name: name, timingJson: t.json, elapsedMs: at, reducedMotion: false) else { return "no frame @\(at)" }
                        if let d = frameNear(JSONValue.parse(frame), fr["frame"]!) { return "@\(at) \(d)" }
                    }
                    if let d = same(try animationFrameJson(name: name, timingJson: t.json, elapsedMs: 5, reducedMotion: true) ?? "null", e["reduced"]!) { return "reduced \(d)" }
                    return same(JSONValue.string(try keyframesCss(name)).json, f["css"]![name]!).map { "css \($0)" }
                })
            }
        }
        // The painted opacity = the node's own × the frame's.
        for c in list(f, "opacity") {
            out.append(run("opacity \(s(c, "name"))") {
                let name = s(c, "animation")
                guard let t = try timing(s(c, "theme"), name, nil) else { return "no animation on the visual" }
                let frame = JSONValue.parse(try animationFrameJson(name: name, timingJson: t.json, elapsedMs: num(c["t"]) ?? 0, reducedMotion: false) ?? "{}")
                let got = AnimationFrame(frame).paintedOpacity(own: num(c["own"]) ?? 1)
                let want = num(c["expected"]) ?? .nan
                return abs(got - want) <= 1e-3 ? nil : "\(got) vs \(want)"
            })
        }
        let o = f["override"]!
        out.append(run("override") {
            guard let t = try timing(s(o, "theme"), s(o, "name"), s(o, "durationToken")) else { return "no animation on the visual" }
            if let d = same(t.json, o["timing"]!) { return d }
            guard let frame = try animationFrameJson(name: s(o, "name"), timingJson: t.json, elapsedMs: 60, reducedMotion: false) else { return "no frame" }
            return frameNear(JSONValue.parse(frame), o["frame"]!)
        })
        return out
    }
}
