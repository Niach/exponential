import XCTest
@testable import ExpCore

// EXP-897/EXP-980/SLOP-3: the blocked-start prompt, byte-identical ×4 and
// locked by `domain-contract/fixtures/blocked-start.json`: the copy, the
// stack target rule and the stacked start's prompt text.
final class BlockedStartTests: XCTestCase {
    private struct Fixture: Decodable {
        let copy: [String: String]
        let reasons: [String]
        let notes: [String: String]
        let promptTemplate: String
        let targetCases: [TargetCase]
        let promptCases: [PromptCase]
    }

    private struct FixtureBlocker: Decodable {
        let identifier: String
        let prState: String?
        let branch: String?
        let repositoryId: String?
    }

    private struct TargetCase: Decodable {
        let name: String
        let pickedCount: Int
        let subjectRepositoryId: String?
        let blockers: [FixtureBlocker]
        let target: String?
        let reason: String?
        let note: String?
    }

    private struct PromptCase: Decodable {
        let name: String
        let identifier: String
        let branch: String
        let text: String
        let prompt: String
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/blocked-start.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testTheCopyMatchesTheFixture() throws {
        let copy = try fixture().copy
        XCTAssertEqual(copy["title"], BlockedStart.blockedStartTitle)
        XCTAssertEqual(copy["batchTitle"], BlockedStart.blockedBatchTitle)
        XCTAssertEqual(copy["batchBody"], BlockedStart.blockedBatchBody)
        XCTAssertEqual(copy["bodyPrefix"], BlockedStart.bodyPrefix)
        XCTAssertEqual(copy["bodySuffix"], BlockedStart.bodySuffix)
        XCTAssertEqual(copy["bodySuffixStackable"], BlockedStart.bodySuffixStackable)
        XCTAssertEqual(copy["startAnyway"], BlockedStart.startAnywayLabel)
        XCTAssertEqual(copy["stackedPr"], BlockedStart.stackedPrLabel)
        XCTAssertEqual(copy.count, 8, "a new copy key needs a constant here")
    }

    func testTheReasonsAndNotesMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(BlockedStart.StackReason.allCases.map(\.rawValue), fixture.reasons)
        XCTAssertEqual(fixture.promptTemplate, BlockedStart.promptTemplate)
        for reason in BlockedStart.StackReason.allCases {
            let template = try XCTUnwrap(fixture.notes[reason.rawValue], reason.rawValue)
            XCTAssertEqual(
                BlockedStart.stackDisabledNote(reason, ident: "APP-7"),
                template.replacingOccurrences(of: "{ident}", with: "APP-7"),
                reason.rawValue
            )
        }
    }

    func testEveryTargetCase() throws {
        let cases = try fixture().targetCases
        XCTAssertFalse(cases.isEmpty, "the blocked-start fixture must not be empty")
        for testCase in cases {
            let blockers = testCase.blockers.map {
                BlockedStart.Blocker(
                    identifier: $0.identifier, prState: $0.prState,
                    branch: $0.branch, repositoryId: $0.repositoryId
                )
            }
            let result = BlockedStart.stackTarget(
                pickedCount: testCase.pickedCount,
                subjectRepositoryId: testCase.subjectRepositoryId,
                blockers: blockers
            )
            XCTAssertEqual(result.target?.identifier, testCase.target, testCase.name)
            XCTAssertEqual(result.reason?.rawValue, testCase.reason, testCase.name)
            let note = result.reason.map {
                BlockedStart.stackDisabledNote($0, ident: blockers.first?.identifier ?? "")
            }
            XCTAssertEqual(note, testCase.note, testCase.name)
        }
    }

    func testEveryPromptCase() throws {
        let cases = try fixture().promptCases
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            XCTAssertEqual(
                BlockedStart.stackedStartPrompt(
                    identifier: testCase.identifier, branch: testCase.branch, text: testCase.text
                ),
                testCase.prompt,
                testCase.name
            )
        }
    }
}
