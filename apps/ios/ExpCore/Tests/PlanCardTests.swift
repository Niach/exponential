import Foundation
import XCTest
@testable import ExpCore

// EXP-1280: THE plan / question card's model, locked ×4 against
// packages/domain-contract/fixtures/plan-card.json (TS `plan-card.test.ts`,
// desktop `domain::plan_card`, Android `PlanCardTest`). `answerTextCases` and
// `resolveUploadCases` are the server's and the device's; the client replays
// the rest and pins its constants.
final class PlanCardTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/plan-card.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    func testCopyTestIdsAndTilesMatchTheFixture() throws {
        let fixture = try fixture()
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["answers"] as? String, PlanCard.Copy.answers)
        XCTAssertEqual(copy["feedback"] as? String, PlanCard.Copy.feedback)
        XCTAssertEqual(copy["chooseOne"] as? String, PlanCard.Copy.chooseOne)
        XCTAssertEqual(copy["chooseAny"] as? String, PlanCard.Copy.chooseAny)
        XCTAssertEqual(copy["custom"] as? String, PlanCard.Copy.custom)
        XCTAssertEqual(copy["surface"] as? String, PlanCard.Copy.surface)
        XCTAssertEqual(copy["answer"] as? String, PlanCard.Copy.answer)
        let issues = try XCTUnwrap(copy["answerIssues"] as? [String: String])
        XCTAssertEqual(Set(issues.keys), Set(PlanCard.AnswerIssue.allCases.map(\.rawValue)))
        for issue in PlanCard.AnswerIssue.allCases {
            XCTAssertEqual(issues[issue.rawValue], PlanCard.Copy.issue(issue), issue.rawValue)
        }

        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds, [
            "card": PlanCard.TestIds.card,
            "question": PlanCard.TestIds.question,
            "option": PlanCard.TestIds.option,
            "tile": PlanCard.TestIds.tile,
            "custom": PlanCard.TestIds.custom,
            "surface": PlanCard.TestIds.surface,
            "feedback": PlanCard.TestIds.feedback,
            "issue": PlanCard.TestIds.issue,
        ])

        let tiles = try XCTUnwrap(fixture["tiles"] as? [String: Double])
        XCTAssertEqual(tiles["columns"], Double(PlanCard.Tiles.columns))
        XCTAssertEqual(tiles["phoneColumns"], Double(PlanCard.Tiles.phoneColumns))
        XCTAssertEqual(tiles["gap"], PlanCard.Tiles.gap)
        XCTAssertEqual(tiles["pictureAspect"], PlanCard.Tiles.pictureAspect)
        XCTAssertEqual(tiles["radius"], PlanCard.Tiles.radius)
        XCTAssertEqual(tiles["selectedRing"], PlanCard.Tiles.selectedRing)
        XCTAssertEqual(tiles["padding"], PlanCard.Tiles.padding)
        XCTAssertEqual(fixture["uploadScheme"] as? String, PlanCard.uploadScheme)
    }

    func testAnswerIssueCases() throws {
        for testCase in try cases("answerIssueCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let card = try XCTUnwrap(testCase["card"] as? [String: Any])
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let expected = testCase["expected"] as? String
            let decision = input["decision"] as? String
            let issue: PlanCard.AnswerIssue?
            switch card["kind"] as? String {
            case "plan":
                let questions = ThreadPlanQuestion.list(card["questions"], pictures: PlanCardPicture.list(card["pictures"]))
                let answers = (input["answers"] as? [String: [String]])
                issue = PlanCard.answerIssue(
                    questions: questions,
                    input: PlanCard.AnswerInput(planAnswers: answers, decision: decision)
                )
            default:
                let parsed = TreeMessageCard(object: card)
                issue = PlanCard.answerIssue(
                    options: parsed.options.map(\.label),
                    multi: parsed.multi,
                    custom: parsed.custom,
                    input: PlanCard.AnswerInput(answer: input["answer"] as? String, labels: input["answers"] as? [String])
                )
            }
            XCTAssertEqual(issue?.rawValue, expected, name)
        }
    }

    func testPictureForCases() throws {
        for testCase in try cases("pictureForCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let pictures = testCase["pictures"] is NSNull ? nil : PlanCardPicture.list(testCase["pictures"])
            XCTAssertEqual(
                PlanCard.pictureFor(pictures, label: testCase["label"] as? String),
                testCase["expected"] as? String,
                name
            )
        }
    }

    func testOptionRowsCases() throws {
        for testCase in try cases("optionRowsCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let options = try XCTUnwrap(testCase["options"] as? [[String: Any]])
            XCTAssertEqual(
                PlanCard.optionRows(options.map { $0["attachmentId"] as? String }).rawValue,
                try XCTUnwrap(testCase["expected"] as? String),
                name
            )
        }
    }

    func testUnresolvedUploadsRenderNothing() {
        XCTAssertEqual(
            PlanCard.renderableMarkdown("Before\n\n![b](/api/attachments/a1)\n\n![a](upload:after)"),
            "Before\n\n![b](/api/attachments/a1)"
        )
        XCTAssertEqual(PlanCard.renderableMarkdown("no pictures"), "no pictures")
    }

    func testSurfaceJsonSortsKeys() throws {
        let surface = try XCTUnwrap(CardSurface.parse([
            "root": ["id": "r", "component": "Text", "props": ["text": "x"]],
            "data": ["b": 1, "a": true],
        ]))
        XCTAssertEqual(
            surface.rootJSON,
            #"{"component":"Text","id":"r","props":{"text":"x"}}"#
        )
        XCTAssertEqual(surface.data?.compactString, #"{"a":true,"b":1}"#)
        XCTAssertNil(CardSurface.parse(["nope": true]))
        XCTAssertNil(CardSurface.parse(["root": "text"]))
    }
}
