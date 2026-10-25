import Foundation
import XCTest
@testable import ExpCore

// NEW-53: the Voice group of Device settings, locked ×4 (TS `voice.ts`,
// desktop `domain::voice`, Android `VoiceTest`) against fixtures/voice.json:
// every `copy` key, the `testIds` and `assistantOptionsCases`.
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

    func testCopyMatchesTheFixture() throws {
        let copy = try XCTUnwrap(try fixture()["copy"] as? [String: String])
        let mirrored: [String: String] = [
            "group": Voice.Copy.group,
            "assistant": Voice.Copy.assistant,
            "assistantNone": Voice.Copy.assistantNone,
            "speakReplies": Voice.Copy.speakReplies,
            "downloading": Voice.Copy.downloading,
            "downloadFailed": Voice.Copy.downloadFailed,
            "speaking": Voice.Copy.speaking,
            "stop": Voice.Copy.stop,
            "link": Voice.Copy.link,
            "ellipsis": Voice.Copy.ellipsis,
        ]
        XCTAssertEqual(copy, mirrored)
    }

    func testTestIdsAndMaxCharsMatchTheFixture() throws {
        let fixture = try fixture()
        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds, [
            "group": Voice.TestIds.group,
            "assistant": Voice.TestIds.assistant,
            "speakReplies": Voice.TestIds.speakReplies,
        ])
        XCTAssertEqual(fixture["maxChars"] as? Int, Voice.maxChars)
    }

    func testAssistantOptionsCases() throws {
        let cases = try XCTUnwrap(try fixture()["assistantOptionsCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? "?"
            let agents = try XCTUnwrap(testCase["agents"] as? [[String: String]]).map {
                Voice.AgentLike(id: $0["id"] ?? "", name: $0["name"] ?? "", team: $0["team"] ?? "")
            }
            let expected = try XCTUnwrap(testCase["options"] as? [[String: Any]]).map {
                Voice.AssistantOption(id: $0["id"] as? String, label: $0["label"] as? String ?? "")
            }
            XCTAssertEqual(Voice.assistantOptions(agents), expected, name)
        }
    }
}
