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
        XCTAssertEqual(copy["answered"] as? String, PlanCard.Copy.answered)
        XCTAssertEqual(copy["answering"] as? String, PlanCard.Copy.answering)
        XCTAssertEqual(copy["feedbackLine"] as? String, PlanCard.Copy.feedbackLine)
        XCTAssertEqual(PlanCard.Copy.feedbackLine("Keep it short"), "Feedback: Keep it short")
        let specimen = try XCTUnwrap(copy["specimen"] as? [String: String])
        XCTAssertEqual(specimen, [
            "pending": PlanCard.Copy.Specimen.pending,
            "answered": PlanCard.Copy.Specimen.answered,
            "approved": PlanCard.Copy.Specimen.approved,
        ])
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
            "step": PlanCard.TestIds.step,
            "stepBack": PlanCard.TestIds.stepBack,
            "stepNext": PlanCard.TestIds.stepNext,
            "decision": PlanCard.TestIds.decision,
        ])

        // Every key of `tiles` and `card`, both ways (a new fixture key
        // without a mirror fails here).
        let tiles = try XCTUnwrap(fixture["tiles"] as? [String: Double])
        let tileMirror: [String: Double] = [
            "columns": Double(PlanCard.Tiles.columns),
            "phoneColumns": Double(PlanCard.Tiles.phoneColumns),
            "gap": PlanCard.Tiles.gap,
            "pictureAspect": PlanCard.Tiles.pictureAspect,
            "radius": PlanCard.Tiles.radius,
            "selectedRing": PlanCard.Tiles.selectedRing,
            "padding": PlanCard.Tiles.padding,
            "pictureRadius": PlanCard.Tiles.pictureRadius,
            "stroke": PlanCard.Tiles.stroke,
            "placeholderGlyph": PlanCard.Tiles.placeholderGlyph,
            "labelGap": PlanCard.Tiles.labelGap,
            "inertOpacity": PlanCard.Tiles.inertOpacity,
        ]
        XCTAssertEqual(Set(tiles.keys), Set(tileMirror.keys))
        for (key, value) in tiles {
            XCTAssertEqual(tileMirror[key], value, "tiles.\(key)")
        }
        let card = try XCTUnwrap(fixture["card"] as? [String: Double])
        let cardMirror: [String: Double] = [
            "pictureRadius": PlanCard.Card.pictureRadius,
            "pictureStroke": PlanCard.Card.pictureStroke,
            "surfaceRadius": PlanCard.Card.surfaceRadius,
            "surfacePadding": PlanCard.Card.surfacePadding,
            "surfaceLabelGap": PlanCard.Card.surfaceLabelGap,
            "blockGap": PlanCard.Card.blockGap,
            "blockInnerGap": PlanCard.Card.blockInnerGap,
            "optionsGap": PlanCard.Card.optionsGap,
            "rowMinHeight": PlanCard.Card.rowMinHeight,
            "rowRadius": PlanCard.Card.rowRadius,
            "fieldMinHeight": PlanCard.Card.fieldMinHeight,
            "inertOpacity": PlanCard.Card.inertOpacity,
            "listIndent": PlanCard.Card.listIndent,
            "headingScale": PlanCard.Card.headingScale,
        ]
        XCTAssertEqual(Set(card.keys), Set(cardMirror.keys))
        for (key, value) in card {
            XCTAssertEqual(cardMirror[key], value, "card.\(key)")
        }
        XCTAssertEqual(fixture["uploadScheme"] as? String, PlanCard.uploadScheme)
    }

    // NEW-6 (N-1): the stepper's constants + copy, and when it applies.
    func testStepperMatchesTheFixture() throws {
        let fixture = try fixture()
        let stepper = try XCTUnwrap(fixture["stepper"] as? [String: Any])
        XCTAssertEqual(stepper["minQuestions"] as? Int, PlanCard.Stepper.minQuestions)
        XCTAssertEqual(stepper["buttonGap"] as? Double, PlanCard.Stepper.buttonGap)
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["stepLabel"] as? String, PlanCard.Copy.stepLabel)
        XCTAssertEqual(copy["back"] as? String, PlanCard.Copy.back)
        XCTAssertEqual(copy["next"] as? String, PlanCard.Copy.next)
        XCTAssertEqual(copy["decide"] as? String, PlanCard.Copy.decide)
        XCTAssertEqual(PlanCard.Copy.stepLabel(step: 2, count: 3), "Question 2 of 3")

        XCTAssertTrue(PlanCard.stepperApplies(kind: "plan", decision: nil, questionCount: 2))
        XCTAssertFalse(PlanCard.stepperApplies(kind: "plan", decision: nil, questionCount: 1))
        XCTAssertFalse(PlanCard.stepperApplies(kind: "plan", decision: "approved", questionCount: 3))
        XCTAssertFalse(PlanCard.stepperApplies(kind: "plan", decision: "rejected", questionCount: 3))
        XCTAssertFalse(PlanCard.stepperApplies(kind: "question", decision: nil, questionCount: 3))
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

    /// Web `togglePick` (`plan-card.test.tsx`): a single choice picks /
    /// unpicks, a multi toggles and keeps OPTION order whatever the tap order.
    func testTogglePickKeepsOptionOrder() {
        let order = ["a", "b", "c"]
        XCTAssertEqual(PlanCard.togglePick([], label: "a", multi: false, order: order), ["a"])
        XCTAssertEqual(PlanCard.togglePick(["a"], label: "b", multi: false, order: order), ["b"])
        XCTAssertEqual(PlanCard.togglePick(["a"], label: "a", multi: false, order: order), [])
        XCTAssertEqual(PlanCard.togglePick(["b"], label: "a", multi: true, order: order), ["a", "b"])
        XCTAssertEqual(PlanCard.togglePick(["a", "b"], label: "a", multi: true, order: order), ["b"])
        XCTAssertEqual(PlanCard.togglePick(["c", "a"], label: "b", multi: true, order: order), ["a", "b", "c"])
        // A pick that is no option sorts last, in pick order.
        XCTAssertEqual(PlanCard.togglePick(["zz", "yy"], label: "b", multi: true, order: order), ["b", "zz", "yy"])
    }

    /// Web `closedPicks`: `answers` wins; a legacy bare `answer` is a pick
    /// only when it names an option, so an old joined multi answer "A, B" (or
    /// a free answer) highlights nothing and reads as the answer line.
    func testClosedPicksAndTheAnswerLine() {
        let options = [TreeQuestionOption(label: "A"), TreeQuestionOption(label: "B")]
        XCTAssertEqual(PlanCard.closedPicks(options: options, answer: "A, B", answers: ["A", "B"]), ["A", "B"])
        XCTAssertEqual(PlanCard.closedPicks(options: options, answer: "B", answers: nil), ["B"])
        XCTAssertEqual(PlanCard.closedPicks(options: options, answer: "A, B", answers: nil), [])
        XCTAssertEqual(PlanCard.closedPicks(options: options, answer: "free", answers: nil), [])
        XCTAssertEqual(PlanCard.closedPicks(options: options, answer: nil, answers: nil), [])

        let row = { (answer: String?, answers: [String]?) in
            ThreadQuestionRow(
                id: "m1", sessionId: "s1", question: "Which?", options: options, multi: true,
                answer: answer, answers: answers, answered: true
            )
        }
        XCTAssertEqual(row("A, B", ["A", "B"]).picked, ["A", "B"])
        XCTAssertEqual(row("A, B", ["A", "B"]).closedAnswerLine, [])
        XCTAssertEqual(row("B", nil).picked, ["B"])
        XCTAssertEqual(row("B", nil).closedAnswerLine, [])
        XCTAssertEqual(row("A, B", nil).picked, [])
        XCTAssertEqual(row("A, B", nil).closedAnswerLine, ["A, B"])
        XCTAssertEqual(row("typed", ["A", "typed"]).picked, ["A", "typed"])
        XCTAssertEqual(row("typed", ["A", "typed"]).closedAnswerLine, ["typed"])
        XCTAssertEqual(row(nil, nil).closedAnswerLine, [])
    }

    func testLabelMaxIsTheServerCap() {
        XCTAssertEqual(PlanCard.labelMax, 500)
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
