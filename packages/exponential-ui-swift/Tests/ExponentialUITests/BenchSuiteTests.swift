import Foundation
import XCTest
import ExponentialUICore
@testable import ExponentialUI
#if canImport(UIKit)
import UIKit
#endif

/// VAPP-105: the Exponential UI benchmark suite on the SwiftUI painter's
/// MODEL (`packages/exponential-ui-bench/scenarios/*.json`, every scenario's
/// metrics as scenarios/manifest.json names them). A pass = `SurfaceModel`'s
/// `pass()` as the setters trigger it: the core through the facade, the
/// TextKit measurer, nodes and visuals read back; no SwiftUI body. Runs
/// only under `EXPONENTIAL_UI_BENCH=1` (through xcodebuild:
/// `TEST_RUNNER_EXPONENTIAL_UI_BENCH=1`); `EXPONENTIAL_UI_BENCH_REPEAT` (3)
/// repeats every scenario in-process and keeps the median per metric
/// (cold-start's `coldStartMs` is the first, only cold, run);
/// `EXPONENTIAL_UI_BENCH_SCENARIO=<id>,<id>` runs only those. The result
/// fragment goes to `EXPONENTIAL_UI_BENCH_OUT` when set and writable, and
/// ALWAYS to stdout as one line `EXPONENTIAL_UI_BENCH_FRAGMENT <json>`
/// (`packages/exponential-ui-bench/runners/swift.ts` and `swift-ios.ts`
/// parse it). Scenario files: the repo's (macOS, the Simulator), else the
/// test bundle's `Resources/bench/` (a real device; the runner copies them).
@MainActor
final class BenchSuiteTests: XCTestCase {
    private static let env = ProcessInfo.processInfo.environment
    private static func flag(_ name: String) -> String? { env[name] ?? env["TEST_RUNNER_\(name)"] }

    func testBenchSuite() throws {
        guard Self.flag("EXPONENTIAL_UI_BENCH") == "1" else { throw XCTSkip("EXPONENTIAL_UI_BENCH=1 runs the benchmark suite") }
        let dir = try Self.scenarioDir()
        let manifest = try Self.read(dir.appendingPathComponent("manifest.json"))
        let only = Set((Self.flag("EXPONENTIAL_UI_BENCH_SCENARIO") ?? "").split(separator: ",").map(String.init))
        let ids = (manifest["scenarios"]?.array ?? []).compactMap { $0["id"]?.string }.filter { only.isEmpty || only.contains($0) }
        XCTAssertFalse(ids.isEmpty, "scenarios/manifest.json lists scenarios")
        let repeats = max(1, Int(Self.flag("EXPONENTIAL_UI_BENCH_REPEAT") ?? "3") ?? 3)

        var scenarios: [String: [String: Double]] = [:]
        for id in ids {
            let scenario = try Self.read(dir.appendingPathComponent("\(id).json"))
            var samples: [[String: Double]] = []
            for _ in 0..<repeats { samples.append(try run(scenario)) }
            var merged: [String: Double] = [:]
            for key in Set(samples.flatMap(\.keys)) {
                // The cold start is cold once: keep the first sample. Memory: the first run's footprint too.
                let values = samples.compactMap { $0[key] }
                merged[key] = (key == "coldStartMs" || key == "memoryMb") ? values[0] : Self.median(values)
            }
            // Counts round to whole numbers, timings to 3 decimals.
            scenarios[id] = merged.mapValues { v in v == v.rounded() ? v : (v * 1000).rounded() / 1000 }
            print("bench \(id): \(merged.keys.sorted().map { "\($0) \(String(format: "%.3f", merged[$0]!))" }.joined(separator: ", "))")
        }

        let fragment = Self.fragment(scenarios: scenarios)
        let json = JSONValue(any: fragment).json
        print("EXPONENTIAL_UI_BENCH_FRAGMENT \(json)")
        if let out = Self.flag("EXPONENTIAL_UI_BENCH_OUT") {
            let url = URL(fileURLWithPath: out)
            try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? (json + "\n").write(to: url, atomically: true, encoding: .utf8)
        }
    }

    // MARK: - One scenario

    private func run(_ s: JSONValue) throws -> [String: Double] {
        let id = s["id"]!.string!
        let vw = CGFloat(s["viewport"]!["width"]!.number!)
        let vh = CGFloat(s["viewport"]!["height"]!.number!)
        let steps = s["steps"] ?? .object([:])
        let metrics = Set((s["metrics"]?.array ?? []).compactMap(\.string))
        var out: [String: Double] = [:]

        var options = SurfaceOptions()
        options.catalogId = s["catalogId"]?.string ?? coreCatalogId()
        options.theme = ThemeHandle.builtin(s["theme"]?.string ?? "neutral")
        options.mode = s["mode"]?.string == "dark" ? .dark : .light

        /// A fresh model with the scenario's surface and data, painted once (the first pass). Returns the model and its first paint cost.
        func firstPaint(timeInit: Bool) throws -> (SurfaceModel, Double) {
            let t0 = ContinuousClock.now
            let m = try SurfaceModel(id: "bench", options: options, host: NoHost())
            let t1 = timeInit ? t0 : ContinuousClock.now
            if let nested = s["surface"]?["nested"] {
                _ = try m.setNested(json: nested.json)
            } else if let components = s["surface"]?["components"] {
                _ = try m.setComponents(json: components.json)
            }
            if let data = s["data"] {
                if let gen = data["generate"] {
                    m.setData(path: gen["path"]!.string!, value: Self.generate(gen))
                } else {
                    m.setData(path: "", value: data)
                }
            }
            m.setViewport(width: vw, height: vh)
            return (m, Self.ms(ContinuousClock.now - t1))
        }
        func alive(_ m: SurfaceModel) -> Int { m.nodes.lazy.filter { !$0.removed && !$0.hidden }.count }
        func timed(_ body: () -> Void) -> Double {
            let t = ContinuousClock.now
            body()
            return Self.ms(ContinuousClock.now - t)
        }
        /// `widths` alternating × `passes`: the median pass, the max upcalls / measure calls.
        func widthPasses(_ m: SurfaceModel, _ prefix: String) {
            let widths = (steps["widths"]?.array ?? []).compactMap { $0.number }.map { CGFloat($0) }
            let passes = Int(steps["passes"]?.number ?? 100)
            var times: [Double] = []
            var upcalls = 0
            var measures = 0
            for p in 0..<passes {
                let w = widths[p % max(1, widths.count)]
                times.append(timed { m.setViewport(width: w, height: vh) })
                upcalls = max(upcalls, m.stats.upcalls)
                measures = max(measures, m.stats.measureCalls)
            }
            out[prefix] = Self.median(times)
            if metrics.contains("upcallsPerPass") { out["upcallsPerPass"] = Double(upcalls) }
            if metrics.contains("upcallsPerResize") { out["upcallsPerResize"] = Double(upcalls) }
            if metrics.contains("measureCallsPerPass") { out["measureCallsPerPass"] = Double(measures) }
        }

        switch id {
        case "cold-start":
            // The first model of the process: the facade's catalog, macros and themes parse here.
            let (cold, coldMs) = try firstPaint(timeInit: true)
            out["coldStartMs"] = Self.coldStartMs ?? coldMs
            Self.coldStartMs = Self.coldStartMs ?? coldMs
            out["nodes"] = Double(alive(cold))
            let (_, warm) = try firstPaint(timeInit: false)
            out["firstPaintMs"] = warm
            out["memoryMb"] = Self.footprintMb()

        case "card-grid", "deep-nesting":
            let (m, first) = try firstPaint(timeInit: false)
            if metrics.contains("firstPaintMs") { out["firstPaintMs"] = first }
            widthPasses(m, "warmPassMs")
            if metrics.contains("depth") { out["depth"] = Double((m.nodes.lazy.filter { !$0.removed }.map(\.depth).max() ?? -1) + 1) }

        case "resize":
            let (m, _) = try firstPaint(timeInit: false)
            widthPasses(m, "resizeMs")

        case "list-100k":
            let (m, first) = try firstPaint(timeInit: false)
            out["firstPaintMs"] = first
            let count = Int(steps["scroll"]?["count"]?.number ?? 100)
            var measures = 0
            SurfaceModel.benchCounters = BenchCounters()
            let t1 = ContinuousClock.now
            for i in 1...count {
                m.scrollTo(id: "root", offset: CGPoint(x: 0, y: CGFloat(i) * vh))
                measures += m.stats.measureCalls
            }
            out["scrollStepMs"] = Self.ms(ContinuousClock.now - t1) / Double(count)
            if let c = SurfaceModel.benchCounters {
                out["ffiCallsPerStep"] = (Double(c.calls) / Double(count)).rounded()
                out["ffiBytesPerStep"] = (Double(c.bytesIn + c.bytesOut) / Double(count)).rounded()
            }
            SurfaceModel.benchCounters = nil
            out["measureCallsPerStep"] = (Double(measures) / Double(count)).rounded()
            let index = Int(steps["scrollToIndex"]?["index"]?.number ?? 50000)
            let align = steps["scrollToIndex"]?["align"]?.string ?? "start"
            out["scrollToIndexMs"] = timed { m.scrollToIndex(id: "root", index: index, align: align) }
            let list = m.lists["root"]
            out["renderedItems"] = Double((list?.end ?? 0) - (list?.start ?? 0))
            XCTAssertNotNil(m.index(of: "bench-row.r\(index)"), "item \(index) rendered after the jump")
            out["memoryMb"] = Self.footprintMb()

        case "streaming":
            let (m, _) = try firstPaint(timeInit: false)
            let messages = (steps["messages"]?.array ?? []).map(\.json)
            SurfaceModel.benchCounters = BenchCounters()
            var times: [Double] = []
            times.reserveCapacity(messages.count)
            for message in messages {
                times.append(timed { _ = try? m.apply(json: message) })
            }
            if let c = SurfaceModel.benchCounters { out["ffiBytesTotal"] = Double(c.bytesIn + c.bytesOut) }
            SurfaceModel.benchCounters = nil
            out["applyMs"] = times.reduce(0, +) / Double(max(1, times.count))
            out["p95ApplyMs"] = Self.percentile(times, 0.95)
            out["totalMs"] = times.reduce(0, +)
            out["nodes"] = Double(alive(m))

        case "theme-switch":
            let (m, _) = try firstPaint(timeInit: false)
            let themes = (steps["themes"]?.array ?? []).compactMap(\.string)
            let cycles = Int(steps["cycles"]?.number ?? 10)
            var times: [Double] = []
            var restyled = 0
            for _ in 0..<cycles {
                for theme in themes {
                    guard let handle = ThemeHandle.builtin(theme) else { continue }
                    times.append(timed { m.setTheme(handle) })
                    restyled = max(restyled, m.stats.restyled)
                }
            }
            out["themeSwitchMs"] = Self.median(times)
            if metrics.contains("restyled") { out["restyled"] = Double(restyled) }
            let modes = (steps["modes"]?.array ?? []).compactMap(\.string)
            let modeCycles = Int(steps["modeCycles"]?.number ?? 10)
            var modeTimes: [Double] = []
            for _ in 0..<modeCycles {
                for mode in modes {
                    modeTimes.append(timed { m.setMode(mode == "dark" ? .dark : .light) })
                }
            }
            out["modeSwitchMs"] = Self.median(modeTimes)

        case "template-10k":
            let (m, first) = try firstPaint(timeInit: false)
            out["firstPaintMs"] = first
            let rebind = steps["rebind"]!
            let items = Self.generate(rebind)
            out["rebindMs"] = timed { m.setData(path: rebind["path"]!.string!, value: items) }
            out["nodes"] = Double(alive(m))

        case "overlay":
            let (m, _) = try firstPaint(timeInit: false)
            let path = steps["path"]?.string ?? "/ui/open"
            let toggles = Int(steps["toggles"]?.number ?? 50)
            var opens: [Double] = []
            var closes: [Double] = []
            for _ in 0..<toggles {
                opens.append(timed { m.setData(path: path, value: .bool(true)) })
                closes.append(timed { m.setData(path: path, value: .bool(false)) })
            }
            out["openMs"] = Self.median(opens)
            out["closeMs"] = Self.median(closes)

        case "table-1k":
            let (m, first) = try firstPaint(timeInit: false)
            out["firstPaintMs"] = first
            // The Table's list record is keyed by the Table's id; its scroller is the node the core reports a scroll for.
            let scroller = m.scrolls.keys.compactMap { m.node($0)?.id }.first { $0.hasPrefix("table") } ?? "table"
            let count = Int(steps["scroll"]?["count"]?.number ?? 20)
            let t1 = ContinuousClock.now
            for i in 1...count { m.scrollTo(id: scroller, offset: CGPoint(x: 0, y: CGFloat(i) * vh)) }
            out["scrollStepMs"] = Self.ms(ContinuousClock.now - t1) / Double(count)
            let list = m.lists.values.first { $0.id.hasPrefix("table") }
            out["renderedRows"] = Double((list?.end ?? 0) - (list?.start ?? 0))
            let sort = steps["sort"]!
            out["sortMs"] = timed { _ = try? m.apply(json: sort.json) }

        default:
            XCTFail("unknown scenario \(id)")
        }
        return out
    }

    // MARK: - Support

    private static var coldStartMs: Double?

    /// `packages/exponential-ui-bench/scenarios` beside the shared fixtures (macOS, the Simulator), else the bundle's copy (a device).
    private static func scenarioDir() throws -> URL {
        let repo = Fixtures.dir.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("exponential-ui-bench/scenarios")
        if FileManager.default.fileExists(atPath: repo.appendingPathComponent("manifest.json").path) { return repo }
        let bundled = Bundle.module.resourceURL!.appendingPathComponent("Resources/bench")
        if FileManager.default.fileExists(atPath: bundled.appendingPathComponent("manifest.json").path) { return bundled }
        throw XCTSkip("no benchmark scenarios: neither \(repo.path) nor the bundle's Resources/bench")
    }

    private static func read(_ url: URL) throws -> JSONValue {
        JSONValue(any: try JSONSerialization.jsonObject(with: try Data(contentsOf: url), options: [.fragmentsAllowed]))
    }

    /// `generateItems` of src/scenarios.ts: every field a STRING; `{i}`, `{i % N}`.
    /// Each template is cut into its pieces once (VAPP-131: a regex per
    /// item and field cost the 100k list's first paint ~150 ms of harness).
    static func generate(_ spec: JSONValue) -> JSONValue {
        enum Piece { case text(String), index, modulo(Int) }
        let count = Int(spec["count"]?.number ?? 0)
        let item = spec["item"]?.object ?? [:]
        let pattern = try! NSRegularExpression(pattern: "\\{i(?: % (\\d+))?\\}")
        let fields: [(String, [Piece])] = item.keys.sorted().map { key in
            let template = item[key]!.string ?? ""
            let ns = template as NSString
            var pieces: [Piece] = []
            var at = 0
            for m in pattern.matches(in: template, range: NSRange(location: 0, length: ns.length)) {
                if m.range.location > at { pieces.append(.text(ns.substring(with: NSRange(location: at, length: m.range.location - at)))) }
                pieces.append(m.range(at: 1).location == NSNotFound ? .index : .modulo(Int(ns.substring(with: m.range(at: 1))) ?? 1))
                at = m.range.location + m.range.length
            }
            if at < ns.length { pieces.append(.text(ns.substring(from: at))) }
            return (key, pieces)
        }
        var items: [JSONValue] = []
        items.reserveCapacity(count)
        for i in 0..<count {
            var o: [String: JSONValue] = [:]
            o.reserveCapacity(fields.count)
            for (key, pieces) in fields {
                var text = ""
                for p in pieces {
                    switch p {
                    case let .text(t): text += t
                    case .index: text += String(i)
                    case let .modulo(n): text += String(i % n)
                    }
                }
                o[key] = .string(text)
            }
            items.append(.object(o))
        }
        return .array(items)
    }

    private static func fragment(scenarios: [String: [String: Double]]) -> [String: Any] {
        var renderer = "swift-macos"
        var platform = "macos"
        var device = ""
        #if os(iOS)
        platform = "ios"
        #if targetEnvironment(simulator)
        renderer = "swift-ios-simulator"
        device = "\(UIDevice.current.name) Simulator (\(env["SIMULATOR_MODEL_IDENTIFIER"] ?? UIDevice.current.model), iOS \(UIDevice.current.systemVersion))"
        #else
        renderer = "swift-ios-device"
        device = "\(UIDevice.current.name) (\(sysctl("hw.machine")), iOS \(UIDevice.current.systemVersion))"
        #endif
        #else
        device = "\(sysctl("hw.model")) (\(sysctl("machdep.cpu.brand_string")), macOS \(ProcessInfo.processInfo.operatingSystemVersionString))"
        #endif
        let iso = ISO8601DateFormatter().string(from: Date())
        return ["renderer": renderer, "platform": platform, "device": device, "date": iso, "scenarios": scenarios]
    }

    private static func sysctl(_ name: String) -> String {
        var size = 0
        sysctlbyname(name, nil, &size, nil, 0)
        guard size > 0 else { return "" }
        var buffer = [CChar](repeating: 0, count: size)
        sysctlbyname(name, &buffer, &size, nil, 0)
        return String(cString: buffer)
    }

    /// The process's physical footprint (what Xcode's memory gauge shows), MB.
    private static func footprintMb() -> Double {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count) }
        }
        return result == KERN_SUCCESS ? (Double(info.phys_footprint) / 1_048_576 * 10).rounded() / 10 : 0
    }

    private static func ms(_ d: Duration) -> Double {
        Double(d.components.seconds) * 1000 + Double(d.components.attoseconds) / 1e15
    }

    private static func median(_ v: [Double]) -> Double {
        guard !v.isEmpty else { return 0 }
        let s = v.sorted()
        return s.count % 2 == 1 ? s[s.count / 2] : (s[s.count / 2 - 1] + s[s.count / 2]) / 2
    }

    private static func percentile(_ v: [Double], _ p: Double) -> Double {
        guard !v.isEmpty else { return 0 }
        let s = v.sorted()
        return s[min(s.count - 1, Int((Double(s.count - 1) * p).rounded()))]
    }
}
