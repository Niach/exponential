import Foundation
import CoreText
import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// The SwiftUI half of the renderer conformance harness
/// (`packages/exponential-ui/conformance`, round 1): every case of
/// `fixtures/conformance-cases.json` (fixture × theme × mode × width ×
/// direction) laid out by `SurfaceModel` with the painter's REAL measurer
/// (CoreText) and ONLY the conformance font files of `conformance/fonts.json`
/// (`TextFonts.Set`, exclusive: theme families map to those files, the
/// system font and unmapped families to the `default` one), dumped in the
/// shared frame-dump format (`conformance/dump.ts`, `xui-frame-dump/1`,
/// renderer `swiftui-coretext`) exactly as gpui's producer does
/// (`apps/desktop/crates/exponential-ui-gpui/examples/conformance_dump.rs`).
///
/// The gate is a RATCHET (gpui's `tests/conformance.rs`, ported): each case
/// is compared with the committed WEB baseline under the matrix tolerance;
/// its divergence counts may not exceed
/// `packages/exponential-ui-swift/conformance/conformance-known-swift.json`.
/// Worse fails, better asks for the budget to be lowered:
/// `EXPONENTIAL_UI_WRITE_FIXTURES=1 swift test --filter RealFont`.
/// `EXPONENTIAL_UI_CONFORMANCE_DUMP=<path>` also writes the dump (compare it
/// with `conformance/compare.ts`, see `conformance/README.md`);
/// `EXPONENTIAL_UI_CONFORMANCE_VERBOSE=1` lists every cascaded node.
@MainActor
final class RealFontConformanceTests: XCTestCase {
    static let renderer = "swiftui-coretext"
    static let dumpFormat = "xui-frame-dump/1"
    static var repo: URL { Fixtures.dir.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent() }
    static var budgetURL: URL { repo.appendingPathComponent("packages/exponential-ui-swift/conformance/conformance-known-swift.json") }

    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    override func tearDown() async throws {
        TextFonts.install(nil)
    }

    // MARK: - The ratchet

    func testRealFontLayoutMatchesTheWebBaselineWithinTheKnownBudget() throws {
        let manifest = try Self.read("packages/exponential-ui/fixtures/conformance-cases.json")
        let env = ProcessInfo.processInfo.environment
        let only = env["EXPONENTIAL_UI_CONFORMANCE_ONLY"]
        let cases = Self.cases(manifest).filter { only == nil || $0.key.contains(only!) }
        let px = CGFloat(manifest["tolerance"]?["px"]?.number ?? 1)
        let textLines = CGFloat(manifest["tolerance"]?["textLines"]?.number ?? 1)
        let baseline = try Self.decodeBaseline(Self.read("packages/exponential-ui/fixtures/conformance-baseline.json"))
        let dump = try Self.dump(cases, manifest: manifest)
        if let path = env["EXPONENTIAL_UI_CONFORMANCE_DUMP"], !path.isEmpty {
            try Self.write(dump: dump, to: URL(fileURLWithPath: path))
            print("conformance dump: \(path)")
        }
        let known = (try? Self.readAbsolute(Self.budgetURL)) ?? .null
        let write = env["EXPONENTIAL_UI_WRITE_FIXTURES"] == "1"
        let verbose = env["EXPONENTIAL_UI_CONFORMANCE_VERBOSE"] == "1"

        var worse: [String] = []
        var better: [String] = []
        var failedCases: [String] = []
        var budget: [String: JSONValue] = [:]
        var groups: [String: (count: Int, example: String)] = [:]
        var totals = (size: 0, position: 0, wrap: 0, onlyRef: 0, onlyCand: 0, origins: 0)
        for c in cases {
            guard let ref = baseline[c.key] else {
                XCTFail("\(c.key): not in the baseline (rewrite it: bun run --filter @exponential-at/ui conformance -- --write-baseline)")
                continue
            }
            let cand = dump.cases[c.key]!
            let r = Self.compareCase(ref, cand, px: px, textLines: textLines)
            let origins = r.diffs.filter(\.origin)
            print("\n## \(c.key)  matched \(r.matched) · size \(r.size) · position \(r.position) · wrap \(r.wrap) · origins \(origins.count) · only web \(r.onlyRef.count) · only swift \(r.onlyCand.count) · height web \(ref.height) / swift \(cand.height)")
            for d in origins.prefix(verbose ? origins.count : 40) {
                print("  FIX  \(d.line)")
            }
            if !verbose, origins.count > 40 { print("  … \(origins.count - 40) more origins") }
            for d in origins {
                let k = "\(d.id) (\(d.component)) \(d.kinds.joined(separator: "+"))"
                groups[k] = ((groups[k]?.count ?? 0) + 1, groups[k]?.example ?? c.key)
            }
            let cascade = r.diffs.filter { !$0.origin }
            if verbose {
                for d in cascade { print("       \(d.line)") }
            } else if !cascade.isEmpty {
                print("  + \(cascade.count) cascaded (moved/resized by the origins; EXPONENTIAL_UI_CONFORMANCE_VERBOSE=1 lists them): \(cascade.prefix(8).map(\.id).joined(separator: ", "))\(cascade.count > 8 ? " …" : "")")
            }
            if !r.wrapTolerated.isEmpty { print("  rewrapped within tolerance: \(r.wrapTolerated.prefix(12).joined(separator: "; "))\(r.wrapTolerated.count > 12 ? " …" : "")") }
            let list = { (ids: [String]) in verbose || ids.count <= 12 ? ids.joined(separator: ", ") : ids.prefix(12).joined(separator: ", ") + " …" }
            if !r.onlyRef.isEmpty { print("  only in web (\(r.onlyRef.count)): \(list(r.onlyRef))") }
            if !r.onlyCand.isEmpty { print("  only in swift (\(r.onlyCand.count); parts the DOM paints without data-xui-id): \(list(r.onlyCand))") }
            totals.size += r.size; totals.position += r.position; totals.wrap += r.wrap
            totals.onlyRef += r.onlyRef.count; totals.onlyCand += r.onlyCand.count; totals.origins += origins.count

            let now = r.counts
            let was = known["cases"]?[c.key]
            var caseWorse = false
            for k in CaseReport.countKeys {
                let n = now[k] ?? 0
                switch was?[k]?.number.map(Int.init) {
                case .none: worse.append("\(c.key): \(k) \(n) (no budget)"); caseWorse = true
                case .some(let b) where n > b: worse.append("\(c.key): \(k) \(n) > budget \(b)"); caseWorse = true
                case .some(let b) where n < b: better.append("\(c.key): \(k) \(n) < budget \(b)")
                default: break
                }
            }
            if caseWorse { failedCases.append(c.key) }
            budget[c.key] = .object(now.mapValues { .number(Double($0)) })
        }
        print("\n# divergence origins across cases (fix these; the rest cascades from them)")
        let sorted = groups.sorted { $0.value.count != $1.value.count ? $0.value.count > $1.value.count : $0.key < $1.key }
        for (what, g) in sorted.prefix(verbose ? sorted.count : 80) {
            print("  \(String(g.count).leftPad(3))× \(what)   e.g. \(g.example)")
        }
        if !verbose, sorted.count > 80 { print("  … \(sorted.count - 80) more") }
        print("\n# TOTAL \(cases.count) cases: size \(totals.size) · position \(totals.position) · wrap \(totals.wrap) · origins \(totals.origins) · only web \(totals.onlyRef) · only swift \(totals.onlyCand)")

        // The `real-font` suite summary (the manifest adds the suite once
        // the harness lands there; README "With the real-font harness").
        try Self.writeReport(cases: cases.map(\.key), failed: write ? [] : failedCases)

        if write {
            guard only == nil else { return XCTFail("EXPONENTIAL_UI_WRITE_FIXTURES with EXPONENTIAL_UI_CONFORMANCE_ONLY would drop cases from the budget") }
            // gpui's file shape: envelope, then the cases in MATRIX order,
            // each with its counts in `countKeys` order.
            let comment = "The SwiftUI conformance RATCHET (packages/exponential-ui-swift/Tests/ExponentialUITests/RealFontConformanceTests.swift): per case, the divergence counts of the SwiftUI painter (SurfaceModel + the CoreText measurer, the conformance fonts only) against the web baseline (packages/exponential-ui/fixtures/conformance-baseline.json) today. A case may only go DOWN; rewrite after a fix with EXPONENTIAL_UI_WRITE_FIXTURES=1 swift test --filter RealFont. `onlyRef`/`onlyCand` = nodes only the web / only Swift placed (coverage, not compared)."
            let rows = cases.map { c in
                let counts = budget[c.key]?.object ?? [:]
                let inner = CaseReport.countKeys.map { "      \(Self.quote($0)): \(Self.jsString(counts[$0]?.number ?? 0))" }.joined(separator: ",\n")
                return "    \(Self.quote(c.key)): {\n\(inner)\n    }"
            }
            let text = "{\n  \"$comment\": \(Self.quote(comment)),\n  \"renderer\": \(Self.quote(Self.renderer)),\n  \"cases\": {\n\(rows.joined(separator: ",\n"))\n  }\n}\n"
            try FileManager.default.createDirectory(at: Self.budgetURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            try text.write(to: Self.budgetURL, atomically: true, encoding: .utf8)
            print("\nwrote \(Self.budgetURL.path)")
            return
        }
        if !better.isEmpty {
            print("\nIMPROVED — lower the budget (EXPONENTIAL_UI_WRITE_FIXTURES=1 swift test --filter RealFont):\n  \(better.joined(separator: "\n  "))")
        }
        XCTAssertTrue(worse.isEmpty, "the SwiftUI painter diverges MORE from the web baseline than conformance-known-swift.json allows:\n  \(worse.prefix(40).joined(separator: "\n  "))\(worse.count > 40 ? "\n  … \(worse.count - 40) more" : "")")
    }

    // MARK: - The matrix

    struct Case {
        let key: String
        let fixture: String
        let theme: String
        let mode: String
        let width: CGFloat
        let direction: String
    }

    /// `allCases` of `dump.ts`: the manifest's order (fixtures in FILE
    /// order: the `fixtures` object is read order-preserving).
    static func cases(_ manifest: JSONValue) -> [Case] {
        let strs = { (k: String) in manifest[k]?.array?.compactMap(\.string) ?? [] }
        let widths = manifest["widths"]?.array?.compactMap(\.number) ?? []
        var out: [Case] = []
        for fixture in fixtureOrder {
            for theme in strs("themes") {
                for mode in strs("modes") {
                    for w in widths {
                        for dir in strs("directions") {
                            out.append(Case(key: "\(fixture)/\(theme)/\(mode)/\(Int(w))/\(dir)", fixture: fixture, theme: theme, mode: mode, width: CGFloat(w), direction: dir))
                        }
                    }
                }
            }
        }
        return out
    }

    /// The manifest's fixture keys in file order (`JSONValue` objects are
    /// unordered).
    static var fixtureOrder: [String] {
        guard let text = try? String(contentsOf: repo.appendingPathComponent("packages/exponential-ui/fixtures/conformance-cases.json"), encoding: .utf8),
              let range = text.range(of: "\"fixtures\"") else { return [] }
        return RawJSON.objectKeys(in: String(text[range.upperBound...]))
    }

    /// `caseInput` of `dump.ts`: the tree (root `direction` = the case's)
    /// and the data model.
    static func input(_ manifest: JSONValue, _ c: Case) throws -> (tree: [String: JSONValue], data: [String: JSONValue]) {
        guard let spec = manifest["fixtures"]?[c.fixture] else { throw XCTSkip("unknown conformance fixture \(c.fixture)") }
        var tree: [String: JSONValue]
        var data: [String: JSONValue] = [:]
        if let geometry = spec["geometry"]?.string {
            let g = try read(geometry)
            tree = g["surface"]?.object ?? [:]
            data = g["data"]?.object ?? [:]
        } else {
            tree = try read(spec["tree"]!.string!).object ?? [:]
            if let d = spec["data"]?.string {
                data = try read(d).object ?? [:]
                data["$comment"] = nil
            }
        }
        var style = tree["style"]?.object ?? [:]
        style["direction"] = .string(c.direction)
        tree["style"] = .object(style)
        return (tree, data)
    }

    // MARK: - Fonts

    /// `conformance/fonts.json` as an EXCLUSIVE `TextFonts.Set`: each family
    /// with faces gets its files, a substitute the substitute's files under
    /// its own name, every other name (and the system font) the default.
    static func installConformanceFonts(_ manifest: JSONValue) throws -> String {
        let fonts = try read(manifest["fonts"]!.string!)
        var set = TextFonts.Set(exclusive: true)
        var files: [String] = []
        let families = fonts["families"]?.object ?? [:]
        for (name, spec) in families {
            guard let faces = spec["faces"]?.array else { continue }
            let urls = faces.compactMap { $0["file"]?.string }.map { repo.appendingPathComponent($0) }
            for u in urls { XCTAssertTrue(FileManager.default.fileExists(atPath: u.path), "font file \(u.path)") }
            set.add(family: name, files: urls)
            files += faces.compactMap { $0["file"]?.string }
        }
        for (name, spec) in families {
            if let sub = spec["substitute"]?.string { set.aliases[name] = sub }
        }
        set.defaultFamily = fonts["default"]?.string ?? "Inter"
        TextFonts.install(set)
        // The file names, as gpui reports them.
        return files.map { ($0 as NSString).lastPathComponent }.joined(separator: ",")
    }

    // MARK: - The producer

    struct DumpNode: Codable, Equatable {
        var id: String
        var component: String
        var part: String?
        var parent: String?
        var x: CGFloat
        var y: CGFloat
        var w: CGFloat
        var h: CGFloat
        var text: String?
        var lines: Int?
        var lh: CGFloat?
    }

    struct CaseDump: Codable {
        var width: CGFloat
        var height: CGFloat
        var nodes: [DumpNode]
    }

    struct Dump: Codable {
        var format: String
        var renderer: String
        var fonts: String
        var cases: [String: CaseDump]
    }

    /// Lay every case out and dump it.
    static func dump(_ cases: [Case], manifest: JSONValue) throws -> Dump {
        let fonts = try installConformanceFonts(manifest)
        let textComponents = Set(manifest["textComponents"]?.array?.compactMap(\.string) ?? [])
        let locale = manifest["locale"]?.string ?? "en-US"
        var out: [String: CaseDump] = [:]
        for c in cases {
            out[c.key] = try dumpCase(c, manifest: manifest, locale: locale, textComponents: textComponents)
        }
        return Dump(format: dumpFormat, renderer: renderer, fonts: fonts, cases: out)
    }

    static func dumpCase(_ c: Case, manifest: JSONValue, locale: String, textComponents: Set<String>) throws -> CaseDump {
        guard let theme = ThemeHandle.builtin(c.theme) else { throw XCTSkip("unknown theme \(c.theme)") }
        let mode: Mode = c.mode == "light" ? .light : .dark
        var options = SurfaceOptions()
        options.theme = theme
        options.mode = mode
        let m = try SurfaceModel(id: "conformance", options: options, host: NoHost())
        var settings = m.surface.settings()
        settings.locale = locale
        settings.mode = c.mode
        try m.surface.setSettings(settings: settings)
        let (tree, data) = try input(manifest, c)
        let outcome = try m.setNested(json: JSONValue.object(tree).json)
        XCTAssertEqual(JSONValue.parse(outcome.issuesJson).array?.count ?? 0, 0, "\(c.key): \(outcome.issuesJson)")
        for (k, v) in data.sorted(by: { $0.key < $1.key }) {
            m.setData(path: "/\(k)", value: v)
        }
        // Unbounded: the surface grows with its content. The viewport height
        // stays UNKNOWN (0) for the core's conditions, as gpui's view passes
        // it (`orientation` = landscape, no height queries match).
        m.setViewport(width: c.width, height: 0)
        m.pass()
        m.pass()
        return dumpSurface(m, textComponents: textComponents)
    }

    /// The main tree's placed nodes relative to the root's frame, pre-order
    /// through `children` (paint order); `lines`/`lh` for text components
    /// (from the frame height minus the vertical padding + border, as gpui).
    static func dumpSurface(_ m: SurfaceModel, textComponents: Set<String>) -> CaseDump {
        let raw = m.surface.nodes()
        let visuals = m.surface.visuals()
        let live = raw.filter { !$0.removed }
        var ids: [UInt32: String] = [:]
        for n in live { ids[n.index] = n.id }
        let roots = live.filter { $0.parent == nil && $0.layer == 0 }
        let root = roots.first.map { m.frame(Int($0.index)) } ?? .zero
        var order: [Int] = []
        var stack = roots.map { Int($0.index) }.reversed() as [Int]
        while let i = stack.popLast() {
            order.append(i)
            stack += raw[i].children.reversed().map { Int($0) }
        }
        var out: [DumpNode] = []
        for i in order where i < raw.count {
            let n = raw[i]
            if n.removed || n.hidden || n.layer != 0 { continue }
            let f = m.frame(i)
            var node = DumpNode(id: n.id, component: n.component, part: n.part, parent: n.parent.flatMap { ids[$0] }, x: r2(f.minX - root.minX), y: r2(f.minY - root.minY), w: r2(f.width), h: r2(f.height))
            if n.isLeaf, let text = leafText(JSONValue.parse(n.propsJson).object ?? [:]) {
                if let ts = m.surface.textStyle(index: n.index), ts.lineHeight > 0, textComponents.contains(n.component) {
                    // Round 2 (layout boxes): the lines the painter lays the
                    // text out in at its frame WIDTH, as Compose's dump (the
                    // web counts its text's line boxes; a stretched box, an
                    // overlay trigger as tall as its row, is not more lines).
                    let v = i < visuals.count ? visuals[i] : nil
                    let padX = v?.padding.map { CGFloat($0[1] + $0[3]) } ?? 0
                    let borderX = v.flatMap { v in v.borderWidths.map { CGFloat($0[1] + $0[3]) } ?? v.borderWidth.map { 2 * CGFloat($0) } } ?? 0
                    let props = JSONValue.parse(n.propsJson).object ?? [:]
                    let raw = props["text"]?.string ?? props["label"]?.string ?? text
                    let clamp = n.lines.map { Int($0) }
                    let style = m.textStyle(i)
                    let h = raw.isEmpty ? 0 : TextShaper.measure(raw, style, wrap: clamp == 1 ? nil : max(1, f.width - padX - borderX), lines: clamp).height
                    node.lines = max(Int((h / CGFloat(ts.lineHeight)).rounded()), 1)
                    node.lh = r2(CGFloat(ts.lineHeight))
                }
                node.text = text
            }
            out.append(node)
        }
        return CaseDump(width: r2(root.width), height: r2(root.height), nodes: dropCollapsed(out))
    }

    static func r2(_ v: CGFloat) -> CGFloat { (v * 100).rounded() / 100 }

    /// The text a leaf lays out (`text`, `label`, `title`, `value`; numbers
    /// and booleans as JS prints them), whitespace collapsed, ≤ 80 chars.
    static func leafText(_ props: [String: JSONValue]) -> String? {
        for key in ["text", "label", "title", "value"] {
            let s: String
            switch props[key] {
            case .string(let v): s = v
            case .number(let n): s = jsString(n)
            case .bool(let b): s = b ? "true" : "false"
            default: continue
            }
            let t = s.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
            if !t.isEmpty { return String(String.UnicodeScalarView(t.unicodeScalars.prefix(80))) }
        }
        return nil
    }

    static func jsString(_ n: Double) -> String {
        if n.isNaN { return "NaN" }
        if n == n.rounded(), abs(n) < 1e21 { return String(format: "%.0f", n) }
        return "\(n)"
    }

    /// `dropCollapsed` of `dump.ts`: a node whose box is 0×0 together with
    /// every descendant's is not placed.
    static func dropCollapsed(_ nodes: [DumpNode]) -> [DumpNode] {
        var parentOf: [String: String?] = [:]
        for n in nodes { parentOf[n.id] = n.parent }
        var visible = Set<String>()
        for n in nodes where n.w > 0 || n.h > 0 {
            var id: String? = n.id
            while let i = id, !visible.contains(i) {
                visible.insert(i)
                id = parentOf[i] ?? nil
            }
        }
        return nodes.filter { visible.contains($0.id) }
    }

    static func write(dump: Dump, to url: URL) throws {
        let enc = JSONEncoder()
        enc.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        try enc.encode(dump).write(to: url)
    }

    static func writeReport(cases: [String], failed: [String]) throws {
        let env = ProcessInfo.processInfo.environment
        let path = env["EXPONENTIAL_UI_REAL_FONT_REPORT"].map { URL(fileURLWithPath: $0) } ?? repo.appendingPathComponent(".conformance/exponential-ui-swift-real-font.json")
        let report: JSONValue = .object([
            "renderer": .string(renderer),
            "platform": .string("ios"),
            "suites": .object(["real-font": .object(["cases": .number(Double(cases.count)), "passed": .number(Double(cases.count - failed.count)), "failed": .array(failed.map { .string($0) })])]),
        ])
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
        try (orderedJSON(report) + "\n").write(to: path, atomically: true, encoding: .utf8)
    }

    // MARK: - The comparator (`conformance/compare.ts`, gpui's port)

    struct Diff {
        var id: String
        var component: String
        var kinds: [String]
        var origin = false
        var line: String
    }

    struct CaseReport {
        static let countKeys = ["size", "position", "wrap", "onlyRef", "onlyCand"]
        var matched = 0
        var size = 0
        var position = 0
        var wrap = 0
        var wrapTolerated: [String] = []
        var onlyRef: [String] = []
        var onlyCand: [String] = []
        var diffs: [Diff] = []

        var counts: [String: Int] { ["size": size, "position": position, "wrap": wrap, "onlyRef": onlyRef.count, "onlyCand": onlyCand.count] }
    }

    static func fmt(_ v: CGFloat) -> String {
        v == v.rounded() ? String(Int(v)) : String(format: "%g", Double(v))
    }

    static func fmtBox(_ n: DumpNode) -> String {
        let lines = n.lines.map { " \($0)ln@\(fmt(n.lh ?? 0))" } ?? ""
        return "\(fmt(n.x)),\(fmt(n.y)) \(fmt(n.w))×\(fmt(n.h))\(lines)"
    }

    /// `compareCase`: tolerance ±px on x/y/w/h, a text node may differ by
    /// `textLines` line heights; origins = a size divergence nothing below
    /// explains, a position divergence whose parent is in tolerance.
    static func compareCase(_ ref: CaseDump, _ cand: CaseDump, px: CGFloat, textLines: CGFloat) -> CaseReport {
        var refBy: [String: DumpNode] = [:]
        var refOrder: [DumpNode] = []
        for n in ref.nodes where refBy[n.id] == nil {
            refBy[n.id] = n
            refOrder.append(n)
        }
        var candBy: [String: DumpNode] = [:]
        for n in cand.nodes where candBy[n.id] == nil { candBy[n.id] = n }
        var report = CaseReport()
        for a in refOrder {
            guard let b = candBy[a.id] else {
                report.onlyRef.append(a.id)
                continue
            }
            report.matched += 1
            let dx = r2(b.x - a.x), dy = r2(b.y - a.y), dw = r2(b.w - a.w), dh = r2(b.h - a.h)
            let hasLines = a.lines != nil || b.lines != nil
            let lh = max(a.lh ?? 0, b.lh ?? 0)
            let hTol = hasLines ? max(px, textLines * lh) : px
            var kinds: [String] = []
            if abs(dw) > px || abs(dh) > hTol {
                kinds.append("size")
            } else if abs(dx) > px || abs(dy) > px {
                kinds.append("position")
            }
            if let la = a.lines, let lb = b.lines, la != lb {
                if CGFloat(abs(la - lb)) > textLines {
                    kinds.append("wrap")
                } else {
                    report.wrapTolerated.append("\(a.id): \(la) → \(lb) lines")
                }
            }
            if kinds.isEmpty { continue }
            let text = (a.text ?? b.text).map { "  \"\(String($0.prefix(40)))\"" } ?? ""
            let line = "\(kinds.joined(separator: "+").padding(toLength: 13, withPad: " ", startingAt: 0)) \(a.id) (\(a.component)) web \(fmtBox(a)) → swift \(fmtBox(b))  Δ \(fmt(dx)),\(fmt(dy)) \(fmt(dw))×\(fmt(dh))\(text)"
            report.diffs.append(Diff(id: a.id, component: a.component, kinds: kinds, line: line))
        }
        var seenCand = Set<String>()
        for n in cand.nodes where refBy[n.id] == nil && !seenCand.contains(n.id) {
            seenCand.insert(n.id)
            report.onlyCand.append(n.id)
        }
        let diverged = Set(report.diffs.map(\.id))
        var sizeBelow = Set<String>()
        for d in report.diffs where d.kinds.contains("size") || d.kinds.contains("wrap") {
            var p = refBy[d.id]?.parent
            while let id = p {
                p = refBy[id]?.parent
                sizeBelow.insert(id)
            }
        }
        for i in report.diffs.indices {
            let d = report.diffs[i]
            let parent = refBy[d.id]?.parent
            if d.kinds.contains("size") || d.kinds.contains("wrap") {
                report.diffs[i].origin = !sizeBelow.contains(d.id)
            } else {
                report.diffs[i].origin = parent.map { !diverged.contains($0) } ?? true
            }
            if d.kinds.contains("size") { report.size += 1 }
            if d.kinds.contains("position") { report.position += 1 }
            if d.kinds.contains("wrap") { report.wrap += 1 }
        }
        return report
    }

    /// `decodeBaseline` of `conformance/baseline.ts`.
    static func decodeBaseline(_ b: JSONValue) throws -> [String: CaseDump] {
        XCTAssertEqual(b["format"]?.string, "xui-conformance-baseline/1", "baseline format")
        var out: [String: CaseDump] = [:]
        for (key, c) in b["cases"]?.object ?? [:] {
            let fixture = String(key.split(separator: "/").first ?? "")
            let table = b["fixtures"]?[fixture]?["nodes"]?.array ?? []
            let nodes: [DumpNode] = (c["frames"]?.array ?? []).compactMap { rowValue in
                guard let row = rowValue.array, let i = row.first?.number.map(Int.init), i < table.count, let n = table[i].array else { return nil }
                let f = { (k: Int) in CGFloat(row[k].number ?? 0) }
                var node = DumpNode(id: n[0].string ?? "", component: n[1].string ?? "", part: n[2].string, parent: n[3].string, x: f(1), y: f(2), w: f(3), h: f(4), text: n[4].string)
                if row.count > 5 {
                    node.lines = row[5].number.map(Int.init)
                    node.lh = row.count > 6 ? f(6) : 0
                }
                return node
            }
            out[key] = CaseDump(width: CGFloat(c["width"]?.number ?? 0), height: CGFloat(c["height"]?.number ?? 0), nodes: nodes)
        }
        return out
    }

    // MARK: - IO

    static func read(_ rel: String) throws -> JSONValue {
        try readAbsolute(repo.appendingPathComponent(rel))
    }

    static func readAbsolute(_ url: URL) throws -> JSONValue {
        let data = try Data(contentsOf: url)
        return JSONValue(any: try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]))
    }

    /// A JSON string literal (slashes unescaped, as serde/JS write them).
    static func quote(_ s: String) -> String {
        let data = try! JSONSerialization.data(withJSONObject: s, options: [.fragmentsAllowed, .withoutEscapingSlashes])
        return String(decoding: data, as: UTF8.self)
    }

    /// Pretty JSON with sorted keys, `$comment` first (the report file).
    static func orderedJSON(_ v: JSONValue, indent: String = "") -> String {
        let inner = indent + "  "
        switch v {
        case .object(let o):
            if o.isEmpty { return "{}" }
            let keys = o.keys.sorted { a, b in a == "$comment" ? b != "$comment" : (b == "$comment" ? false : a < b) }
            return "{\n" + keys.map { "\(inner)\(quote($0)): \(orderedJSON(o[$0]!, indent: inner))" }.joined(separator: ",\n") + "\n\(indent)}"
        case .array(let a):
            if a.isEmpty { return "[]" }
            return "[\n" + a.map { "\(inner)\(orderedJSON($0, indent: inner))" }.joined(separator: ",\n") + "\n\(indent)]"
        case .number(let n):
            return jsString(n)
        case .string(let s):
            return quote(s)
        default:
            return v.json
        }
    }

    // MARK: - The comparator's rules (`compare.test.ts`, gpui's port)

    static func n(_ id: String, _ parent: String?, _ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat) -> DumpNode {
        DumpNode(id: id, component: "Box", part: nil, parent: parent, x: x, y: y, w: w, h: h)
    }

    static func box(_ nodes: [DumpNode]) -> CaseDump { CaseDump(width: 400, height: 400, nodes: nodes) }

    func testCompareRulesMatchTheTSComparator() {
        typealias T = RealFontConformanceTests
        // ±1 px is in tolerance, 1.5 px is a position origin.
        let a = T.box([T.n("root", nil, 0, 0, 400, 100), T.n("a", "root", 10, 10, 50, 20)])
        XCTAssertTrue(T.compareCase(a, T.box([T.n("root", nil, 0, 0, 400, 100), T.n("a", "root", 11, 9, 51, 21)]), px: 1, textLines: 1).diffs.isEmpty)
        let r = T.compareCase(a, T.box([T.n("root", nil, 0, 0, 400, 100), T.n("a", "root", 11.5, 10, 50, 20)]), px: 1, textLines: 1)
        XCTAssertEqual(r.diffs.map { "\($0.id) \($0.kinds) \($0.origin)" }, ["a [\"position\"] true"])

        // Origins: the deepest size divergence; a sibling moved inside a diverging parent cascades.
        let a2 = T.box([T.n("root", nil, 0, 0, 400, 100), T.n("card", "root", 0, 0, 400, 60), T.n("text", "card", 0, 0, 100, 20), T.n("below", "root", 0, 60, 400, 40), T.n("belowChild", "below", 0, 60, 10, 10)])
        let b2 = T.box([T.n("root", nil, 0, 0, 400, 120), T.n("card", "root", 0, 0, 400, 80), T.n("text", "card", 0, 0, 100, 40), T.n("below", "root", 0, 80, 400, 40), T.n("belowChild", "below", 0, 80, 10, 10)])
        let r2 = T.compareCase(a2, b2, px: 1, textLines: 1)
        XCTAssertEqual(r2.diffs.map { "\($0.id):\($0.origin)" }, ["root:false", "card:false", "text:true", "below:false", "belowChild:false"])
        XCTAssertEqual(r2.size, 3)
        XCTAssertEqual(r2.position, 2)

        // Text: one more line is tolerated and reported, two is a wrap divergence.
        func t(_ lines: Int) -> CaseDump {
            var node = T.n("t", nil, 0, 0, 200, 20 * CGFloat(lines))
            node.component = "Text"
            node.lines = lines
            node.lh = 20
            return T.box([node])
        }
        let one = T.compareCase(t(2), t(3), px: 1, textLines: 1)
        XCTAssertTrue(one.diffs.isEmpty)
        XCTAssertEqual(one.wrapTolerated, ["t: 2 → 3 lines"])
        XCTAssertEqual(T.compareCase(t(2), t(4), px: 1, textLines: 1).diffs.first?.kinds, ["size", "wrap"])

        // Coverage is not a divergence.
        let cov = T.compareCase(T.box([T.n("a", nil, 0, 0, 1, 1), T.n("webOnly", nil, 0, 0, 1, 1)]), T.box([T.n("a", nil, 0, 0, 1, 1), T.n("swiftOnly", nil, 0, 0, 1, 1)]), px: 1, textLines: 1)
        XCTAssertEqual(cov.onlyRef, ["webOnly"])
        XCTAssertEqual(cov.onlyCand, ["swiftOnly"])
        XCTAssertEqual(cov.diffs.count, 0)

        // dropCollapsed: any order, 0×0 parents of placed nodes stay, thin boxes stay.
        let kept = T.dropCollapsed([T.n("leaf", "zeroParent", 0, 0, 10, 10), T.n("root", nil, 0, 0, 100, 100), T.n("zeroParent", "root", 0, 0, 0, 0), T.n("gone", "root", 5, 5, 0, 0), T.n("goneChild", "gone", 5, 5, 0, 0), T.n("rule", "root", 0, 0, 100, 0)]).map(\.id)
        XCTAssertEqual(kept, ["leaf", "root", "zeroParent", "rule"])
    }

    // MARK: - The shaper's web rules (gpui `text.rs` tests, CoreText widths)

    func testTheShaperBreaksAndHangsLikeTheWeb() throws {
        let manifest = try Self.read("packages/exponential-ui/fixtures/conformance-cases.json")
        _ = try Self.installConformanceFonts(manifest)
        let ts = TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: "Inter")
        // Trailing spaces hang; max-content = the widest hard line.
        XCTAssertEqual(TextShaper.maxContent("ab  ", ts), TextShaper.width("ab", ts))
        XCTAssertEqual(TextShaper.maxContent("a\nlonger line", ts), TextShaper.width("longer line", ts))
        // Min-content = the widest segment; a URL stays whole at its slashes.
        XCTAssertEqual(TextShaper.minContent("tiny enormousword", ts), TextShaper.width("enormousword", ts))
        XCTAssertEqual(TextShaper.segments("see exponential.at/docs/x now").map(\.text), ["see ", "exponential.at/docs/x ", "now"])
        // Greedy wrapping; a long word overflows on its own line.
        let w = TextShaper.width("aaa bbb", ts)
        XCTAssertEqual(TextShaper.lines("aaa bbb ccc", ts, wrap: w), ["aaa bbb", "ccc"])
        XCTAssertEqual(TextShaper.lines("a enormousword b", ts, wrap: 10), ["a", "enormousword", "b"])
        XCTAssertEqual(TextShaper.lines("one\ntwo", ts, wrap: nil), ["one", "two"])
        // CSS weight matching: Geist has no 500 face → 400 (CoreText's
        // nearest-trait match would give the 600 one).
        let geist500 = TextFonts.font(family: "Geist", weight: 500, size: 14)
        let geist400 = TextFonts.font(family: "Geist", weight: 400, size: 14)
        XCTAssertEqual(CTFontCopyPostScriptName(geist500) as String, CTFontCopyPostScriptName(geist400) as String)
        // Substitutes and generics map onto the set, nothing else.
        XCTAssertEqual(CTFontCopyFamilyName(TextFonts.font(family: "Nunito", weight: 400, size: 14)) as String, "Nunito")
        XCTAssertEqual(CTFontCopyFamilyName(TextFonts.font(family: "ui-monospace", weight: 400, size: 14)) as String, "JetBrains Mono")
        XCTAssertEqual(CTFontCopyFamilyName(TextFonts.font(family: nil, weight: 400, size: 14)) as String, "Inter")
        XCTAssertEqual(CTFontCopyFamilyName(TextFonts.font(family: "Comic Sans MS", weight: 400, size: 14)) as String, "Inter")
    }
}

/// A minimal order-preserving reader over raw JSON text (`JSONValue`
/// objects are unordered): the tests that need a fixture's KEY ORDER slice
/// the text instead.
enum RawJSON {
    /// The keys of the object the text starts with (after whitespace and
    /// a `:`), in file order.
    static func objectKeys(in text: String) -> [String] {
        let s = Array(text.utf8)
        var i = 0
        func ws() { while i < s.count, s[i] == 0x20 || s[i] == 0x0A || s[i] == 0x0D || s[i] == 0x09 || s[i] == 0x3A { i += 1 } }
        ws()
        guard i < s.count, s[i] == 0x7B else { return [] }
        i += 1
        var keys: [String] = []
        while i < s.count {
            ws()
            if s[i] == 0x2C { i += 1; continue }
            if s[i] == 0x7D { break }
            let start = i
            skipValue(s, &i)
            if let k = try? JSONSerialization.jsonObject(with: Data(s[start..<i]), options: [.fragmentsAllowed]) as? String { keys.append(k) }
            ws()
            skipValue(s, &i)
        }
        return keys
    }

    /// The raw text of each element of the array value the text starts
    /// with.
    static func arrayElements(in text: String) -> [String] {
        let s = Array(text.utf8)
        var i = 0
        while i < s.count, s[i] != 0x5B { i += 1 }
        guard i < s.count else { return [] }
        i += 1
        var out: [String] = []
        while i < s.count {
            while i < s.count, s[i] == 0x20 || s[i] == 0x0A || s[i] == 0x0D || s[i] == 0x09 || s[i] == 0x2C { i += 1 }
            if i >= s.count || s[i] == 0x5D { break }
            let start = i
            skipValue(s, &i)
            out.append(String(decoding: s[start..<i], as: UTF8.self))
        }
        return out
    }

    /// The raw text of `key`'s value in the object `text` is.
    static func value(of key: String, inObject text: String) -> String? {
        let s = Array(text.utf8)
        var i = 0
        func ws() { while i < s.count, s[i] == 0x20 || s[i] == 0x0A || s[i] == 0x0D || s[i] == 0x09 || s[i] == 0x2C { i += 1 } }
        while i < s.count, s[i] != 0x7B { i += 1 }
        i += 1
        while i < s.count {
            ws()
            if i >= s.count || s[i] == 0x7D { return nil }
            let ks = i
            skipValue(s, &i)
            let k = try? JSONSerialization.jsonObject(with: Data(s[ks..<i]), options: [.fragmentsAllowed]) as? String
            while i < s.count, s[i] != 0x3A { i += 1 }
            i += 1
            while i < s.count, s[i] == 0x20 || s[i] == 0x0A || s[i] == 0x0D || s[i] == 0x09 { i += 1 }
            let vs = i
            skipValue(s, &i)
            if k == key { return String(decoding: s[vs..<i], as: UTF8.self) }
        }
        return nil
    }

    /// Advance past one JSON value (string, number, literal, array, object).
    static func skipValue(_ s: [UInt8], _ i: inout Int) {
        guard i < s.count else { return }
        switch s[i] {
        case 0x22:
            i += 1
            while i < s.count, s[i] != 0x22 {
                if s[i] == 0x5C { i += 1 }
                i += 1
            }
            i += 1
        case 0x7B, 0x5B:
            var depth = 0
            while i < s.count {
                switch s[i] {
                case 0x22:
                    skipValue(s, &i)
                    continue
                case 0x7B, 0x5B: depth += 1
                case 0x7D, 0x5D:
                    depth -= 1
                    if depth == 0 { i += 1; return }
                default: break
                }
                i += 1
            }
        default:
            while i < s.count, ![0x2C, 0x7D, 0x5D, 0x20, 0x0A, 0x0D, 0x09, 0x3A].contains(s[i]) { i += 1 }
        }
    }
}

private extension String {
    func leftPad(_ n: Int) -> String { count >= n ? self : String(repeating: " ", count: n - count) + self }
}
