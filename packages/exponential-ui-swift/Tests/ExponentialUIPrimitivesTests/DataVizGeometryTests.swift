import XCTest
@testable import ExponentialUIPrimitives

/// Group D: fixtures/data-viz.json replays through `DataViz`.
final class DataVizGeometryTests: XCTestCase {
    private static let packages = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()  // Tests/ExponentialUIPrimitivesTests/
        .deletingLastPathComponent()  // Tests/
        .deletingLastPathComponent()  // packages/exponential-ui-swift/
        .deletingLastPathComponent()  // packages/

    /// A JSON number (never a bool), else nil.
    private func num(_ v: Any?) -> Double? {
        guard let n = v as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID() else { return nil }
        return n.doubleValue
    }

    func testFixtureReplays() throws {
        let url = Self.packages.appendingPathComponent("exponential-ui/fixtures/data-viz.json")
        let root = try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
        let w = try XCTUnwrap(root["waveform"] as? [String: Any])
        XCTAssertEqual([num(w["barWidth"]), num(w["gap"]), num(w["minHeight"]), num(w["height"])], [DataViz.barWidth, DataViz.gap, DataViz.minHeight, DataViz.height])
        for c in try XCTUnwrap(root["rings"] as? [[String: Any]]) {
            let i = try XCTUnwrap(c["input"] as? [String: Any])
            let rings = (i["rings"] as? [[String: Any]])?.map { DataViz.RingSpec(value: num($0["value"]), max: num($0["max"]), tone: $0["tone"] as? String) }
            let got = DataViz.ringArcs(size: num(i["size"])!, stroke: num(i["stroke"])!, value: num(i["value"]), tone: i["tone"] as? String, sweep: num(i["sweep"]), rings: rings)
            let e = try XCTUnwrap(c["expected"] as? [String: Any])
            let expected = DataViz.RingArcs(start: num(e["start"])!, sweep: num(e["sweep"])!, rings: (e["rings"] as! [[String: Any]]).map { DataViz.RingArc(r: num($0["r"])!, fraction: num($0["fraction"])!, tone: $0["tone"] as! String) })
            XCTAssertEqual(got, expected, c["name"] as? String ?? "")
        }
        for c in try XCTUnwrap(root["bars"] as? [[String: Any]]) {
            let i = try XCTUnwrap(c["input"] as? [String: Any])
            let got = DataViz.waveformBars(width: num(i["width"])!, peaks: (i["peaks"] as? [Any] ?? []).map(num), progress: num(i["progress"]))
            let expected = (c["expected"] as! [[String: Any]]).map { DataViz.WaveBar(x: num($0["x"])!, y: num($0["y"])!, width: num($0["width"])!, height: num($0["height"])!, played: $0["played"] as! Bool) }
            XCTAssertEqual(got, expected, c["name"] as? String ?? "")
        }
        for c in try XCTUnwrap(root["seek"] as? [[String: Any]]) {
            XCTAssertEqual(DataViz.seekFraction(x: num(c["x"])!, width: num(c["width"])!), num(c["expected"])!)
        }
    }
}
