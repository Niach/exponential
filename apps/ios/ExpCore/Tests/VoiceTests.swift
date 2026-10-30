import Foundation
import XCTest
@testable import ExpCore

// NEW-67: Settings → Voice, locked ×4 (TS `voice.ts`, desktop
// `domain::voice`, Android `VoiceTest`) against fixtures/voice.json: every
// `copy` key, the `testIds`, `shortcutGrammar`, `languages`, `limits`, `defaults` and every case list but
// the speech and tray ones (shouldSpeak / spokenText / numbers / trayMenu are desktop-only).
final class VoiceTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/etalk/fixtures/voice.json")
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
            "language": Voice.Copy.language,
            "languageNote": Voice.Copy.languageNote,
            "voiceFile": Voice.Copy.voiceFile,
            "voiceFilePlaceholder": Voice.Copy.voiceFilePlaceholder,
            "voiceTranscript": Voice.Copy.voiceTranscript,
            "voiceTranscriptPlaceholder": Voice.Copy.voiceTranscriptPlaceholder,
            "trayHeader": Voice.Copy.trayHeader,
            "pushToTalk": Voice.Copy.pushToTalk,
            "wakeWord": Voice.Copy.wakeWord,
            "wakePhrase": Voice.Copy.wakePhrase,
            "wakePhrasePlaceholder": Voice.Copy.wakePhrasePlaceholder,
            "wakePhraseTooShort": Voice.Copy.wakePhraseTooShort,
            "trayOpen": Voice.Copy.trayOpen,
            "trayCall": Voice.Copy.trayCall,
            "trayMute": Voice.Copy.trayMute,
            "traySettings": Voice.Copy.traySettings,
            "trayQuit": Voice.Copy.trayQuit,
            "downloading": Voice.Copy.downloading,
            "downloadingProgress": Voice.Copy.downloadingProgress,
            "downloadFailed": Voice.Copy.downloadFailed,
            "unavailable": Voice.Copy.unavailable,
            "speaking": Voice.Copy.speaking,
            "stop": Voice.Copy.stop,
            "link": Voice.Copy.link,
            "ellipsis": Voice.Copy.ellipsis,
            "shortcutNone": Voice.Copy.shortcutNone,
            "shortcutRecord": Voice.Copy.shortcutRecord,
            "shortcutClear": Voice.Copy.shortcutClear,
            "trayEndCall": Voice.Copy.trayEndCall,
        ]
        XCTAssertEqual(copy, mirrored)
        let ptt = try XCTUnwrap(try fixture()["pttCopy"] as? [String: Any])
        XCTAssertEqual(ptt["shortcutNeedsModifier"] as? String, Voice.PttCopy.shortcutNeedsModifier)
        XCTAssertEqual(ptt["wakeDetail"] as? [String: String], Voice.PttCopy.wakeDetail)
    }

    func testTestIdsShortcutsAndMaxCharsMatchTheFixture() throws {
        let fixture = try fixture()
        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds, [
            "page": Voice.TestIds.page,
            "voiceDevice": Voice.TestIds.voiceDevice,
            "agent": Voice.TestIds.agent,
            "language": Voice.TestIds.language,
            "voiceFile": Voice.TestIds.voiceFile,
            "voiceTranscript": Voice.TestIds.voiceTranscript,
            "shortcut": Voice.TestIds.shortcut,
            "wakeWord": Voice.TestIds.wakeWord,
            "wakePhrase": Voice.TestIds.wakePhrase,
            "shortcutClear": Voice.TestIds.shortcutClear,
        ])
        let grammar = try XCTUnwrap(fixture["shortcutGrammar"] as? [String: Any])
        XCTAssertEqual(grammar["mods"] as? [String], Voice.ShortcutGrammar.mods)
        XCTAssertEqual(grammar["named"] as? [String], Voice.ShortcutGrammar.named)
        XCTAssertEqual(grammar["bareKeys"] as? [String], Voice.ShortcutGrammar.bareKeys)
        XCTAssertEqual(grammar["mouse"] as? [String: Int], [
            "min": Voice.ShortcutGrammar.mouse.lowerBound, "max": Voice.ShortcutGrammar.mouse.upperBound,
        ])
        XCTAssertEqual(grammar["function"] as? [String: Int], [
            "min": Voice.ShortcutGrammar.function.lowerBound, "max": Voice.ShortcutGrammar.function.upperBound,
        ])
        XCTAssertEqual(grammar["maxLength"] as? Int, Voice.ShortcutGrammar.maxLength)
        XCTAssertEqual(grammar["labels"] as? [String: String], Voice.ShortcutGrammar.labels)
        XCTAssertEqual(grammar["separator"] as? String, Voice.ShortcutGrammar.separator)
        XCTAssertEqual(fixture["maxChars"] as? Int, Voice.maxChars)
    }

    /// #NEW-73: `languages` and `limits`.
    func testLanguagesAndLimitsMatchTheFixture() throws {
        let fixture = try fixture()
        let languages = try XCTUnwrap(fixture["languages"] as? [[String: String]]).map {
            Voice.Language(value: $0["value"] ?? "", label: $0["label"] ?? "")
        }
        XCTAssertEqual(languages, Voice.languages)
        let limits = try XCTUnwrap(fixture["limits"] as? [String: Int])
        XCTAssertEqual(limits, [
            "voiceFile": Voice.Limits.voiceFile,
            "voiceTranscript": Voice.Limits.voiceTranscript,
            "wakePhrase": Voice.Limits.wakePhrase,
            "wakePhraseMinLetters": Voice.Limits.wakePhraseMinLetters,
        ])
    }

    /// `defaults` decodes into the mirrored row (and `voice.get`'s shape).
    func testDefaultsMatchTheFixture() throws {
        let defaults = try XCTUnwrap(try fixture()["defaults"])
        let data = try JSONSerialization.data(withJSONObject: defaults)
        XCTAssertEqual(try JSONDecoder().decode(Voice.Settings.self, from: data), Voice.defaults)
        XCTAssertEqual(Voice.defaults.pttShortcut, "")
        XCTAssertTrue(Voice.languages.contains { $0.value == Voice.defaults.language })
        XCTAssertEqual(Voice.defaults.voiceFile, "")
        XCTAssertEqual(Voice.defaults.voiceTranscript, "")
    }

    /// A row from a server without the #NEW-73 keys reads their defaults;
    /// the legacy `speakReplies` / `pushToTalk` keys are tolerated, ignored.
    func testSettingsDecodeWithoutTheNewKeys() throws {
        let data = Data(#"{"voiceDeviceId":null,"agentId":null,"speakReplies":true,"pushToTalk":false,"pttShortcut":"","wakeWord":false,"wakePhrase":""}"#.utf8)
        XCTAssertEqual(try JSONDecoder().decode(Voice.Settings.self, from: data), Voice.defaults)
    }

    /// S-3: `shortcutParseCases` (canonical form + label).
    func testShortcutParseCases() throws {
        for testCase in try cases("shortcutParseCases") {
            let text = try XCTUnwrap(testCase["in"] as? String)
            let value = testCase["value"] as? String
            XCTAssertEqual(Voice.shortcutCanonical(text), value, text)
            if let value {
                XCTAssertEqual(Voice.shortcutLabel(value), testCase["label"] as? String, text)
            }
        }
        XCTAssertEqual(Voice.shortcutLabel(""), "None")
    }

    /// S-3: `captureCases` (the push-to-talk recorder).
    func testCaptureCases() throws {
        for testCase in try cases("captureCases") {
            let name = testCase["name"] as? String ?? "?"
            let events = try XCTUnwrap(testCase["events"] as? [[String: Any]], name).map { raw -> Voice.CaptureEvent in
                switch raw["t"] as? String {
                case "down": return .down(key: try XCTUnwrap(raw["key"] as? String, name))
                case "up": return .up(key: try XCTUnwrap(raw["key"] as? String, name))
                default: return .mouse(button: try XCTUnwrap(raw["button"] as? Int, name))
                }
            }
            let state = events.reduce(Voice.captureStart(try XCTUnwrap(testCase["before"] as? String, name)), Voice.captureStep)
            let result = try XCTUnwrap(testCase["result"] as? [String: Any], name)
            XCTAssertEqual(state.status.rawValue, result["status"] as? String, name)
            XCTAssertEqual(state.value, result["value"] as? String, name)
            XCTAssertEqual(Voice.capturePending(state), result["pending"] as? String, name)
            XCTAssertEqual(state.error, result["error"] as? String, name)
        }
    }

    /// S-3: `wakeDetailCases` (the Wake word row's trailing detail ×4).
    func testWakeDetailCases() throws {
        for testCase in try cases("wakeDetailCases") {
            let name = testCase["name"] as? String ?? "?"
            let context = try XCTUnwrap(testCase["context"] as? [String: Any], name)
            let state = try (context["state"] as? String).map { try XCTUnwrap(Voice.WakeState(rawValue: $0), name) }
            XCTAssertEqual(Voice.wakeDetail(Voice.WakeDetailContext(
                wakeWord: try XCTUnwrap(context["wakeWord"] as? Bool, name),
                agent: try XCTUnwrap(context["agent"] as? Bool, name),
                online: try XCTUnwrap(context["online"] as? Bool, name),
                device: context["device"] as? String ?? "",
                state: state
            )), testCase["detail"] as? String, name)
        }
    }

    /// The doctor's `wake` row detail, verbatim (other clients' source).
    func testWakeDoctorDetail() {
        let json = #"{"items":[{"key":"wake","group":"voice","state":"off","detail":"A call is using the microphone"}]}"#
        XCTAssertEqual(Voice.wakeDoctorDetail(ofJSON: json), "A call is using the microphone")
        XCTAssertNil(Voice.wakeDoctorDetail(ofJSON: #"{"items":[{"key":"wake","group":"agents","state":"ok","detail":"x"}]}"#))
        XCTAssertNil(Voice.wakeDoctorDetail(ofJSON: nil))
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

    /// #NEW-75: THE input rule (UTF-16 units, never a split surrogate pair).
    func testVoiceTextInputCases() throws {
        for testCase in try cases("voiceTextInputCases") {
            let text = testCase["in"] as? String ?? ""
            let max = try XCTUnwrap(testCase["max"] as? Int)
            XCTAssertEqual(Voice.textInput(text, max: max), testCase["out"] as? String, text.debugDescription)
        }
    }

    /// #NEW-56: WAKE PHRASE LENGTH.
    func testWakePhraseErrorCases() throws {
        for testCase in try cases("wakePhraseErrorCases") {
            let text = testCase["in"] as? String ?? ""
            XCTAssertEqual(Voice.wakePhraseError(text), testCase["error"] as? String, text)
        }
    }

    func testLanguageNoteCases() throws {
        for testCase in try cases("languageNoteCases") {
            let name = testCase["name"] as? String ?? "?"
            XCTAssertEqual(
                Voice.languageNote(language: testCase["language"] as? String ?? "", deviceName: testCase["device"] as? String),
                testCase["note"] as? String,
                name
            )
        }
    }

    func testLanguageLabelCases() throws {
        for testCase in try cases("languageLabelCases") {
            let name = testCase["name"] as? String ?? "?"
            XCTAssertEqual(
                Voice.languageLabel(language: testCase["language"] as? String ?? "", deviceName: testCase["device"] as? String),
                testCase["label"] as? String,
                name
            )
        }
    }

    /// None travels as an explicit null; an untouched field writes no key.
    func testChangeEncodesNoneAsNull() throws {
        let data = try JSONEncoder().encode(Voice.Change(agentId: .some(nil), wakeWord: true))
        let object = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(Set(object.keys), ["agentId", "wakeWord"])
        XCTAssertTrue(object["agentId"] is NSNull)
        XCTAssertEqual(object["wakeWord"] as? Bool, true)
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
        settings.wakeWord = raw["wakeWord"] as? Bool ?? false
        return settings
    }
}
