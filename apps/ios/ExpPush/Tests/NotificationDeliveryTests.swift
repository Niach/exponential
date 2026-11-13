import ExpPush
import Foundation
import XCTest

// #EXP-1815: notification-delivery.json replayed ×4 (TS
// `notification-delivery.test.ts`, Android `NotificationDeliveryTest`,
// desktop `domain::notification_delivery`): every `*Cases` array.
final class NotificationDeliveryTests: XCTestCase {
    private typealias D = NotificationDelivery

    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/notification-delivery.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let cases = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(cases.isEmpty, key)
        return cases
    }

    private func string(_ value: Any?) -> String? { value as? String }

    private func prefs(_ value: Any?) -> D.SourcePrefs? {
        guard let dict = value as? [String: Any] else { return nil }
        return D.SourcePrefs(
            assistant: dict["assistant"] as? Bool,
            temporary: dict["temporary"] as? Bool,
            agents: dict["agents"] as? [String: Bool]
        )
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        let keys = try XCTUnwrap(f["dataKeys"] as? [String: String])
        XCTAssertEqual(keys["thread"], D.DataKeys.thread)
        XCTAssertEqual(keys["source"], D.DataKeys.source)
        XCTAssertEqual(keys["sourceName"], D.DataKeys.sourceName)
        XCTAssertEqual(keys["dismissType"], D.DataKeys.dismissType)
        XCTAssertEqual(keys["ids"], D.DataKeys.ids)
        XCTAssertEqual(keys["threads"], D.DataKeys.threads)
        XCTAssertEqual(keys["all"], D.DataKeys.all)
        XCTAssertEqual(keys["teamId"], D.DataKeys.teamId)
        let limits = try XCTUnwrap(f["limits"] as? [String: Int])
        XCTAssertEqual(limits["maxIds"], D.Limits.maxIds)
        XCTAssertEqual(limits["maxThreads"], D.Limits.maxThreads)
        let copy = try XCTUnwrap(f["copy"] as? [String: String])
        XCTAssertEqual(copy["toastAction"], D.Copy.toastAction)
        XCTAssertEqual(copy["sessionsBand"], D.Copy.sessionsBand)
        XCTAssertEqual(copy["assistant"], D.Copy.assistant)
        XCTAssertEqual(copy["temporary"], D.Copy.temporary)
        XCTAssertEqual(copy["agentsBand"], D.Copy.agentsBand)
    }

    func testThreadCases() throws {
        for c in try cases("threadCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let row = D.ThreadRow(
                type: try XCTUnwrap(string(input["type"])),
                issueId: string(input["issueId"]),
                sessionRootId: string(input["sessionRootId"]),
                deviceId: string(input["deviceId"])
            )
            XCTAssertEqual(D.notificationThread(row), string(c["expected"]), string(c["name"]) ?? "")
        }
    }

    func testPresentCases() throws {
        for c in try cases("presentCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = D.presentation(
                platform: try XCTUnwrap(D.Platform(rawValue: try XCTUnwrap(string(input["platform"])))),
                active: try XCTUnwrap(input["active"] as? Bool),
                viewing: string(input["viewing"]),
                thread: try XCTUnwrap(string(input["thread"])),
                muted: try XCTUnwrap(input["muted"] as? Bool)
            )
            XCTAssertEqual(got.rawValue, string(c["expected"]), string(c["name"]) ?? "")
        }
    }

    /// The fixture's `rowsOfRule`.
    private func rows(_ input: [String: Any]) throws -> [D.ReadRow] {
        if let rows = input["rows"] as? [[String: String]] {
            return rows.map { D.ReadRow(id: $0["id"] ?? "", thread: $0["thread"] ?? "") }
        }
        let rowsOf = try XCTUnwrap(input["rowsOf"] as? [String: Any])
        let count = try XCTUnwrap(rowsOf["count"] as? Int)
        let threads = rowsOf["threads"] as? [String]
        let threadCount = rowsOf["threadCount"] as? Int
        return (0..<count).map { i in
            D.ReadRow(id: "n\(i)", thread: threads.map { $0[i % $0.count] } ?? "t\(i % (threadCount ?? 1))")
        }
    }

    func testDismissCases() throws {
        for c in try cases("dismissCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = D.dismissPayload(
                rows: try rows(input),
                all: try XCTUnwrap(input["all"] as? Bool),
                teamId: string(input["teamId"])
            )
            XCTAssertEqual(got, c["expected"] as? [String: String], string(c["name"]) ?? "")
        }
    }

    func testMatchCases() throws {
        for c in try cases("matchCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let delivered = try XCTUnwrap(input["delivered"] as? [String: Any])
            let got = D.dismissMatches(
                try XCTUnwrap(input["payload"] as? [String: String]),
                D.Delivered(
                    notificationId: string(delivered["notificationId"]),
                    thread: string(delivered["thread"]),
                    teamId: string(delivered["teamId"])
                )
            )
            XCTAssertEqual(got, c["expected"] as? Bool, string(c["name"]) ?? "")
        }
    }

    func testSourceCases() throws {
        for c in try cases("sourceCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = D.notificationSource(D.SourceSession(
                agentId: string(input["agentId"]),
                agentRole: string(input["agentRole"]),
                startedByAgentId: string(input["startedByAgentId"]),
                startedByAgentRole: string(input["startedByAgentRole"])
            ))
            XCTAssertEqual(got, string(c["expected"]), string(c["name"]) ?? "")
        }
    }

    func testAllowCases() throws {
        for c in try cases("allowCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = D.sourceAllowed(prefs(input["prefs"]), string(input["source"]))
            XCTAssertEqual(got, c["expected"] as? Bool, string(c["name"]) ?? "")
        }
    }

    func testToggleCases() throws {
        for c in try cases("toggleCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = D.toggleSourcePref(
                try XCTUnwrap(prefs(input["prefs"])),
                key: try XCTUnwrap(string(input["key"])),
                on: try XCTUnwrap(input["on"] as? Bool)
            )
            XCTAssertEqual(got, prefs(c["expected"]), string(c["name"]) ?? "")
        }
    }

    func testSettingsCases() throws {
        for c in try cases("settingsCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let teams = try XCTUnwrap(input["teams"] as? [[String: String]])
                .map { D.SettingsTeam(id: $0["id"] ?? "", name: $0["name"] ?? "") }
            let agents = try XCTUnwrap(input["agents"] as? [[String: String]]).map {
                D.SettingsAgent(id: $0["id"] ?? "", teamId: $0["teamId"] ?? "", name: $0["name"] ?? "", role: $0["role"] ?? "")
            }
            let expected = try XCTUnwrap(c["expected"] as? [[String: Any]]).map { group in
                D.SettingsGroup(
                    band: group["band"] as? String ?? "",
                    rows: (group["rows"] as? [[String: Any]] ?? []).map {
                        D.SettingsRow(key: $0["key"] as? String ?? "", label: $0["label"] as? String ?? "", detail: $0["detail"] as? String)
                    }
                )
            }
            XCTAssertEqual(D.settingsRows(teams: teams, agents: agents), expected, string(c["name"]) ?? "")
        }
    }

    func testChannelCases() throws {
        for c in try cases("channelCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let expected = try XCTUnwrap(c["expected"] as? [String: Any])
            let got = D.androidChannel(source: string(input["source"]), sourceName: string(input["sourceName"]))
            XCTAssertEqual(
                got,
                D.AndroidChannel(id: expected["id"] as? String ?? "", name: expected["name"] as? String, group: expected["group"] as? String),
                string(c["name"]) ?? ""
            )
        }
    }
}
