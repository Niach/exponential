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

        /// EXP-1272: which prompt an exit raises, reopened or fresh.
        struct ExitPromptCase: Decodable {
            let `case`: String
            let hasContent: Bool
            let reopened: Bool
            let prompt: String
        }

        /// EXP-1272: the phones' Inbox tab dot.
        struct InboxDotCase: Decodable {
            let `case`: String
            let unread: Int
            let drafts: Int
            let lit: Bool
        }

        let copy: Copy
        let autosave: Autosave
        let concurrency: Concurrency
        let exitPrompt: [ExitPromptCase]
        let inboxDot: [InboxDotCase]
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

    // EXP-1272: a reopened draft saves and leaves; only a fresh one asks.

    func testExitPromptTable() throws {
        let cases = try fixture().exitPrompt
        XCTAssertFalse(cases.isEmpty)
        for row in cases {
            let expected: IssueDraftPage.Prompt
            switch row.prompt {
            case "none": expected = .none
            case "leave": expected = .leave
            case "keep": expected = .keep
            default:
                XCTFail("unknown prompt \(row.prompt) in \(row.case)")
                continue
            }
            XCTAssertEqual(
                IssueDraftPage.prompt(for: .leave, hasContent: row.hasContent, reopened: row.reopened),
                expected,
                row.case
            )
            // The page's own exits never ask, reopened or not.
            XCTAssertEqual(
                IssueDraftPage.prompt(for: .own, hasContent: row.hasContent, reopened: row.reopened),
                .none,
                row.case
            )
        }
    }

    // #EXP-2125: the board picker over every team, and what a pick does.

    private func rows(_ key: String) throws -> [[String: Any]] {
        let root = try JSONSerialization.jsonObject(with: try Data(contentsOf: fixtureURL)) as? [String: Any]
        return try XCTUnwrap(root?[key] as? [[String: Any]])
    }

    func testBoards() throws {
        for c in try rows("boards") {
            let teams = try XCTUnwrap(c["teams"] as? [[String: String]]).map {
                (id: $0["id"] ?? "", name: $0["name"] ?? "", kind: $0["kind"] ?? "")
            }
            let got = IssueDraftPage.boards(
                teams: teams, boards: try XCTUnwrap(c["boards"] as? [[String: Any]]), only: c["only"] as? String
            ) { ($0["id"] as! String, $0["teamId"] as! String, ($0["sortOrder"] as? NSNumber)?.doubleValue) }
            let expected = try XCTUnwrap(c["expected"] as? [String: Any])
            XCTAssertEqual(got.boards.map { $0["id"] as? String }, expected["boards"] as? [String], "\(c["case"] ?? "")")
            XCTAssertEqual(got.showTeam, expected["showTeam"] as? Bool, "\(c["case"] ?? "")")
        }
    }

    func testPicks() throws {
        func fields(_ row: [String: Any]) throws -> IssueDraftPage.PickFields {
            .init(
                teamId: try XCTUnwrap(row["teamId"] as? String), boardId: try XCTUnwrap(row["boardId"] as? String),
                statusId: row["statusId"] as? String, labelIds: try XCTUnwrap(row["labelIds"] as? [String]),
                assigneeId: row["assigneeId"] as? String, assigneeAgentId: row["assigneeAgentId"] as? String
            )
        }
        for c in try rows("picks") {
            let board = try XCTUnwrap(c["board"] as? [String: String])
            let got = IssueDraftPage.pick(
                try fields(try XCTUnwrap(c["draft"] as? [String: Any])),
                board: (board["id"] ?? "", board["teamId"] ?? ""), soleMemberId: c["soleMemberId"] as? String
            )
            XCTAssertEqual(got, try fields(try XCTUnwrap(c["expected"] as? [String: Any])), "\(c["case"] ?? "")")
        }
    }

    /// A default board: ties by id, never the order the boards arrived in.
    func testDefaultBoardTie() {
        XCTAssertEqual(
            AppIntentsModel.board(
                boards: [.init(id: "b-2", sortOrder: 0), .init(id: "b-1", sortOrder: 0)], lastBoardId: nil, pickedId: nil
            ),
            "b-1"
        )
    }

    func testInboxDotTable() throws {
        let cases = try fixture().inboxDot
        XCTAssertFalse(cases.isEmpty)
        for row in cases {
            XCTAssertEqual(IssueDraftPage.inboxDotLit(unread: row.unread, drafts: row.drafts), row.lit, row.case)
        }
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

    /// EXP-1259: type a title, clear it, leave = no row left behind. The
    /// emptied page deletes on the way out whenever it may have written the
    /// row (even an upsert with no answer), and the store hold never blocks
    /// that delete; only an upsert waits for it.
    func testTheWriteAnEmptiedPageMakes() {
        func write(needsRow: Bool = false, leaving: Bool = true, known: Bool = true, mayHaveRow: Bool = true, held: Bool = false) -> IssueDraftPage.Write {
            IssueDraftPage.write(
                needsRow: needsRow, leaving: leaving, attachmentsKnown: known, mayHaveRow: mayHaveRow, held: held
            )
        }
        XCTAssertEqual(write(), .delete)
        XCTAssertEqual(write(held: true), .delete)
        // Mid-edit an empty field is a moment between two thoughts.
        XCTAssertEqual(write(leaving: false), .none)
        // A file list not known yet may hold files.
        XCTAssertEqual(write(known: false), .none)
        // Nothing was ever written: nothing to delete.
        XCTAssertEqual(write(mayHaveRow: false), .none)
        // Content saves, except under the hold (it could resurrect a draft
        // discarded elsewhere).
        XCTAssertEqual(write(needsRow: true), .upsert)
        XCTAssertEqual(write(needsRow: true, leaving: false, mayHaveRow: false), .upsert)
        XCTAssertEqual(write(needsRow: true, held: true), .none)
    }
}
