import Foundation
import XCTest
@testable import ExpCore

// EXP-1274: the empty-state copy, locked ×4 against `empty-state.json`.
final class EmptyStateCopyTests: XCTestCase {
    private func fixtureStates() throws -> [String: [String: String]] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/empty-state.json")
        let json = try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
        return try XCTUnwrap(json["states"] as? [String: [String: String]])
    }

    func testEveryStateMatchesTheFixture() throws {
        let states = try fixtureStates()
        for state in EmptyStateCopy.all {
            let row = try XCTUnwrap(states[state.id], "\(state.id) is not in the fixture")
            XCTAssertEqual(state.size.rawValue, row["size"], state.id)
            XCTAssertEqual(state.icon, row["icon"], state.id)
            XCTAssertEqual(state.title, row["title"], state.id)
            XCTAssertEqual(state.description, row["description"], state.id)
            XCTAssertEqual(state.action, row["action"], state.id)
        }
    }

    func testTheFixtureHasNoUndeclaredState() throws {
        let declared = EmptyStateCopy.all.map(\.id)
        XCTAssertEqual(Set(declared).count, declared.count, "a state is declared twice")
        XCTAssertEqual(Set(declared), Set(try fixtureStates().keys))
    }

    func testCopyRules() {
        for state in EmptyStateCopy.all {
            XCTAssertFalse(state.title.hasSuffix("."), state.id)
            if state.size == .list {
                XCTAssertNil(state.description, state.id)
                XCTAssertNil(state.action, state.id)
            }
        }
    }
}
