import Foundation
import XCTest
@testable import ExpCore

// #EXP-1561 / #EXP-1562: waiting cards, locked ×4 against
// `packages/domain-contract/fixtures/waiting-cards.json` (`stateCases`,
// `chipCases`, `targetCases`; `dismissCases` are the server's). Whole-value compares.
final class WaitingCardsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/waiting-cards.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        XCTAssertEqual(f["copy"] as? [String: String], [
            "dismiss": WaitingCards.Copy.dismiss, "dismissed": WaitingCards.Copy.dismissed,
            "waiting": WaitingCards.Copy.waiting, "waitingCount": WaitingCards.Copy.waitingCount,
        ])
        XCTAssertEqual(f["icons"] as? [String: String], [
            "plan": WaitingCards.Icons.plan, "question": WaitingCards.Icons.question,
            "dismiss": WaitingCards.Icons.dismiss, "waiting": WaitingCards.Icons.waiting,
        ])
        let constants = try XCTUnwrap(f["constants"] as? [String: Any])
        XCTAssertEqual(constants["labelMax"] as? Int, WaitingCards.Constants.labelMax)
        XCTAssertEqual(constants["ellipsis"] as? String, WaitingCards.Constants.ellipsis)
        XCTAssertEqual(constants["highlightMs"] as? Int, WaitingCards.Constants.highlightMs)
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "dismiss": WaitingCards.TestIds.dismiss, "dismissed": WaitingCards.TestIds.dismissed,
            "chip": WaitingCards.TestIds.chip, "button": WaitingCards.TestIds.button,
            "menu": WaitingCards.TestIds.menu,
        ])
    }

    func testStateCases() throws {
        let cases = try XCTUnwrap(try fixture()["stateCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? "?"
            let card = (testCase["card"] as? [String: Any]).map(TreeMessageCard.init(object:))
            let state = WaitingCards.cardState(card, answeredAt: testCase["answeredAt"] as? String)
            XCTAssertEqual(state?.rawValue, testCase["expected"] as? String, name)
        }
    }

    func testChipCases() throws {
        let cases = try XCTUnwrap(try fixture()["chipCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? "?"
            let cards = try XCTUnwrap(testCase["cards"] as? [[String: Any]]).enumerated().map { index, raw in
                AskDock.Pending(
                    id: raw["id"] as? String ?? "",
                    kind: (raw["kind"] as? String).flatMap(AskDock.Kind.init(rawValue:)) ?? .question,
                    text: raw["text"] as? String ?? "",
                    sessionName: nil,
                    createdAt: Double(index)
                )
            }
            let result = WaitingCards.chips(cards, phone: try XCTUnwrap(testCase["phone"] as? Bool))
            let expected = try XCTUnwrap(testCase["expected"] as? [String: Any])
            let actual: [String: Any] = [
                "layout": result.layout.rawValue,
                "count": result.count,
                "chips": result.chips.map {
                    ["id": $0.id, "kind": $0.kind.rawValue, "icon": $0.icon, "label": $0.label]
                },
            ]
            XCTAssertEqual(NSDictionary(dictionary: actual), NSDictionary(dictionary: expected), name)
        }
    }

    func testTargetCases() throws {
        let cases = try XCTUnwrap(try fixture()["targetCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? "?"
            let card = try XCTUnwrap(testCase["card"] as? [String: Any])
            let picked = WaitingCards.TargetCard(
                kind: try XCTUnwrap((card["kind"] as? String).flatMap(AskDock.Kind.init(rawValue:))),
                sessionId: try XCTUnwrap(card["sessionId"] as? String)
            )
            let target = WaitingCards.waitingTarget(card: picked, rootId: try XCTUnwrap(testCase["rootId"] as? String))
            let actual: [String: Any] = ["open": target.open.rawValue, "sessionId": target.sessionId]
            let expected = try XCTUnwrap(testCase["expected"] as? [String: Any])
            XCTAssertEqual(NSDictionary(dictionary: actual), NSDictionary(dictionary: expected), name)
        }
    }

    func testWaitingCountLabel() {
        XCTAssertEqual(WaitingCards.waitingCountLabel(3), "3 waiting")
    }
}
