import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// One snapshot per catalog component: the painted tree (id, component,
/// part, frame, box visual, ink) of every `catalog-components.json` case
/// under the exponential theme at 390 px with the core's FIXED measure, so
/// the file is byte-stable across machines. `__Snapshots__/components.json`
/// is the lock; `EXPONENTIAL_UI_RECORD=1 swift test` rewrites it.
@MainActor
final class SnapshotTests: XCTestCase {
    static let path = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("__Snapshots__/components.json")

    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    func testComponentSnapshots() throws {
        let cases = try Fixtures.json("catalog-components.json")["cases"]?.array ?? []
        var snapshot: [String: JSONValue] = [:]
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let m = try makeModel("snap", fixed: true)
            try m.setNested(json: c["node"]!.json)
            var rows: [JSONValue] = []
            for n in m.nodes {
                let f = m.frame(n.index)
                let s = m.boxStyle(n.index)
                var row: [String: JSONValue] = [
                    "id": .string(n.id),
                    "component": .string(n.component),
                    "frame": .array([f.minX, f.minY, f.width, f.height].map { .number(Double(($0 * 100).rounded() / 100)) }),
                ]
                if let p = n.part { row["part"] = .string(p) }
                if n.hidden { row["hidden"] = .bool(true) }
                if let bg = m.style(n.index).backgroundHex { row["bg"] = .string(bg) }
                if s.borderWidth > 0 { row["border"] = .number(Double(s.borderWidth)) }
                if s.radius > 0 { row["radius"] = .number(Double(s.radius)) }
                if let o = s.opacity { row["opacity"] = .number(o) }
                if !n.partStates.isEmpty { row["states"] = .array(n.partStates.map { .string($0) }) }
                rows.append(.object(row))
            }
            snapshot[name] = .object(["height": .number(Double(m.surfaceSize.height)), "nodes": .array(rows)])
        }
        let data = try JSONSerialization.data(withJSONObject: JSONValue.object(snapshot).any, options: [.prettyPrinted, .sortedKeys])
        let text = String(decoding: data, as: UTF8.self) + "\n"
        if ProcessInfo.processInfo.environment["EXPONENTIAL_UI_RECORD"] == "1" || !FileManager.default.fileExists(atPath: Self.path.path) {
            try FileManager.default.createDirectory(at: Self.path.deletingLastPathComponent(), withIntermediateDirectories: true)
            try text.write(to: Self.path, atomically: true, encoding: .utf8)
            if ProcessInfo.processInfo.environment["EXPONENTIAL_UI_RECORD"] == "1" { return }
        }
        let stored = try String(contentsOf: Self.path, encoding: .utf8)
        if stored != text {
            let tmp = FileManager.default.temporaryDirectory.appendingPathComponent("components.snapshot.json")
            try text.write(to: tmp, atomically: true, encoding: .utf8)
            XCTFail("component snapshot drifted; compare \(tmp.path) with \(Self.path.path) (EXPONENTIAL_UI_RECORD=1 swift test rewrites it)")
        }
    }
}

extension PaintStyle {
    /// The background as the core gave it (hex), for snapshots.
    var backgroundHex: String? {
        guard let bg = background else { return nil }
        #if canImport(UIKit)
        let c = UIColor(bg)
        #else
        let c = NSColor(bg).usingColorSpace(.sRGB) ?? NSColor(bg)
        #endif
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        c.getRed(&r, green: &g, blue: &b, alpha: &a)
        let hex = String(format: "#%02x%02x%02x", Int((r * 255).rounded()), Int((g * 255).rounded()), Int((b * 255).rounded()))
        return a < 0.999 ? hex + String(format: "%02x", Int((a * 255).rounded())) : hex
    }
}
