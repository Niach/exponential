import Foundation
import XCTest
@testable import ExpCore

// #NEW-146: THE elicitation card, locked ×4 against
// packages/domain-contract/fixtures/elicitation-card.json (TS
// `elicitation-card.test.ts`, desktop `domain::elicitation_card`, Android
// `ElicitationCardTest`): the renderers replay `missingCases` and
// `actionCases`, whole values compared, and pin the constants.
final class ElicitationCardTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/elicitation-card.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    func testConstantsMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["formKey"] as? String, ElicitationCard.formKey)
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["submit"] as? String, ElicitationCard.Copy.submit)
        XCTAssertEqual(copy["decline"] as? String, ElicitationCard.Copy.decline)
        XCTAssertEqual(copy["cancel"] as? String, ElicitationCard.Copy.cancel)
        XCTAssertEqual(copy["openLink"] as? String, ElicitationCard.Copy.openLink)
        XCTAssertEqual(copy["done"] as? String, ElicitationCard.Copy.done)
        XCTAssertEqual(copy["missing"] as? String, ElicitationCard.Copy.missing)
        XCTAssertEqual(copy["unknownServer"] as? String, ElicitationCard.Copy.unknownServer)
        XCTAssertEqual(try XCTUnwrap(copy["specimen"] as? [String: String]), [
            "form": ElicitationCard.Copy.Specimen.form,
            "url": ElicitationCard.Copy.Specimen.url,
            "warned": ElicitationCard.Copy.Specimen.warned,
            "answered": ElicitationCard.Copy.Specimen.answered,
        ])
        XCTAssertEqual(try XCTUnwrap(fixture["icons"] as? [String: String]), [
            "server": ElicitationCard.Icons.server,
            "url": ElicitationCard.Icons.url,
        ])
        XCTAssertEqual(try XCTUnwrap(fixture["testIds"] as? [String: String]), [
            "card": ElicitationCard.TestIds.card,
            "server": ElicitationCard.TestIds.server,
            "submit": ElicitationCard.TestIds.submit,
            "decline": ElicitationCard.TestIds.decline,
            "cancel": ElicitationCard.TestIds.cancel,
            "openLink": ElicitationCard.TestIds.openLink,
            "done": ElicitationCard.TestIds.done,
        ])
    }

    func testMissingCases() throws {
        for entry in try cases("missingCases") {
            let name = entry["name"] as? String ?? "?"
            let required = try XCTUnwrap(entry["required"] as? [String], name)
            let expected = try XCTUnwrap(entry["expected"] as? [String], name)
            let data = CardJSON(any: entry["surfaceData"])
            XCTAssertEqual(ElicitationCard.missing(required: required, surfaceData: data), expected, name)
        }
    }

    func testActionCases() throws {
        for entry in try cases("actionCases") {
            let name = entry["name"] as? String ?? "?"
            let card = try XCTUnwrap(entry["card"] as? [String: Any], name)
            let expected = try XCTUnwrap((entry["expected"] as? String).flatMap(ElicitationCard.Action.init(rawValue:)), name)
            let action = ElicitationCard.action(
                answers: (card["answers"] as? [Any])?.compactMap { $0 as? String },
                answer: card["answer"] as? String,
                dismissed: CardJSON.isTrue(card["dismissed"])
            )
            XCTAssertEqual(action, expected, name)
        }
    }

    /// Every card case's sidecar reaches the question row (session-tree.ts
    /// `elicitationField`) whole.
    func testCardCasesCarryTheSidecarOntoTheRow() throws {
        for entry in try cases("cardCases") {
            let name = entry["name"] as? String ?? "?"
            guard let expected = entry["expected"] as? [String: Any] else { continue }
            let card = TreeMessageCard(object: expected)
            let row = SessionChatRow.askCard(TreeMessage(
                id: "m1", sessionId: "s1", authorKind: "agent", authorId: nil, body: "",
                card: card, answeredAt: nil, consumedAt: nil, createdAt: "2026-11-26T12:00:00.000Z",
                authorSessionId: nil
            ))
            guard case let .question(question)? = row else { return XCTFail(name) }
            let raw = try XCTUnwrap(expected["elicitation"] as? [String: Any], name)
            XCTAssertEqual(question.elicitation, ElicitationCard.Sidecar(
                server: try XCTUnwrap(raw["server"] as? String),
                mode: try XCTUnwrap((raw["mode"] as? String).flatMap(ElicitationCard.Mode.init(rawValue:))),
                required: try XCTUnwrap(raw["required"] as? [String]),
                url: raw["url"] as? String
            ), name)
            XCTAssertFalse(question.custom, name)
        }
    }
}
