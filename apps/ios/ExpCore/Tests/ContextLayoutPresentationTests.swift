import Foundation
import XCTest
@testable import ExpCore

// EXP-1051: the context-window bar + legend, locked ×4 (web
// `context-layout.test.ts`, desktop `ui::context_layout`, Android
// `ContextLayoutPresentationTest`) against the ONE contract fixture — same
// cases, same names. Porting a client means making this file's fixture pass,
// nothing else.
final class ContextLayoutPresentationTests: XCTestCase {
    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func fixtureCases() throws -> [[String: Any]] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/context-layout.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [[String: Any]])
    }

    private func usage(_ raw: Any?) -> AgentSessionUsage? {
        guard let object = raw as? [String: Any],
              let used = object["contextUsed"] as? Int,
              let size = object["contextSize"] as? Int else { return nil }
        return AgentSessionUsage(contextUsed: used, contextSize: size)
    }

    private func segments(_ raw: Any?) -> [ContextSegment]? {
        guard let rows = raw as? [[String: Any]] else { return nil }
        return rows.map { row in
            ContextSegment(
                key: row["key"] as? String ?? "",
                tokens: row["tokens"] as? Int ?? 0,
                source: row["source"] as? String ?? "estimated",
                detail: row["detail"] as? String
            )
        }
    }

    func testEveryFixtureCaseFoldsExactly() throws {
        let cases = try fixtureCases()
        XCTAssertGreaterThanOrEqual(cases.count, 12)
        for entry in cases {
            let name = try XCTUnwrap(entry["name"] as? String)
            let folded = ContextLayoutPresentation.contextWindowView(
                usage: usage(entry["usage"]), segments: segments(entry["segments"])
            )
            guard let expected = entry["expected"] as? [String: Any] else {
                // A JSON null expectation: no window to draw at all.
                XCTAssertNil(folded, name)
                continue
            }
            let view = try XCTUnwrap(folded, name)
            XCTAssertEqual(view.headline, expected["headline"] as? String, name)
            XCTAssertEqual(view.percent, expected["percent"] as? Int, name)
            XCTAssertEqual(view.severity.rawValue, expected["severity"] as? String, name)
            XCTAssertEqual(view.ticks, expected["ticks"] as? [Int], name)

            let bar = try XCTUnwrap(expected["bar"] as? [[String: Any]], name)
            XCTAssertEqual(view.bar.count, bar.count, name)
            for (slice, row) in zip(view.bar, bar) {
                let label = "\(name) · \(slice.key)"
                XCTAssertEqual(slice.key, row["key"] as? String, label)
                XCTAssertEqual(slice.tone, row["tone"] as? String, label)
                let percent = try XCTUnwrap((row["percent"] as? NSNumber)?.doubleValue, label)
                XCTAssertEqual(slice.percent, percent, accuracy: 0.005, label)
            }

            let legend = try XCTUnwrap(expected["legend"] as? [[String: Any]], name)
            XCTAssertEqual(view.legend.count, legend.count, name)
            for (drawn, row) in zip(view.legend, legend) {
                let label = "\(name) · \(drawn.key)"
                XCTAssertEqual(drawn.key, row["key"] as? String, label)
                XCTAssertEqual(drawn.label, row["label"] as? String, label)
                XCTAssertEqual(drawn.tone, row["tone"] as? String, label)
                XCTAssertEqual(drawn.tokens, row["tokens"] as? String, label)
                XCTAssertEqual(drawn.percent, row["percent"] as? String, label)
                XCTAssertEqual(drawn.estimated, row["estimated"] as? Bool, label)
                XCTAssertEqual(drawn.detail, row["detail"] as? String, label)
            }
        }
    }

    func testTheFixtureCoversTheRulesAPortHasToGetRight() throws {
        let cases = try fixtureCases()
        let names = try cases.map { try XCTUnwrap($0["name"] as? String) }
        XCTAssertEqual(names.count, Set(names).count)
        let views = cases.map { entry in
            ContextLayoutPresentation.contextWindowView(
                usage: usage(entry["usage"]), segments: segments(entry["segments"])
            )
        }
        // A nil view (no usage / a zero window) and a real one both appear.
        XCTAssertTrue(views.contains { $0 == nil })
        XCTAssertTrue(views.contains { $0 != nil })
        // Every severity.
        for level in [AgentUsageSeverity.normal, .warning, .danger] {
            XCTAssertTrue(views.contains { $0?.severity == level }, level.rawValue)
        }
        // Every contract segment key draws in at least one case, so a client
        // that forgets a label fails here rather than in someone's sidebar.
        for key in DomainContract.contextLayoutSegmentKeys {
            let drawn = views.contains { view in
                view?.bar.contains { slice in slice.key == key } == true
            }
            XCTAssertTrue(drawn, key)
        }
        // The derived rows ride EVERY non-nil legend — they are computed,
        // never received, so they can never be missing.
        for view in views.compactMap({ $0 }) {
            XCTAssertEqual(view.legend.suffix(2).map(\.key), ["conversation", "free"])
            // The conversation is the bar's last slice, always; free is the
            // track.
            XCTAssertEqual(view.bar.last?.key, "conversation")
            XCTAssertFalse(view.bar.contains { $0.key == "free" })
        }
    }

    func testNeverRescalesAnOvershootingLayout() throws {
        // The device's estimates may add past the measured `contextUsed`; the
        // renderer clips at 100% rather than shrinking the layers to fit, so
        // the numbers a reader compares stay the numbers the device reported.
        let view = try XCTUnwrap(ContextLayoutPresentation.contextWindowView(
            usage: AgentSessionUsage(contextUsed: 20_000, contextSize: 200_000),
            segments: [
                ContextSegment(key: "base", tokens: 21_000, source: "measured"),
                ContextSegment(key: "tools", tokens: 2_400, source: "estimated"),
            ]
        ))
        XCTAssertEqual(view.bar.reduce(0) { $0 + $1.percent }, 11.7, accuracy: 0.0001)
        XCTAssertEqual(view.bar.first { $0.key == "base" }?.percent, 10.5)
    }

    func testTokensCompactDropsATrailingZeroAndKeepsSmallCountsRaw() {
        XCTAssertEqual(ContextLayoutPresentation.tokensCompact(0), "0")
        XCTAssertEqual(ContextLayoutPresentation.tokensCompact(600), "600")
        XCTAssertEqual(ContextLayoutPresentation.tokensCompact(999), "999")
        XCTAssertEqual(ContextLayoutPresentation.tokensCompact(1_000), "1k")
        XCTAssertEqual(ContextLayoutPresentation.tokensCompact(1_500), "1.5k")
        XCTAssertEqual(ContextLayoutPresentation.tokensCompact(21_000), "21k")
        XCTAssertEqual(ContextLayoutPresentation.tokensCompact(37_400), "37.4k")
        XCTAssertEqual(ContextLayoutPresentation.tokensCompact(134_700), "134.7k")
        // A negative count is a producer bug, never a negative label.
        XCTAssertEqual(ContextLayoutPresentation.tokensCompact(-500), "0")
    }

    func testTheContractPinsTheTicksAndTheTitle() throws {
        // The first tick is the floor `exponential_sessions_compact` refuses
        // below — the engine's COMPACT_MIN_CONTEXT_FRACTION mirrors it.
        XCTAssertEqual(DomainContract.contextLayoutCompactMinPercent, 50)
        let view = try XCTUnwrap(ContextLayoutPresentation.contextWindowView(
            usage: AgentSessionUsage(contextUsed: 1, contextSize: 100), segments: nil
        ))
        XCTAssertEqual(view.ticks, [50, 75, 95])
        XCTAssertEqual(ContextLayoutPresentation.title, DomainContract.contextLayoutTitle)
        XCTAssertEqual(DomainContract.contextLayoutDerivedKeys, ["conversation", "free"])
    }
}
