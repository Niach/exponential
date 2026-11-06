import ExpPush
import Foundation
import XCTest

// #EXP-1639: the Live Activity model, replayed off `live-activity.json`
// (TS `live-activity.test.ts`, Android `LiveActivityTest`, same case names),
// plus the widget's reading of the opened content.
final class LiveActivityModelTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/live-activity.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let all = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(all.isEmpty, key)
        return all
    }

    private func str(_ o: [String: Any], _ key: String) -> String? { o[key] as? String }

    private func session(_ o: [String: Any]) -> LiveActivityModel.Session {
        LiveActivityModel.Session(
            id: o["id"] as! String,
            parentId: str(o, "parentId"),
            userId: o["userId"] as! String,
            agentId: str(o, "agentId"),
            startedByAgentId: str(o, "startedByAgentId"),
            title: str(o, "title"),
            sealedTitle: o["sealedTitle"] as! Bool,
            status: o["status"] as! String,
            busy: o["busy"] as! Bool,
            needsInput: o["needsInput"] as! Bool,
            blocked: o["blocked"] as! Bool,
            startedAt: str(o, "startedAt")
        )
    }

    func testCopySegmentsAndLimitsAreTheFixtures() throws {
        let f = try fixture()
        let copy = try XCTUnwrap(f["copy"] as? [String: Any])
        XCTAssertEqual(copy["encryptedTitle"] as? String, LiveActivityModel.encryptedTitle)
        XCTAssertEqual(copy["prBadge"] as? String, LiveActivityModel.prBadge)
        let phases = try XCTUnwrap(copy["phases"] as? [String: String])
        XCTAssertEqual(phases, Dictionary(uniqueKeysWithValues: LiveActivityModel.phases.map { ($0.key, $0.label) }))
        XCTAssertEqual(f["segments"] as? [String], LiveActivityModel.segments)
        XCTAssertEqual(f["maxActivities"] as? Int, LiveActivityModel.maxActivities)
        XCTAssertEqual(f["dismissMinutes"] as? Int, LiveActivityModel.dismissMinutes)
        XCTAssertEqual(f["minUpdateSeconds"] as? Int, LiveActivityModel.minUpdateSeconds)
    }

    func testFollowCases() throws {
        for c in try cases("followCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let sessions = try XCTUnwrap(input["sessions"] as? [[String: Any]]).map(session)
            let got = LiveActivityModel.follows(me: input["me"] as! String, sessions: sessions)
            XCTAssertEqual(got, c["expected"] as? [String], c["name"] as? String ?? "")
        }
    }

    func testContentCases() throws {
        for c in try cases("contentCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let e = try XCTUnwrap(c["expected"] as? [String: Any])
            let got = LiveActivityModel.content(
                session: session(try XCTUnwrap(input["session"] as? [String: Any])),
                deviceLabel: str(input, "deviceLabel"),
                prNumber: input["prNumber"] as? Int,
                askPreview: str(input, "askPreview"),
                blockedLabel: str(input, "blockedLabel")
            )
            let expected = LiveActivityModel.Content(
                phase: e["phase"] as! String,
                title: e["title"] as! String,
                subtitle: e["subtitle"] as! String,
                detail: str(e, "detail"),
                badge: e["badge"] as! String,
                segment: e["segment"] as! Int
            )
            XCTAssertEqual(got, expected, c["name"] as? String ?? "")
        }
    }

    /// The server's `activityPayload` of every content case reads back as
    /// what the widget draws.
    func testTheOpenedPayloadDrawsTheContent() throws {
        for c in try cases("contentCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let raw = try XCTUnwrap(input["session"] as? [String: Any])
            let content = LiveActivityModel.content(
                session: session(raw),
                deviceLabel: str(input, "deviceLabel"),
                prNumber: input["prNumber"] as? Int,
                askPreview: str(input, "askPreview"),
                blockedLabel: str(input, "blockedLabel")
            )
            var data = [
                "type": LiveActivityModel.pushType, "sessionId": raw["id"] as! String, "phase": content.phase,
                "badge": content.badge, "segment": String(content.segment), "userId": "u1",
                "startedAt": raw["startedAt"] as! String,
            ]
            if let detail = content.detail { data["detail"] = detail }
            let plaintext = try JSONSerialization.data(withJSONObject: [
                "title": content.title, "body": content.subtitle, "data": data,
            ])
            let opened = try XCTUnwrap(SealedPush.map(plaintext: plaintext, userInfo: [SealedPush.sealedKey: "x"]))
            let shown = try XCTUnwrap(LiveActivityDisplay.from(opened: opened), c["name"] as? String ?? "")
            XCTAssertEqual(shown.title, content.title)
            XCTAssertEqual(shown.subtitle, content.subtitle)
            XCTAssertEqual(shown.detail, content.detail)
            XCTAssertEqual(shown.phase, content.phase)
            XCTAssertEqual(shown.sessionId, "s1")
            XCTAssertNotNil(shown.startedAt)
            switch content.badge {
            case "timer":
                XCTAssertEqual(shown.badge, .timer)
                XCTAssertEqual(shown.badgeText, content.phase == "done" ? "Done" : nil)
            case "needs-you":
                XCTAssertEqual(shown.badge, .needsYou)
                XCTAssertEqual(shown.badgeText, "Needs you")
            default:
                XCTAssertEqual(shown.badgeText, content.badge)
            }
            XCTAssertEqual(shown.deepLink?.absoluteString, "exponential://session/s1?userId=u1")
        }
    }

    func testOtherPushesAndUnopenableBlobsDrawNothingOfTheirOwn() {
        XCTAssertNil(LiveActivityDisplay.from(title: "New comment", body: nil, data: ["type": "comment"]))
        let shown = LiveActivityDisplay.open(sealed: "not-a-blob", keys: [])
        XCTAssertEqual(shown, .sealed)
        XCTAssertEqual(shown.title, "Encrypted session")
        XCTAssertNil(shown.deepLink)
    }
}
