import Foundation
import XCTest
@testable import ExpCore

// NEW-67: Settings → Voice, locked ×4 (TS `voice.ts`, desktop
// `domain::voice`, Android `VoiceTest`) against fixtures/voice.json: every
// `copy` key, the `testIds`, `shortcuts`, `defaults` and every case list but
// the speech ones (shouldSpeak / spokenText / numbers are desktop-only).
final class VoiceTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/voice.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let cases = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(cases.isEmpty, key)
        return cases
    }

    func testCopyMatchesTheFixture() throws {
        let copy = try XCTUnwrap(try fixture()["copy"] as? [String: String])
        let mirrored: [String: String] = [
            "group": Voice.Copy.group,
            "voiceDevice": Voice.Copy.voiceDevice,
            "voiceDeviceNone": Voice.Copy.voiceDeviceNone,
            "agent": Voice.Copy.agent,
            "agentNone": Voice.Copy.agentNone,
            "speakReplies": Voice.Copy.speakReplies,
            "trayHeader": Voice.Copy.trayHeader,
            "pushToTalk": Voice.Copy.pushToTalk,
            "shortcut": Voice.Copy.shortcut,
            "wakeWord": Voice.Copy.wakeWord,
            "wakePhrase": Voice.Copy.wakePhrase,
            "wakePhrasePlaceholder": Voice.Copy.wakePhrasePlaceholder,
            "downloading": Voice.Copy.downloading,
            "downloadFailed": Voice.Copy.downloadFailed,
            "unavailable": Voice.Copy.unavailable,
            "speaking": Voice.Copy.speaking,
            "stop": Voice.Copy.stop,
            "link": Voice.Copy.link,
            "ellipsis": Voice.Copy.ellipsis,
        ]
        XCTAssertEqual(copy, mirrored)
    }

    func testTestIdsShortcutsAndMaxCharsMatchTheFixture() throws {
        let fixture = try fixture()
        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds, [
            "page": Voice.TestIds.page,
            "voiceDevice": Voice.TestIds.voiceDevice,
            "agent": Voice.TestIds.agent,
            "speakReplies": Voice.TestIds.speakReplies,
            "pushToTalk": Voice.TestIds.pushToTalk,
            "shortcut": Voice.TestIds.shortcut,
            "wakeWord": Voice.TestIds.wakeWord,
            "wakePhrase": Voice.TestIds.wakePhrase,
        ])
        let shortcuts = try XCTUnwrap(fixture["shortcuts"] as? [[String: String]]).map {
            Voice.Shortcut(value: $0["value"] ?? "", label: $0["label"] ?? "")
        }
        XCTAssertEqual(shortcuts, Voice.shortcuts)
        XCTAssertEqual(fixture["maxChars"] as? Int, Voice.maxChars)
    }

    /// `defaults` decodes into the mirrored row (and `voice.get`'s shape).
    func testDefaultsMatchTheFixture() throws {
        let defaults = try XCTUnwrap(try fixture()["defaults"])
        let data = try JSONSerialization.data(withJSONObject: defaults)
        XCTAssertEqual(try JSONDecoder().decode(Voice.Settings.self, from: data), Voice.defaults)
        XCTAssertTrue(Voice.shortcuts.contains { $0.value == Voice.defaults.pttShortcut })
    }

    func testVoiceDeviceOptionsCases() throws {
        for testCase in try cases("voiceDeviceOptionsCases") {
            let name = testCase["name"] as? String ?? "?"
            let devices = try XCTUnwrap(testCase["devices"] as? [[String: String]]).map {
                Voice.DeviceLike(deviceId: $0["deviceId"] ?? "", name: $0["name"] ?? "", kind: $0["kind"] ?? "")
            }
            XCTAssertEqual(Voice.voiceDeviceOptions(devices), try options(testCase), name)
        }
    }

    func testAgentOptionsCases() throws {
        for testCase in try cases("agentOptionsCases") {
            let name = testCase["name"] as? String ?? "?"
            let agents = try XCTUnwrap(testCase["agents"] as? [[String: String]]).map {
                Voice.AgentLike(id: $0["id"] ?? "", name: $0["name"] ?? "", team: $0["team"] ?? "")
            }
            XCTAssertEqual(Voice.agentOptions(agents), try options(testCase), name)
        }
    }

    func testClearOnNoneCases() throws {
        for testCase in try cases("clearOnNoneCases") {
            let name = testCase["name"] as? String ?? "?"
            let before = try settings(XCTUnwrap(testCase["before"] as? [String: Any]))
            let after = try settings(XCTUnwrap(testCase["after"] as? [String: Any]))
            let raw = try XCTUnwrap(testCase["change"] as? [String: Any])
            var change = Voice.Change()
            if raw.keys.contains("voiceDeviceId") { change.voiceDeviceId = .some(raw["voiceDeviceId"] as? String) }
            if raw.keys.contains("agentId") { change.agentId = .some(raw["agentId"] as? String) }
            XCTAssertEqual(Voice.clearOnNone(before, change), after, name)
        }
    }

    func testTrayHeaderCases() throws {
        for testCase in try cases("trayHeaderCases") {
            XCTAssertEqual(Voice.trayHeader(testCase["device"] as? String ?? ""), testCase["header"] as? String)
        }
    }

    func testWakePhrasePlaceholderCases() throws {
        for testCase in try cases("wakePhrasePlaceholderCases") {
            XCTAssertEqual(Voice.wakePhrasePlaceholder(testCase["agent"] as? String ?? ""), testCase["placeholder"] as? String)
        }
    }

    /// None travels as an explicit null; an untouched field writes no key.
    func testChangeEncodesNoneAsNull() throws {
        let data = try JSONEncoder().encode(Voice.Change(agentId: .some(nil), speakReplies: true))
        let object = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(Set(object.keys), ["agentId", "speakReplies"])
        XCTAssertTrue(object["agentId"] is NSNull)
        XCTAssertEqual(object["speakReplies"] as? Bool, true)
        XCTAssertEqual(String(decoding: try JSONEncoder().encode(Voice.Change()), as: UTF8.self), "{}")
    }

    private func options(_ testCase: [String: Any]) throws -> [Voice.Option] {
        try XCTUnwrap(testCase["options"] as? [[String: Any]]).map {
            Voice.Option(id: $0["id"] as? String, label: $0["label"] as? String ?? "")
        }
    }

    /// A case's partial row over the defaults.
    private func settings(_ raw: [String: Any]) -> Voice.Settings {
        var settings = Voice.defaults
        settings.voiceDeviceId = raw["voiceDeviceId"] as? String
        settings.agentId = raw["agentId"] as? String
        settings.speakReplies = raw["speakReplies"] as? Bool ?? false
        settings.pushToTalk = raw["pushToTalk"] as? Bool ?? false
        settings.wakeWord = raw["wakeWord"] as? Bool ?? false
        return settings
    }
}
