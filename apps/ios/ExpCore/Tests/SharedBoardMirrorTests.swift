import Foundation
import XCTest
@testable import ExpCore

// The share-extension board mirror lives in app-group UserDefaults, outside the
// per-account GRDB files that sign-out / remove-server / delete-account wipe.
// Nothing but a *remaining* account's board observation ever rewrites it, so the
// teardown paths must evict a departing account themselves.
final class SharedBoardMirrorTests: XCTestCase {
    // Mirrors SharedBoardMirror's private keys (stable persisted names).
    private let boardsKey = "picker_boards_v1"
    private let lastUsedKey = "picker_last_used_board_v1"

    private var defaults: UserDefaults!

    override func setUpWithError() throws {
        defaults = try XCTUnwrap(SharedAppGroup.defaults)
        wipe()
    }

    override func tearDown() {
        wipe()
        defaults = nil
    }

    private func wipe() {
        defaults.removeObject(forKey: boardsKey)
        defaults.removeObject(forKey: lastUsedKey)
    }

    private func board(account: String, board: String) -> MirroredBoard {
        MirroredBoard(
            accountId: account,
            accountName: "server-\(account)",
            teamId: "team-\(account)",
            teamName: "Team \(account)",
            boardId: board,
            boardName: "Board \(board)",
            prefix: "B"
        )
    }

    // Removing one account leaves every other account's boards intact.
    func testRemoveDropsOnlyThatAccountsBoards() {
        SharedBoardMirror.write(boards: [
            board(account: "A", board: "a1"),
            board(account: "A", board: "a2"),
            board(account: "B", board: "b1"),
        ])

        SharedBoardMirror.remove(accountId: "A")

        let kept = SharedBoardMirror.readBoards()
        XCTAssertEqual(kept.map(\.boardId), ["b1"])
    }

    // Removing the last account must leave nothing at rest — not an encoded
    // empty array, which would still be a residue in device backups.
    func testRemoveLastAccountClearsTheStoredValue() {
        SharedBoardMirror.write(boards: [board(account: "A", board: "a1")])

        SharedBoardMirror.remove(accountId: "A")

        XCTAssertTrue(SharedBoardMirror.readBoards().isEmpty)
        XCTAssertNil(defaults.data(forKey: boardsKey))
    }

    // The picker default must not survive the account it points at.
    func testRemoveClearsLastUsedOfThatAccount() {
        SharedBoardMirror.write(boards: [board(account: "A", board: "a1")])
        SharedBoardMirror.writeLastUsed(accountId: "A", boardId: "a1")

        SharedBoardMirror.remove(accountId: "A")

        XCTAssertNil(SharedBoardMirror.readLastUsed())
        XCTAssertNil(defaults.data(forKey: lastUsedKey))
    }

    // …but another account's picker default is untouched.
    func testRemoveKeepsLastUsedOfOtherAccount() {
        SharedBoardMirror.write(boards: [
            board(account: "A", board: "a1"),
            board(account: "B", board: "b1"),
        ])
        SharedBoardMirror.writeLastUsed(accountId: "B", boardId: "b1")

        SharedBoardMirror.remove(accountId: "A")

        XCTAssertEqual(SharedBoardMirror.readLastUsed()?.accountId, "B")
        XCTAssertEqual(SharedBoardMirror.readLastUsed()?.boardId, "b1")
    }

    // Removing an account that was never mirrored is a no-op, not a wipe.
    func testRemoveUnknownAccountIsHarmless() {
        SharedBoardMirror.write(boards: [board(account: "A", board: "a1")])
        SharedBoardMirror.writeLastUsed(accountId: "A", boardId: "a1")

        SharedBoardMirror.remove(accountId: "Z")
        SharedBoardMirror.remove(accountId: "")

        XCTAssertEqual(SharedBoardMirror.readBoards().map(\.boardId), ["a1"])
        XCTAssertEqual(SharedBoardMirror.readLastUsed()?.accountId, "A")
    }

    // The loader prunes the picker default whenever its account drops out of the
    // signed-in set (sign-out, not just removal).
    func testPruneLastUsedDropsSignedOutAccount() {
        SharedBoardMirror.writeLastUsed(accountId: "A", boardId: "a1")

        SharedBoardMirror.pruneLastUsed(signedInAccountIds: ["B"])

        XCTAssertNil(SharedBoardMirror.readLastUsed())
    }

    func testPruneLastUsedKeepsSignedInAccount() {
        SharedBoardMirror.writeLastUsed(accountId: "A", boardId: "a1")

        SharedBoardMirror.pruneLastUsed(signedInAccountIds: ["A", "B"])

        XCTAssertEqual(SharedBoardMirror.readLastUsed()?.boardId, "a1")
    }
}
