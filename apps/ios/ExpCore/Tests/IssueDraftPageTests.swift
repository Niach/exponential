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

        struct DiscardConfirm: Decodable, Equatable {
            let title: String
            let confirm: String
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
            let discard: String
            let untitled: String
            let discardConfirm: DiscardConfirm
            let leave: Leave
        }

        let copy: Copy
        let autosave: Autosave
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
            "discard": IssueDraftPage.discard,
            "untitled": IssueDraftPage.untitled,
            "discardConfirm.title": IssueDraftPage.DiscardConfirm.title,
            "discardConfirm.confirm": IssueDraftPage.DiscardConfirm.confirm,
            "leave.title": IssueDraftPage.Leave.title,
            "leave.create": IssueDraftPage.Leave.create,
            "leave.keep": IssueDraftPage.Leave.keep,
            "leave.discard": IssueDraftPage.Leave.discard,
        ]
        let expected: [String: String] = [
            "header": copy.header,
            "titlePlaceholder": copy.titlePlaceholder,
            "descriptionPlaceholder": copy.descriptionPlaceholder,
            "create": copy.create,
            "discard": copy.discard,
            "untitled": copy.untitled,
            "discardConfirm.title": copy.discardConfirm.title,
            "discardConfirm.confirm": copy.discardConfirm.confirm,
            "leave.title": copy.leave.title,
            "leave.create": copy.leave.create,
            "leave.keep": copy.leave.keep,
            "leave.discard": copy.leave.discard,
        ]
        XCTAssertEqual(mirrored, expected)
        XCTAssertEqual(Set(mirrored.keys), try fixtureCopyKeys())
    }

    // EXP-1212: which prompt an exit raises.

    func testNoContentNeverAsks() {
        for exit in [IssueDraftPage.Exit.discard, .leave, .own] {
            XCTAssertEqual(IssueDraftPage.prompt(for: exit, hasContent: false), .none)
        }
    }

    func testContentAsksOnDiscardAndLeave() {
        XCTAssertEqual(IssueDraftPage.prompt(for: .discard, hasContent: true), .discardConfirm)
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
                for: .discard,
                hasContent: IssueDraftPage.hasContent(
                    title: "", description: "", attachmentCount: 0, attachmentsKnown: false
                )
            ),
            .discardConfirm
        )
        // Known and empty (a brand-new draft): nothing to ask.
        XCTAssertFalse(IssueDraftPage.hasContent(
            title: "", description: "", attachmentCount: 0, attachmentsKnown: true
        ))
    }

    func testLeaveChoices() {
        XCTAssertEqual(IssueDraftPage.leaveChoices(canKeep: true), [.create, .keep, .discard])
        // A sub-issue draft never writes a row: nothing to keep.
        XCTAssertEqual(IssueDraftPage.leaveChoices(canKeep: false), [.create, .discard])
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
}
