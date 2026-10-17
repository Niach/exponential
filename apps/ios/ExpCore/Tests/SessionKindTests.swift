import Foundation
import XCTest
@testable import ExpCore

// EXP-1341: the session kind traits, locked ×4 against
// `packages/domain-contract/fixtures/session-kind.json`.
final class SessionKindTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/session-kind.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func traits(_ raw: Any?) throws -> SessionTraits {
        let object = try XCTUnwrap(raw as? [String: Any])
        return SessionTraits(
            persistent: try XCTUnwrap(object["persistent"] as? Bool),
            triggers: try XCTUnwrap(object["triggers"] as? Bool)
        )
    }

    func testKindsMatchTheFixture() throws {
        let kinds = try XCTUnwrap(try fixture()["kinds"] as? [String: Any])
        XCTAssertEqual(Set(kinds.keys), Set(SessionKind.kinds.keys))
        for (kind, raw) in kinds {
            XCTAssertEqual(SessionKind.kinds[kind], try traits(raw), kind)
        }
    }

    func testReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let kind = testCase["kind"] as? String
            let expected = try traits(testCase["expected"])
            XCTAssertEqual(SessionKind.traits(kind), expected, kind ?? "null")
            XCTAssertEqual(SessionKind.isPersistent(kind), expected.persistent, kind ?? "null")
        }
    }
}
