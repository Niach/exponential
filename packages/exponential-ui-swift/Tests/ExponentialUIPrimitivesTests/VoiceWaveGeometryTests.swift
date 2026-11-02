import XCTest
@testable import ExponentialUIPrimitives

/// VAPP-160: the pure-Swift rings (`VoiceWaveGeometry`) hold the catalog's
/// constants and replay fixtures/voice-wave.json `cases`.
final class VoiceWaveGeometryTests: XCTestCase {
    private static let packages = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()  // Tests/ExponentialUIPrimitivesTests/
        .deletingLastPathComponent()  // Tests/
        .deletingLastPathComponent()  // packages/exponential-ui-swift/
        .deletingLastPathComponent()  // packages/

    private func json(_ path: String) throws -> [String: Any] {
        let url = Self.packages.appendingPathComponent(path)
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func doubles(_ v: Any?) -> [Double]? { (v as? [NSNumber])?.map(\.doubleValue) }

    func testConstantsMatchStyleJson() throws {
        let v = try XCTUnwrap(try json("exponential-ui/catalog/style.json")["voiceWave"] as? [String: Any])
        typealias C = VoiceWaveGeometry.Constants
        XCTAssertEqual((v["steps"] as? NSNumber)?.intValue, C.steps)
        XCTAssertEqual(doubles(v["radius"]), C.radius)
        XCTAssertEqual((v["harmonics"] as? [Any])?.compactMap(doubles), C.harmonics)
        XCTAssertEqual(doubles(v["phase0"]), C.phase0)
        XCTAssertEqual(doubles(v["ringAmp"]), C.ringAmp)
        XCTAssertEqual((v["ampBase"] as? NSNumber)?.doubleValue, C.ampBase)
        XCTAssertEqual((v["ampLevel"] as? NSNumber)?.doubleValue, C.ampLevel)
        XCTAssertEqual((v["thinkingAmp"] as? NSNumber)?.doubleValue, C.thinkingAmp)
        XCTAssertEqual((v["phaseSpeed"] as? NSNumber)?.doubleValue, C.phaseSpeed)
        XCTAssertEqual(doubles(v["fillAlpha"]), C.fillAlpha)
        XCTAssertEqual(doubles(v["strokeAlpha"]), C.strokeAlpha)
        XCTAssertEqual(doubles(v["strokeWidth"]), C.strokeWidth)
        XCTAssertEqual((v["childScale"] as? NSNumber)?.doubleValue, C.childScale)
        XCTAssertEqual((v["childScaleLarge"] as? NSNumber)?.doubleValue, C.childScaleLarge)
        XCTAssertEqual((v["maxBands"] as? NSNumber)?.intValue, C.maxBands)
        XCTAssertEqual(VoiceWaveGeometry.parts.count, C.radius.count)
    }

    func testReplaysTheFixtureCases() throws {
        let cases = try XCTUnwrap(try json("exponential-ui/fixtures/voice-wave.json")["cases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 14)
        var worst = 0.0
        for c in cases {
            let name = c["name"] as? String ?? "?"
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let expected = try XCTUnwrap(c["expected"] as? [String: Any])
            let geometry = VoiceWaveGeometry(size: CGFloat((input["size"] as? NSNumber)?.doubleValue ?? 0))
            let frame = geometry.frame(
                activity: input["activity"] as? String ?? "",
                level: (input["level"] as? NSNumber)?.doubleValue ?? 0,
                bands: doubles(input["bands"]),
                elapsed: (input["elapsed"] as? NSNumber)?.doubleValue ?? 0,
                reducedMotion: input["reducedMotion"] as? Bool ?? false
            )
            XCTAssertEqual(frame.phase, (expected["phase"] as? NSNumber)?.doubleValue ?? -1, accuracy: 1e-9, name)
            let rings = try XCTUnwrap(expected["rings"] as? [[String: Any]])
            XCTAssertEqual(frame.rings.count, rings.count, name)
            for (ring, want) in zip(frame.rings, rings) {
                XCTAssertEqual(ring.part, want["part"] as? String, name)
                XCTAssertEqual(ring.fillAlpha, (want["fillAlpha"] as? NSNumber)?.doubleValue, name)
                XCTAssertEqual(ring.strokeAlpha, (want["strokeAlpha"] as? NSNumber)?.doubleValue, name)
                XCTAssertEqual(Double(ring.strokeWidth), (want["strokeWidth"] as? NSNumber)?.doubleValue, name)
                let points = try XCTUnwrap(want["points"] as? [[NSNumber]])
                XCTAssertEqual(ring.points.count, points.count, "\(name) \(ring.part)")
                for (p, q) in zip(ring.points, points) {
                    let d = max(abs(Double(p.x) - q[0].doubleValue), abs(Double(p.y) - q[1].doubleValue))
                    worst = max(worst, d)
                    XCTAssertLessThanOrEqual(d, 0.011, "\(name) \(ring.part) \(p) vs \(q)")
                }
            }
        }
        // Same math, same order of operations: every point is the core's to the 0.01 grid.
        XCTAssertLessThanOrEqual(worst, 1e-9, "worst point delta \(worst)")
    }

    func testRound2IsJsMathRound() {
        XCTAssertEqual(VoiceWaveGeometry.round2(-0.125), -0.12)
        XCTAssertEqual(VoiceWaveGeometry.round2(0.125), 0.13)
        XCTAssertEqual(VoiceWaveGeometry.round2(28), 28)
    }

    func testRulesAndCache() {
        let g = VoiceWaveGeometry(size: 56)
        // Unknown activity = idle; non-finite inputs = 0; idle lies flat at phase 0.
        let idle = g.frame(activity: "idle", level: 0, bands: nil, elapsed: 0, reducedMotion: false)
        XCTAssertEqual(g.frame(activity: "shouting", level: .nan, bands: [.infinity], elapsed: .nan, reducedMotion: false), idle)
        XCTAssertEqual(VoiceWaveGeometry.phase(activity: "listening", elapsed: 2, reducedMotion: true), 0)
        XCTAssertEqual(VoiceWaveGeometry.phase(activity: "muted", elapsed: 2, reducedMotion: false), 0)
        XCTAssertEqual(VoiceWaveGeometry.phase(activity: "speaking", elapsed: -1, reducedMotion: false), 0)
        // Only the first maxBands bands count.
        let eight = [0.8, 0.5, 1, 0.6, 0.2, 0.9, 0.4, 0.7]
        XCTAssertEqual(
            g.frame(activity: "speaking", level: 0.5, bands: eight + [0.1, 0.3], elapsed: 1, reducedMotion: false),
            g.frame(activity: "speaking", level: 0.5, bands: eight, elapsed: 1, reducedMotion: false)
        )
        let cache = VoiceWaveGeometryCache()
        XCTAssertEqual(cache.geometry(size: 56).size, 56)
        XCTAssertEqual(cache.geometry(size: 160).size, 160)
        XCTAssertEqual(cache.geometry(size: -4).size, 0)
    }
}
