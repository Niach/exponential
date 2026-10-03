import Foundation
import XCTest
@testable import ExpCore

// SLOP-4: the reporter comment card + "Reply to reporter" toggle copy, locked
// ×4 (web imports the json, desktop `domain::reporter_reply`, Android
// `ReporterReplyTest`) against the ONE contract fixture.
final class ReporterReplyTests: XCTestCase {
    private struct Fixture: Decodable {
        let copy: [String: String]
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/reporter-reply.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testCopy() throws {
        let mirrored: [String: String] = [
            "toggleLabel": ReporterReply.toggleLabel,
            "placeholderOn": ReporterReply.placeholderOn,
            "anonymousName": ReporterReply.anonymousName,
            "formerMemberName": ReporterReply.formerMemberName,
            "reporterCaption": ReporterReply.reporterCaption,
            "toReporterCaption": ReporterReply.toReporterCaption,
            "sentToast": ReporterReply.sentToast,
            "notSentToast": ReporterReply.notSentToast,
            "notificationLabel": ReporterReply.notificationLabel,
        ]
        XCTAssertEqual(mirrored, try fixture().copy)
    }

    func testPlaceholderFillsTheReporterName() {
        XCTAssertEqual(ReporterReply.placeholder(name: "Emma"), "Reply to Emma… (emailed to them)")
        XCTAssertEqual(ReporterReply.placeholder(name: "  "), "Reply to Anonymous visitor… (emailed to them)")
        XCTAssertEqual(ReporterReply.placeholder(name: nil), "Reply to Anonymous visitor… (emailed to them)")
    }

    func testReporterNameFallsBackToTheAnonymousVisitor() {
        XCTAssertEqual(ReporterReply.reporterName(" Emma Fischer "), "Emma Fischer")
        XCTAssertEqual(ReporterReply.reporterName(""), ReporterReply.anonymousName)
        XCTAssertEqual(ReporterReply.reporterName(nil), ReporterReply.anonymousName)
    }
}
