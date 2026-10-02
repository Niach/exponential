import XCTest
@testable import ExpCore

// EXP-897/EXP-980/SLOP-3: the blocked-start prompt, byte-identical ×4 and
// locked by `domain-contract/fixtures/blocked-start.json`: the copy, the
// stack line walk, the stack plan rule, the notes and the stacked start's
// prompt text.
final class BlockedStartTests: XCTestCase {
    private struct Fixture: Decodable {
        let copy: [String: String]
        let maxRun: Int
        let reasons: [String]
        let notes: [String: String]
        let planNoteTemplate: String
        let baseTemplate: String
        let lineTemplate: String
        let textTemplate: String
        let planCases: [PlanCase]
        let lineCases: [LineCase]
        let promptCases: [PromptCase]
    }

    private struct FixtureSubject: Decodable {
        let identifier: String
        let repositoryId: String?
    }

    private struct FixtureMember: Decodable {
        let identifier: String
        let prState: String?
        let branch: String?
        let repositoryId: String?
        let running: Bool
    }

    private struct FixtureBase: Decodable, Equatable {
        let identifier: String
        let branch: String
    }

    private struct FixturePlan: Decodable {
        let base: FixtureBase?
        let run: [String]
    }

    private struct PlanCase: Decodable {
        let name: String
        let pickedCount: Int
        let subject: FixtureSubject
        let line: [FixtureMember]
        let fork: String?
        let cycle: Bool
        let plan: FixturePlan?
        let reason: String?
        let note: String?
        let planNote: String?
    }

    private struct FixtureRelation: Decodable {
        let type: String
        let issueId: String
        let relatedIssueId: String
    }

    private struct FixtureIssue: Decodable {
        let id: String
        let identifier: String
        let status: String
    }

    private struct LineCase: Decodable {
        let name: String
        let subject: String
        let relations: [FixtureRelation]
        let issues: [FixtureIssue]
        let line: [String]
        let fork: String?
        let cycle: Bool
    }

    private struct PromptCase: Decodable {
        let name: String
        let base: FixtureBase?
        let run: [String]
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

    func testTheReasonsNotesAndTemplatesMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(BlockedStart.maxRun, fixture.maxRun)
        XCTAssertEqual(BlockedStart.StackReason.allCases.map(\.rawValue), fixture.reasons)
        XCTAssertEqual(fixture.notes.count, fixture.reasons.count)
        for reason in BlockedStart.StackReason.allCases {
            let template = try XCTUnwrap(fixture.notes[reason.rawValue], reason.rawValue)
            XCTAssertEqual(
                BlockedStart.stackDisabledNote(reason, ident: "APP-7"),
                template.replacingOccurrences(of: "{ident}", with: "APP-7"),
                reason.rawValue
            )
        }
        XCTAssertEqual(BlockedStart.planNoteTemplate, fixture.planNoteTemplate)
        XCTAssertEqual(BlockedStart.baseTemplate, fixture.baseTemplate)
        XCTAssertEqual(BlockedStart.lineTemplate, fixture.lineTemplate)
        XCTAssertEqual(BlockedStart.textTemplate, fixture.textTemplate)
    }

    func testEveryLineCase() throws {
        let cases = try fixture().lineCases
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let issues = testCase.issues.map(entity)
            let relations = testCase.relations.enumerated().map { index, row in
                IssueRelationEntity(
                    id: "r\(index)", issueId: row.issueId, relatedIssueId: row.relatedIssueId,
                    type: row.type, source: "user", teamId: "team", boardId: nil,
                    createdAt: "2026-10-01T00:00:00Z", updatedAt: "2026-10-01T00:00:00Z"
                )
            }
            let result = BlockedStart.stackLine(
                subjectId: testCase.subject, relations: relations, issues: issues
            )
            XCTAssertEqual(result.line.map { $0.identifier ?? "" }, testCase.line, testCase.name)
            XCTAssertEqual(result.fork?.identifier, testCase.fork, testCase.name)
            XCTAssertEqual(result.cycle, testCase.cycle, testCase.name)
        }
    }

    func testEveryPlanCase() throws {
        let cases = try fixture().planCases
        XCTAssertFalse(cases.isEmpty, "the blocked-start fixture must not be empty")
        for testCase in cases {
            let result = BlockedStart.stackPlan(
                pickedCount: testCase.pickedCount,
                subject: BlockedStart.Subject(
                    identifier: testCase.subject.identifier,
                    repositoryId: testCase.subject.repositoryId
                ),
                line: testCase.line.map {
                    BlockedStart.LineMember(
                        identifier: $0.identifier, prState: $0.prState, branch: $0.branch,
                        repositoryId: $0.repositoryId, running: $0.running
                    )
                },
                fork: testCase.fork,
                cycle: testCase.cycle
            )
            XCTAssertEqual(
                result.plan?.base.map { FixtureBase(identifier: $0.identifier, branch: $0.branch) },
                testCase.plan?.base,
                testCase.name
            )
            XCTAssertEqual(result.plan?.run, testCase.plan?.run, testCase.name)
            XCTAssertEqual(result.plan == nil, testCase.plan == nil, testCase.name)
            XCTAssertEqual(result.reason?.rawValue, testCase.reason, testCase.name)
            XCTAssertEqual(result.note, testCase.note, testCase.name)
            XCTAssertEqual(result.planNote, testCase.planNote, testCase.name)
        }
    }

    func testEveryPromptCase() throws {
        let cases = try fixture().promptCases
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let plan = BlockedStart.StackPlan(
                base: testCase.base.map {
                    BlockedStart.StackBase(identifier: $0.identifier, branch: $0.branch)
                },
                run: testCase.run
            )
            XCTAssertEqual(
                BlockedStart.stackedStartPrompt(plan: plan, text: testCase.text),
                testCase.prompt,
                testCase.name
            )
        }
    }

    private func entity(_ row: FixtureIssue) -> IssueEntity {
        IssueEntity(
            id: row.id, boardId: "board", number: nil, identifier: row.identifier,
            title: row.identifier, description: nil, status: row.status,
            priority: "none", assigneeId: nil, creatorId: nil, source: nil, dueDate: nil,
            sortOrder: nil, completedAt: nil, duplicateOfId: nil, prUrl: nil,
            prNumber: nil, prState: nil, branch: nil, prMergedAt: nil,
            createdAt: "2026-10-01T00:00:00Z", updatedAt: "2026-10-01T00:00:00Z"
        )
    }
}
