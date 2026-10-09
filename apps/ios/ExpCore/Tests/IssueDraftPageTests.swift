import Foundation
import XCTest
@testable import ExpCore

// EXP-1170: the New issue page's copy + autosave debounce, locked ×4 (web
// `issue-draft-page.test.ts`, Android `IssueDraftPageTest`, desktop
// `domain::issue_draft`) against the ONE contract fixture, plus EXP-1212's
// discard/leave prompts and the pure rule deciding which one an exit raises.
final class IssueDraftPageTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Autosave: Decodable {
            let debounceMs: Double
        }

        struct Leave: Decodable, Equatable {
            let title: String
            let create: String
            let keep: String
            let discard: String
        }

        struct Copy: Decodable {
            let header: String
            let titlePlaceholder: String
            let descriptionPlaceholder: String
            let create: String
            let untitled: String
            let leave: Leave
            let discardedElsewhere: String
        }

        struct Concurrency: Decodable {
            let discardedGraceMs: Double
        }

        let copy: Copy
        let autosave: Autosave
        let concurrency: Concurrency
    }

    private var fixtureURL: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/issue-draft.json")
    }

    private func fixture() throws -> Fixture {
        try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: fixtureURL))
    }

    /// The raw fixture's `copy` keys, so a key added there without a mirror
    /// here fails instead of being silently ignored by `Decodable`.
    private func fixtureCopyKeys() throws -> Set<String> {
        let root = try JSONSerialization.jsonObject(with: try Data(contentsOf: fixtureURL)) as? [String: Any]
        let copy = root?["copy"] as? [String: Any] ?? [:]
        var keys = Set<String>()
        for (key, value) in copy {
            if let nested = value as? [String: Any] {
                keys.formUnion(nested.keys.map { "\(key).\($0)" })
            } else {
                keys.insert(key)
            }
        }
        return keys
    }

    func testCopy() throws {
        let copy = try fixture().copy
        let mirrored: [String: String] = [
            "header": IssueDraftPage.header,
            "titlePlaceholder": IssueDraftPage.titlePlaceholder,
            "descriptionPlaceholder": IssueDraftPage.descriptionPlaceholder,
            "create": IssueDraftPage.create,
            "untitled": IssueDraftPage.untitled,
            "leave.title": IssueDraftPage.Leave.title,
            "leave.create": IssueDraftPage.Leave.create,
            "leave.keep": IssueDraftPage.Leave.keep,
            "leave.discard": IssueDraftPage.Leave.discard,
            "discardedElsewhere": IssueDraftPage.discardedElsewhere,
        ]
        let expected: [String: String] = [
            "header": copy.header,
            "titlePlaceholder": copy.titlePlaceholder,
            "descriptionPlaceholder": copy.descriptionPlaceholder,
            "create": copy.create,
            "untitled": copy.untitled,
            "leave.title": copy.leave.title,
            "leave.create": copy.leave.create,
            "leave.keep": copy.leave.keep,
            "leave.discard": copy.leave.discard,
            "discardedElsewhere": copy.discardedElsewhere,
        ]
        XCTAssertEqual(mirrored, expected)
        XCTAssertEqual(Set(mirrored.keys), try fixtureCopyKeys())
        // EXP-1247: the close button and its confirm are gone ×4.
        XCTAssertFalse(try fixtureCopyKeys().contains("discard"))
        XCTAssertFalse(try fixtureCopyKeys().contains("discardConfirm.title"))
    }

    // EXP-1212: which prompt an exit raises.

    func testNoContentNeverAsks() {
        for exit in [IssueDraftPage.Exit.leave, .own] {
            XCTAssertEqual(IssueDraftPage.prompt(for: exit, hasContent: false), .none)
        }
    }

    func testContentAsksOnLeave() {
        XCTAssertEqual(IssueDraftPage.prompt(for: .leave, hasContent: true), .leave)
    }

    func testOwnExitsNeverAsk() {
        XCTAssertEqual(IssueDraftPage.prompt(for: .own, hasContent: true), .none)
    }

    func testHasContent() {
        XCTAssertFalse(IssueDraftPage.hasContent(title: "", description: "", attachmentCount: 0))
        XCTAssertFalse(IssueDraftPage.hasContent(title: "  \n", description: " \t", attachmentCount: 0))
        XCTAssertTrue(IssueDraftPage.hasContent(title: "Fix it", description: "", attachmentCount: 0))
        XCTAssertTrue(IssueDraftPage.hasContent(title: "", description: "Steps", attachmentCount: 0))
        XCTAssertTrue(IssueDraftPage.hasContent(title: "", description: "", attachmentCount: 1))
    }

    func testUnknownFileListCountsAsContent() {
        // A reopened attachment-only draft before its file list loads.
        XCTAssertTrue(IssueDraftPage.hasContent(
            title: "", description: "", attachmentCount: 0, attachmentsKnown: false
        ))
        XCTAssertEqual(
            IssueDraftPage.prompt(
                for: .leave,
                hasContent: IssueDraftPage.hasContent(
                    title: "", description: "", attachmentCount: 0, attachmentsKnown: false
                )
            ),
            .leave
        )
        // Known and empty (a brand-new draft): nothing to ask.
        XCTAssertFalse(IssueDraftPage.hasContent(
            title: "", description: "", attachmentCount: 0, attachmentsKnown: true
        ))
    }

    func testLeaveChoices() {
        XCTAssertEqual(IssueDraftPage.leaveChoices(canKeep: true), [.discard, .keep, .create])
        // A sub-issue draft never writes a row: nothing to keep.
        XCTAssertEqual(IssueDraftPage.leaveChoices(canKeep: false), [.discard, .create])
    }

    func testLeaveDefaultIsCreateElseKeep() {
        XCTAssertEqual(IssueDraftPage.leaveDefault(canKeep: true, createEnabled: true), .create)
        XCTAssertEqual(IssueDraftPage.leaveDefault(canKeep: false, createEnabled: true), .create)
        // Create disabled (no title): Save draft takes the default.
        XCTAssertEqual(IssueDraftPage.leaveDefault(canKeep: true, createEnabled: false), .keep)
        // Never Discard.
        XCTAssertNil(IssueDraftPage.leaveDefault(canKeep: false, createEnabled: false))
    }

    func testLeaveCreateNeedsATitle() {
        XCTAssertFalse(IssueDraftPage.leaveCreateEnabled(title: ""))
        XCTAssertFalse(IssueDraftPage.leaveCreateEnabled(title: "   "))
        XCTAssertTrue(IssueDraftPage.leaveCreateEnabled(title: "Fix it"))
        XCTAssertFalse(IssueDraftPage.leaveCreateEnabled(title: "Fix it", creating: true))
    }

    func testAutosaveDebounce() throws {
        XCTAssertEqual(IssueDraftPage.autosaveDebounceMs, try fixture().autosave.debounceMs)
    }

    // EXP-1231: draft concurrency.

    func testDiscardedGrace() throws {
        XCTAssertEqual(IssueDraftPage.discardedGraceMs, try fixture().concurrency.discardedGraceMs)
    }

    func testFate() {
        // Created elsewhere wins over everything.
        for seen in [false, true] {
            for present in [false, true] {
                XCTAssertEqual(
                    IssueDraftPage.fate(seen: seen, present: present, createdIssueId: "i1"),
                    .created(issueId: "i1")
                )
            }
        }
        // A row never seen is never gone (a brand-new page's first write).
        XCTAssertEqual(IssueDraftPage.fate(seen: false, present: false, createdIssueId: nil), .open)
        XCTAssertEqual(IssueDraftPage.fate(seen: true, present: true, createdIssueId: nil), .open)
        // Seen, then gone with no issue made from it.
        XCTAssertEqual(IssueDraftPage.fate(seen: true, present: false, createdIssueId: nil), .gone)
    }
}
