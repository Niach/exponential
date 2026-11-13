import Foundation
import XCTest
@testable import ExpCore

// #EXP-1561 / #EXP-1562: waiting cards, locked ×4 against
// `packages/domain-contract/fixtures/waiting-cards.json` (`stateCases`,
// `chipCases`, `targetCases`, `deepLinkCases`; `dismissCases` are the server's). Whole-value compares.
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
        XCTAssertEqual(constants["deepLinkMs"] as? Int, WaitingCards.Constants.deepLinkMs)
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

    func testDeepLinkCases() throws {
        let cases = try XCTUnwrap(try fixture()["deepLinkCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? "?"
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let step = WaitingCards.deepLinkStep(WaitingCards.DeepLinkInput(
                found: try XCTUnwrap(input["found"] as? Bool),
                laidOut: try XCTUnwrap(input["laidOut"] as? Bool),
                centred: try XCTUnwrap(input["centred"] as? Bool),
                paging: try XCTUnwrap(input["paging"] as? Bool),
                more: try XCTUnwrap(input["more"] as? Bool),
                elapsedMs: try XCTUnwrap((input["elapsedMs"] as? NSNumber)?.doubleValue)
            ))
            XCTAssertEqual(step.rawValue, testCase["expected"] as? String, name)
        }
    }

    /// The driver over a 600-row chat that loads its newest 200 rows and
    /// pages 200 at a time, the target = row 50 (outside the newest 200):
    /// it pages twice (one page at a time, waiting while one is in flight),
    /// reveals the lazy row, scrolls it to the centre, then rings.
    func testDeepLinkDriverPagesThenScrollsThenRings() {
        let rows = (0..<600).map { "m\($0)" }
        var loadedFrom = 400
        var link = WaitingCards.DeepLink(target: "m50", takenAtMs: 0)
        var now: Double = 0
        var distance: Double?
        func frame() -> WaitingCards.DeepLink.Frame {
            let found = rows[loadedFrom...].contains("m50")
            return .init(found: found, distance: found ? distance : nil, more: loadedFrom > 0)
        }
        var actions: [WaitingCards.DeepLink.Action] = []
        for _ in 0..<12 {
            now += 16
            let action = link.step(frame(), nowMs: now)
            actions.append(action)
            switch action {
            case .page:
                // In flight: the next frame waits.
                actions.append(link.step(frame(), nowMs: now + 1))
                loadedFrom = max(0, loadedFrom - 200)
                link.pageLanded()
            case .reveal: distance = -2400
            case .scroll: distance = distance.map { $0 == -2400 ? 0.5 : $0 }
            case .ring, .missing: break
            case .none: break
            }
            if link.settled { break }
        }
        XCTAssertEqual(actions, [.page, .none, .page, .none, .reveal, .scroll, .ring])
        XCTAssertTrue(link.settled)
        XCTAssertEqual(link.step(frame(), nowMs: now + 16), .none)
    }

    func testDeepLinkDriverGivesUpAndStalls() {
        var lost = WaitingCards.DeepLink(target: "x", takenAtMs: 1000)
        XCTAssertEqual(lost.step(.init(found: false, more: false), nowMs: 1010), .missing)

        var late = WaitingCards.DeepLink(target: "x", takenAtMs: 0)
        XCTAssertEqual(late.step(.init(found: false, more: true), nowMs: 10), .page)
        XCTAssertEqual(late.step(.init(found: false, more: true), nowMs: 10_000), .missing)

        var unmeasured = WaitingCards.DeepLink(target: "x", takenAtMs: 0)
        XCTAssertEqual(unmeasured.step(.init(found: true, more: true), nowMs: 10), .reveal)
        XCTAssertEqual(unmeasured.step(.init(found: true, more: true), nowMs: 20), .none)
        XCTAssertEqual(unmeasured.step(.init(found: true, more: true), nowMs: 12_000), .ring)

        // At the list's edge = centred as far as it can go.
        var edge = WaitingCards.DeepLink(target: "x", takenAtMs: 0)
        XCTAssertEqual(edge.step(.init(found: true, distance: 300, atEdge: true, more: false), nowMs: 5), .ring)

        // Scrolls that never move the row: the list can go no further.
        var stuck = WaitingCards.DeepLink(target: "x", takenAtMs: 0)
        let off = WaitingCards.DeepLink.Frame(found: true, distance: 40, more: false)
        XCTAssertEqual(stuck.step(off, nowMs: 1), .scroll)
        XCTAssertEqual(stuck.step(off, nowMs: 2), .scroll)
        XCTAssertEqual(stuck.step(off, nowMs: 3), .scroll)
        XCTAssertEqual(stuck.step(off, nowMs: 4), .ring)
    }

    func testWaitingCountLabel() {
        XCTAssertEqual(WaitingCards.waitingCountLabel(3), "3 waiting")
    }
}
