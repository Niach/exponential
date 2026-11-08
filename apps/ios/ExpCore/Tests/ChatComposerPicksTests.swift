import Foundation
import XCTest
@testable import ExpCore

/// EXP-1334: the chat composer's mid-run picks (`sessions.configure`).
/// The Effort picker's "CLI default" row must reach the wire as a BLANK
/// (the server folds `""` to NULL; the `cli-default` sentinel is refused
/// with BAD_REQUEST), and a pick must go once the synced row reads it
/// (web `session-composer-options.tsx` `dropSettled`).
@MainActor
final class ChatComposerPicksTests: XCTestCase {
    // MARK: - The effort sentinel

    func testCliDefaultGoesOnTheWireBlankAndEveryOtherEffortVerbatim() {
        XCTAssertEqual(LaunchVocabulary.wireEffort(LaunchVocabulary.cliDefault), "")
        XCTAssertEqual(LaunchVocabulary.wireEffort("high"), "high")
        XCTAssertEqual(LaunchVocabulary.wireEffort("xhigh"), "xhigh")
        XCTAssertNotEqual(LaunchVocabulary.wireEffort(LaunchVocabulary.cliDefault), LaunchVocabulary.cliDefault)
    }

    func testABlankOrMissingEffortReadsAsTheCliDefaultRow() {
        XCTAssertEqual(LaunchVocabulary.effortChoice(nil), LaunchVocabulary.cliDefault)
        XCTAssertEqual(LaunchVocabulary.effortChoice(""), LaunchVocabulary.cliDefault)
        XCTAssertEqual(LaunchVocabulary.effortChoice("medium"), "medium")
        // The round trip: a pick of the CLI default row reads back as that row.
        XCTAssertEqual(
            LaunchVocabulary.effortChoice(LaunchVocabulary.wireEffort(LaunchVocabulary.cliDefault)),
            LaunchVocabulary.cliDefault
        )
    }

    func testTheLaunchWireCarriesTheSameBlankForTheCliDefault() {
        let state = LaunchOptionsState()
        state.effort = LaunchVocabulary.cliDefault
        XCTAssertEqual(state.buildOptions().effort, "")
        state.effort = "low"
        XCTAssertEqual(state.buildOptions().effort, "low")
    }

    // MARK: - apply

    func testApplySetsOnlyTheKeysTheInputCarries() {
        var picks = ChatComposerPicks()
        picks.apply(.init(sessionId: "s", effort: "high"))
        XCTAssertEqual(picks, ChatComposerPicks(effort: "high"))
        picks.apply(.init(sessionId: "s", ultracode: true, mcpServerIds: ["m1"]))
        XCTAssertEqual(picks, ChatComposerPicks(effort: "high", ultracode: true, mcpServerIds: ["m1"]))
    }

    // MARK: - settle

    func testAPickGoesOnceTheRowReadsIt() {
        var picks = ChatComposerPicks(effort: "high", ultracode: true, computerUse: false)
        picks.settle(.init(effort: "high", ultracode: true, computerUse: false))
        XCTAssertEqual(picks, ChatComposerPicks())
    }

    func testAPickTheRowStillDisagreesWithStays() {
        var picks = ChatComposerPicks(effort: "high", ultracode: true, computerUse: true)
        picks.settle(.init(effort: "low", ultracode: nil, computerUse: nil))
        XCTAssertEqual(picks, ChatComposerPicks(effort: "high", ultracode: true, computerUse: true))
    }

    func testOnlyTheSettledKeysGo() {
        var picks = ChatComposerPicks(effort: "high", ultracode: true, computerUse: true)
        picks.settle(.init(effort: "low", ultracode: true, computerUse: false))
        XCTAssertEqual(picks, ChatComposerPicks(effort: "high", computerUse: true))
    }

    func testASentCliDefaultSettlesAgainstANullOrBlankRowEffort() {
        // The pick went out as "" (wireEffort); the row stores NULL.
        var picks = ChatComposerPicks(effort: LaunchVocabulary.wireEffort(LaunchVocabulary.cliDefault))
        picks.settle(.init(effort: nil, ultracode: nil, computerUse: nil))
        XCTAssertNil(picks.effort)

        picks = ChatComposerPicks(effort: "")
        picks.settle(.init(effort: "", ultracode: nil, computerUse: nil))
        XCTAssertNil(picks.effort)

        // A row still on "high" keeps the CLI default pick up.
        picks = ChatComposerPicks(effort: "")
        picks.settle(.init(effort: "high", ultracode: nil, computerUse: nil))
        XCTAssertEqual(picks.effort, "")
    }

    func testTheMcpPickNeverSettlesAgainstTheRow() {
        var picks = ChatComposerPicks(mcpServerIds: ["m1"])
        picks.settle(.init(effort: nil, ultracode: nil, computerUse: nil))
        XCTAssertEqual(picks.mcpServerIds, ["m1"])
    }
}
