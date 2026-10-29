import Foundation
import XCTest
@testable import ExpCore

// NEW-57: meeting mode, locked ×4 (TS `meeting.ts`, desktop
// `domain::meeting`, Android `MeetingTest`) against fixtures/meeting.json:
// every `copy` key, the `testIds`, `desktopAudioValues`, `limits`, and every
// `desktopAudioOptionCases`, `appOptionCases`, `whoCases` and `consentCases`
// row (channels, flush and the message are desktop-only). Plus
// `contract.meeting.retentionDays` and the wire: the settings' new keys and
// the doctor's `desktopAudio`.
final class MeetingTests: XCTestCase {
    private static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()          // ExpCore/Tests/
        .deletingLastPathComponent()          // ExpCore/
        .deletingLastPathComponent()          // apps/ios/
        .deletingLastPathComponent()          // apps/
        .deletingLastPathComponent()          // the repo root

    private func json(_ path: String) throws -> [String: Any] {
        let url = Self.root.appendingPathComponent(path)
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func fixture() throws -> [String: Any] {
        try json("packages/domain-contract/fixtures/meeting.json")
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let cases = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(cases.isEmpty, key)
        return cases
    }

    private func apps(_ raw: Any?) throws -> [Meeting.AudioApp] {
        try XCTUnwrap(raw as? [[String: String]]).map {
            Meeting.AudioApp(id: $0["id"] ?? "", name: $0["name"] ?? "")
        }
    }

    private func capability(_ raw: Any?) throws -> Meeting.Capability {
        try XCTUnwrap(Meeting.Capability(rawValue: raw as? String ?? ""))
    }

    func testCopyAndTestIdsMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["copy"] as? [String: String], [
            "meeting": Meeting.Copy.meeting,
            "desktopAudio": Meeting.Copy.desktopAudio,
            "apps": Meeting.Copy.apps,
            "appsNone": Meeting.Copy.appsNone,
            "you": Meeting.Copy.you,
            "desktop": Meeting.Copy.desktop,
            "whoAll": Meeting.Copy.whoAll,
            "whoExcept": Meeting.Copy.whoExcept,
            "and": Meeting.Copy.and,
            "consentTitle": Meeting.Copy.consentTitle,
            "consentBody": Meeting.Copy.consentBody,
            "sharedRefused": Meeting.Copy.sharedRefused,
            "start": Meeting.Copy.start,
            "cancel": Meeting.Copy.cancel,
            "pause": Meeting.Copy.pause,
            "doctorLabel": Meeting.Copy.doctorLabel,
            "doctorPerApp": Meeting.Copy.doctorPerApp,
            "doctorSystem": Meeting.Copy.doctorSystem,
            "doctorNone": Meeting.Copy.doctorNone,
            "autoStopped": Meeting.Copy.autoStopped,
            "autoStopAfter": Meeting.Copy.autoStopAfter,
            "autoStopHour": Meeting.Copy.autoStopHour,
            "autoStopHours": Meeting.Copy.autoStopHours,
            "restartStopped": Meeting.Copy.restartStopped,
            "messageHeader": Meeting.Copy.messageHeader,
        ])
        XCTAssertEqual(fixture["testIds"] as? [String: String], [
            "meeting": Meeting.TestIds.meeting,
            "desktopAudio": Meeting.TestIds.desktopAudio,
            "apps": Meeting.TestIds.apps,
            "consentStart": Meeting.TestIds.consentStart,
            "autoStop": Meeting.TestIds.autoStop,
        ])
        XCTAssertEqual(fixture["limits"] as? [String: Int], [
            "apps": Meeting.Limits.apps,
            "appName": Meeting.Limits.appName,
        ])
        let values = try XCTUnwrap(fixture["desktopAudioValues"] as? [[String: String]])
        XCTAssertEqual(values.map { $0["value"] ?? "" }, Meeting.DesktopAudio.allCases.map(\.rawValue))
        XCTAssertEqual(values.map { $0["label"] ?? "" }, Meeting.DesktopAudio.allCases.map(\.label))
    }

    func testRetentionDaysMatchTheContract() throws {
        let meeting = try XCTUnwrap(try json("packages/domain-contract/contract.json")["meeting"] as? [String: Any])
        XCTAssertEqual(meeting["retentionDays"] as? Int, Meeting.retentionDays)
    }

    func testDesktopAudioOptionCases() throws {
        for row in try cases("desktopAudioOptionCases") {
            let name = row["name"] as? String ?? "?"
            XCTAssertEqual(
                Meeting.desktopAudioOptions(try capability(row["capability"])).map(\.rawValue),
                row["expect"] as? [String],
                name
            )
        }
    }

    func testAppOptionCases() throws {
        for row in try cases("appOptionCases") {
            let name = row["name"] as? String ?? "?"
            let expect = try XCTUnwrap(row["expect"] as? [[String: String]]).map {
                Meeting.AppOption(value: $0["value"] ?? "", label: $0["label"] ?? "")
            }
            XCTAssertEqual(
                Meeting.appOptions(live: try apps(row["live"]), stored: try apps(row["stored"])),
                expect,
                name
            )
        }
    }

    func testWhoCases() throws {
        for row in try cases("whoCases") {
            let name = row["name"] as? String ?? "?"
            let desktopAudio = try XCTUnwrap(Meeting.DesktopAudio(rawValue: row["desktopAudio"] as? String ?? ""), name)
            XCTAssertEqual(
                Meeting.who(desktopAudio, apps: try apps(row["apps"]), capability: try capability(row["capability"])),
                row["expect"] as? String,
                name
            )
        }
    }

    func testConsentCases() throws {
        for row in try cases("consentCases") {
            let name = row["name"] as? String ?? "?"
            XCTAssertEqual(
                Meeting.consentBody(
                    who: row["who"] as? String ?? "",
                    agent: row["agent"] as? String ?? "",
                    days: row["days"] as? Int ?? 0
                ),
                row["expect"] as? String,
                name
            )
        }
    }

    // MARK: - #FEED-163 auto-stop

    func testAutoStopMatchesTheFixture() throws {
        let autoStop = try XCTUnwrap(try fixture()["autoStop"] as? [String: Any])
        XCTAssertEqual(autoStop["afterMs"] as? Int, Meeting.AutoStop.afterMs)
        XCTAssertEqual(autoStop["defaultHours"] as? Int, Meeting.AutoStop.defaultHours)
        XCTAssertEqual(autoStop["hoursValues"] as? [Int], Meeting.AutoStop.hoursValues)
    }

    func testAutoStopCases() throws {
        for row in try cases("autoStopCases") {
            let name = row["name"] as? String ?? "?"
            let startedAt = row["startedAt"] as? String
            let nowMs = try XCTUnwrap((row["nowMs"] as? NSNumber)?.int64Value, name)
            let got = if let hours = row["hours"] as? Int {
                Meeting.meetingExpired(startedAt: startedAt, nowMs: nowMs, hours: hours)
            } else {
                Meeting.meetingExpired(startedAt: startedAt, nowMs: nowMs)
            }
            XCTAssertEqual(got, row["expect"] as? Bool, name)
        }
    }

    func testAutoStoppedCases() throws {
        for row in try cases("autoStoppedCases") {
            XCTAssertEqual(Meeting.autoStopped(hours: row["hours"] as? Int ?? 0), row["expect"] as? String)
        }
    }

    func testAutoStopOptionCases() throws {
        for row in try cases("autoStopOptionCases") {
            let name = row["name"] as? String ?? "?"
            let expect = try XCTUnwrap(row["expect"] as? [[String: Any]], name).map {
                Meeting.AutoStopOption(value: $0["value"] as? Int ?? 0, label: $0["label"] as? String ?? "")
            }
            XCTAssertEqual(Meeting.autoStopOptions(), expect, name)
        }
    }

    func testStopNoteCases() throws {
        for row in try cases("stopNoteCases") {
            let name = row["name"] as? String ?? "?"
            XCTAssertEqual(
                Meeting.stopNote(reason: row["reason"] as? String, hours: row["hours"] as? Int ?? 0),
                row["expect"] as? String,
                name
            )
        }
    }

    func testAppsValue() {
        XCTAssertEqual(Meeting.appsValue([]), Meeting.Copy.appsNone)
        XCTAssertEqual(
            Meeting.appsValue([Meeting.AudioApp(id: "z", name: "Zoom"), Meeting.AudioApp(id: "s", name: "Slack")]),
            "Zoom, Slack"
        )
    }

    // MARK: - Wire

    func testSettingsDecodeTheMeetingKeys() throws {
        let data = Data(#"{"voiceDeviceId":"d","agentId":"a","desktopAudio":"include","desktopAudioApps":[{"id":"z","name":"Zoom"}],"meetingStartedAt":"2026-10-28T10:00:00.000Z"}"#.utf8)
        let settings = try JSONDecoder().decode(Voice.Settings.self, from: data)
        XCTAssertEqual(settings.desktopAudio, .include)
        XCTAssertEqual(settings.desktopAudioApps, [Meeting.AudioApp(id: "z", name: "Zoom")])
        XCTAssertTrue(settings.meetingOn)
        // Absent keys (an older server) and an unknown value read the defaults.
        let old = try JSONDecoder().decode(Voice.Settings.self, from: Data(#"{"desktopAudio":"later"}"#.utf8))
        XCTAssertEqual(old.desktopAudio, .off)
        XCTAssertEqual(old.desktopAudioApps, [])
        XCTAssertFalse(old.meetingOn)
        // #FEED-163: Stop after reads its default when absent, the stored value otherwise.
        XCTAssertEqual(old.meetingAutoStopHours, Meeting.AutoStop.defaultHours)
        let eight = try JSONDecoder().decode(Voice.Settings.self, from: Data(#"{"meetingAutoStopHours":8}"#.utf8))
        XCTAssertEqual(eight.meetingAutoStopHours, 8)
    }

    func testChangeEncodesTheMeetingKeys() throws {
        let change = Voice.Change(desktopAudio: .exclude, desktopAudioApps: [Meeting.AudioApp(id: "sp", name: "Spotify")])
        let object = try XCTUnwrap(try JSONSerialization.jsonObject(with: try JSONEncoder().encode(change)) as? [String: Any])
        XCTAssertEqual(Set(object.keys), ["desktopAudio", "desktopAudioApps"])
        XCTAssertEqual(object["desktopAudio"] as? String, "exclude")
        XCTAssertEqual(object["desktopAudioApps"] as? [[String: String]], [["id": "sp", "name": "Spotify"]])
        let hours = try XCTUnwrap(try JSONSerialization.jsonObject(
            with: try JSONEncoder().encode(Voice.Change(meetingAutoStopHours: 1))
        ) as? [String: Any])
        XCTAssertEqual(Set(hours.keys), ["meetingAutoStopHours"])
        XCTAssertEqual(hours["meetingAutoStopHours"] as? Int, 1)
        let before = Voice.Settings(voiceDeviceId: "d", agentId: "a")
        XCTAssertEqual(Voice.clearOnNone(before, Voice.Change(meetingAutoStopHours: 2)).meetingAutoStopHours, 2)
    }

    /// Choosing None for the device or the agent stops the meeting.
    func testClearOnNoneStopsTheMeeting() {
        let before = Voice.Settings(voiceDeviceId: "d", agentId: "a", meetingStartedAt: "2026-10-28T10:00:00.000Z")
        XCTAssertNil(Voice.clearOnNone(before, Voice.Change(agentId: .some(nil))).meetingStartedAt)
        XCTAssertNotNil(Voice.clearOnNone(before, Voice.Change(wakeWord: true)).meetingStartedAt)
    }

    func testDoctorDesktopAudioDecodesLeniently() {
        let full = DeviceDoctor.decode(json: #"{"items":[],"desktopAudio":{"capability":"per_app","apps":[{"id":"z","name":"Zoom"},{"id":7}]}}"#)
        XCTAssertEqual(full?.desktopAudio, Meeting.DoctorAudio(capability: .perApp, apps: [Meeting.AudioApp(id: "z", name: "Zoom")]))
        let unknown = DeviceDoctor.decode(json: #"{"items":[],"desktopAudio":{"capability":"quantum"}}"#)
        XCTAssertEqual(unknown?.desktopAudio, Meeting.DoctorAudio.absent)
        let absent = DeviceDoctor.decode(json: #"{"items":[]}"#)
        XCTAssertNotNil(absent)
        XCTAssertNil(absent?.desktopAudio)
        let broken = DeviceDoctor.decode(json: #"{"items":[{"key":"k","group":"g","state":"ok"}],"desktopAudio":"yes"}"#)
        XCTAssertEqual(broken?.items.count, 1)
        XCTAssertNil(broken?.desktopAudio)
    }
}
