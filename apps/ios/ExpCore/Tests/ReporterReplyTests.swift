import Foundation
import XCTest
@testable import ExpCore

// SLOP-4: the reporter comment card + "Reply to reporter" toggle copy, locked
// ×4 (web imports the json, desktop `domain::reporter_reply`, Android
// `ReporterReplyTest`) against the ONE contract fixture.
final class ReporterReplyTests: XCTestCase {
    private struct Fixture: Decodable {
        let copy: [String: String]
        let captions: Captions
        let authorNames: AuthorNames
    }

    private struct Captions: Decodable {
        let viaMcp: String
        let separator: String
        let cases: [CaptionCase]
    }

    private struct CaptionCase: Decodable {
        let source: String
        let audience: String
        let caption: String?
    }

    private struct AuthorNames: Decodable {
        let cases: [AuthorNameCase]
    }

    private struct AuthorNameCase: Decodable {
        let source: String
        let authorId: String?
        let authorSynced: Bool
        let reporterName: String?
        let name: String
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

    func testCaptionsFollowThePinnedOrder() throws {
        let captions = try fixture().captions
        XCTAssertEqual(ReporterReply.viaMcpCaption, captions.viaMcp)
        XCTAssertEqual(ReporterReply.captionSeparator, captions.separator)
        XCTAssertTrue(captions.cases.contains { $0.source == "mcp" && $0.audience == "reporter" })
        for c in captions.cases {
            XCTAssertEqual(
                ReporterReply.caption(source: c.source, audience: c.audience), c.caption,
                "\(c.source)/\(c.audience)"
            )
        }
    }

    func testAuthorNamesFallBackToTheFormerMember() throws {
        for c in try fixture().authorNames.cases {
            let name = ReporterReply.authorNameOverride(
                source: c.source, authorSynced: c.authorSynced, reporterName: c.reporterName
            ) ?? "{author}"
            XCTAssertEqual(name, c.name, "\(c.source)/\(c.authorId ?? "nil")")
        }
    }
}
