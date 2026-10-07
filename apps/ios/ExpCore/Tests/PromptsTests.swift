import Foundation
import XCTest
@testable import ExpCore

// EXP-1215: every confirm/choice prompt's wording, locked against the ONE
// contract fixture `domain-contract/fixtures/prompts.json` (web
// `prompts.test.ts`, Android `PromptsTest`): the entry key set, every
// title/body variant, the params, the actions (id, label, role, display
// order) and the focus. A key added to the fixture without a mirror here
// fails instead of being silently ignored.
final class PromptsTests: XCTestCase {
    private var fixtureURL: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/prompts.json")
    }

    private func fixture() throws -> [String: Any] {
        let root = try JSONSerialization.jsonObject(with: try Data(contentsOf: fixtureURL))
        return try XCTUnwrap(root as? [String: Any])
    }

    private func prompts() throws -> [String: [String: Any]] {
        try XCTUnwrap(try fixture()["prompts"] as? [String: [String: Any]])
    }

    /// Keys that explain an entry (not copy a card shows).
    private let structuralKeys: Set<String> = ["params", "actions", "focus", "variants", "slot"]

    func testRoles() throws {
        let roles = try XCTUnwrap(try fixture()["roles"] as? [String])
        XCTAssertEqual(PromptRole.allCases.map(\.rawValue), roles)
    }

    func testEntryKeySet() throws {
        let ids = Prompts.all.map(\.id)
        XCTAssertEqual(Set(ids).count, ids.count, "an entry is mirrored twice")
        XCTAssertEqual(Set(ids), Set(try prompts().keys))
    }

    func testEveryEntryMatchesTheFixture() throws {
        let fixture = try prompts()
        for entry in Prompts.all {
            let raw = try XCTUnwrap(fixture[entry.id], entry.id)

            // Every title*/body* string, and no other copy key.
            var texts: [String: String] = [:]
            for (key, value) in raw where !structuralKeys.contains(key) {
                texts[key] = try XCTUnwrap(value as? String, "\(entry.id).\(key)")
            }
            XCTAssertEqual(entry.texts, texts, entry.id)

            let params = try XCTUnwrap(raw["params"] as? [[String: String]], entry.id)
            XCTAssertEqual(entry.params, params.compactMap { $0["name"] }, entry.id)

            let actions = try XCTUnwrap(raw["actions"] as? [[String: String]], entry.id)
            XCTAssertEqual(
                entry.actions.map { [$0.id, $0.label, $0.role.rawValue] },
                actions.map { [$0["id"] ?? "", $0["label"] ?? "", $0["role"] ?? ""] },
                entry.id
            )
            XCTAssertEqual(entry.focus, raw["focus"] as? String, entry.id)
            XCTAssertTrue(entry.actions.contains { $0.id == entry.focus }, entry.id)
        }
    }

    // The fixture's rules every entry keeps.

    func testFocusNeverLandsOnADestructiveAnswer() {
        for entry in Prompts.all {
            let focused = entry.actions.first { $0.id == entry.focus }
            XCTAssertNotEqual(focused?.role, .destructive, entry.id)
            XCTAssertNotEqual(focused?.role, .quietDestructive, entry.id)
        }
    }

    func testNoCurlyQuotesOrEmDashes() {
        for entry in Prompts.all {
            for text in Array(entry.texts.values) + entry.actions.map(\.label) {
                XCTAssertFalse(text.contains { "\u{201C}\u{201D}\u{2018}\u{2019}\u{2014}".contains($0) }, text)
            }
        }
    }

    // Filling the variants.

    func testFillsTheParams() {
        let copy = Prompts.MoveIssue.copy(identifier: "EXP-12", board: "Mobile")
        XCTAssertEqual(copy.title, "Move EXP-12 to \"Mobile\"?")
        XCTAssertEqual(copy.body, "It gets a new identifier in that board.")
        XCTAssertEqual(copy.focus, "move")
        XCTAssertEqual(Prompts.DeleteAction.copy(name: "Triage").title, "Delete \"Triage\"?")
        XCTAssertNil(Prompts.StopRun.copy().body)
        XCTAssertNil(Prompts.RemoveRepository.copy(fullName: "a/b").body)
    }

    // A value is inserted verbatim in ONE pass: a user-typed name holding
    // another `{param}` is never filled again, whatever the dictionary order;
    // an unnamed placeholder stays.
    func testFillIsASinglePass() {
        XCTAssertEqual(
            Prompts.fill("Delete \"{name}\"?", ["name": "{server}", "server": "Cloud"]),
            "Delete \"{server}\"?"
        )
        XCTAssertEqual(
            Prompts.fill("Delete \"{name}\"?", ["server": "Cloud", "name": "{server}"]),
            "Delete \"{server}\"?"
        )
        XCTAssertEqual(
            Prompts.DeleteTeam.copy(name: "{name}").title, "Delete \"{name}\"?"
        )
        XCTAssertEqual(Prompts.fill("Move {identifier} to {board}?", ["board": "Mobile"]), "Move {identifier} to Mobile?")
        XCTAssertEqual(Prompts.fill("{a}{a} {b}", ["a": "x", "b": "{"]), "xx {")
        XCTAssertEqual(Prompts.fill("no params", ["a": "x"]), "no params")
        XCTAssertEqual(Prompts.fill("{a}", [:]), "{a}")
    }

    func testCountVariants() {
        XCTAssertEqual(Prompts.DeleteIssues.copy(count: 1).title, "Delete 1 issue?")
        XCTAssertEqual(Prompts.DeleteIssues.copy(count: 1).body, "Its comments and files are deleted with it.")
        XCTAssertEqual(Prompts.DeleteIssues.copy(count: 3).title, "Delete 3 issues?")
        XCTAssertEqual(Prompts.DeleteIssues.copy(count: 3).body, "Their comments and files are deleted with them.")
    }

    func testMergeVariants() {
        XCTAssertEqual(Prompts.MergeIssuePr.copy(number: 42, issueCount: 1).title, "Merge PR #42?")
        XCTAssertEqual(Prompts.MergeIssuePr.copy(number: 42, issueCount: 1).body, "It is squash-merged.")
        XCTAssertEqual(
            Prompts.MergeIssuePr.copy(number: nil, issueCount: 3).title, "Merge this pull request?"
        )
        XCTAssertEqual(
            Prompts.MergeIssuePr.copy(number: nil, issueCount: 3).body,
            "It is squash-merged. It covers 3 issues."
        )
        XCTAssertEqual(Prompts.MergeRunPr.copy(number: 7).title, "Merge PR #7?")
        XCTAssertEqual(Prompts.MergeRunPr.copy(number: nil).title, "Merge this pull request?")
    }

    func testDeviceAndServerVariants() {
        XCTAssertEqual(Prompts.ResumeRun.copy(device: "Studio").title, "Resume this run on Studio?")
        XCTAssertEqual(Prompts.ResumeRun.copy(device: nil).title, "Resume this run?")
        XCTAssertEqual(Prompts.ResumeRun.copy(device: "").title, "Resume this run?")
        XCTAssertEqual(
            Prompts.DeleteAccount.copy(server: "Cloud").title, "Delete your account on Cloud?"
        )
        XCTAssertEqual(Prompts.DeleteAccount.copy(server: nil).title, "Delete your account?")
    }
}
