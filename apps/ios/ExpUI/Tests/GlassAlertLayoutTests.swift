import XCTest
import ExpCore
import ExpUI

// EXP-1215: every confirm and choice prompt is ONE `GlassAlert`. Its row is
// pure ordering: a quiet destructive answer set apart on the LEADING edge,
// everything else trailing in reading order (the primary last), the stacked
// fallback one pill per line with the primary on top (no two-row hybrid),
// Return never on a destructive answer, and contract prompts mapped by id.
final class GlassAlertLayoutTests: XCTestCase {
    private func ids(_ actions: [GlassAlertAction]) -> [String] { actions.map(\.id) }

    private let leaveDialog = [
        GlassAlertAction("Discard", role: .quietDestructive) {},
        GlassAlertAction("Create issue", role: .outline) {},
        GlassAlertAction("Save draft", role: .primary) {},
    ]

    private let deleteConfirm = [
        GlassAlertAction("Cancel", role: .outline) {},
        GlassAlertAction("Delete", role: .destructive) {},
    ]

    func testQuietDestructiveLeadsTheRestTrail() {
        XCTAssertEqual(ids(GlassAlertLayout.leading(leaveDialog)), ["Discard"])
        XCTAssertEqual(ids(GlassAlertLayout.trailing(leaveDialog)), ["Create issue", "Save draft"])
    }

    func testQuietDestructiveLeadsWhereverItIsListed() {
        let listedLast = [leaveDialog[1], leaveDialog[2], leaveDialog[0]]
        XCTAssertEqual(ids(GlassAlertLayout.leading(listedLast)), ["Discard"])
        XCTAssertEqual(ids(GlassAlertLayout.trailing(listedLast)), ["Create issue", "Save draft"])
    }

    func testPlainDestructiveConfirmStaysTrailingBesideCancel() {
        XCTAssertEqual(ids(GlassAlertLayout.leading(deleteConfirm)), [])
        XCTAssertEqual(ids(GlassAlertLayout.trailing(deleteConfirm)), ["Cancel", "Delete"])
    }

    func testStackedPutsThePrimaryOnTopAndTheQuietOneLast() {
        XCTAssertEqual(
            ids(GlassAlertLayout.stacked(leaveDialog)),
            ["Save draft", "Create issue", "Discard"]
        )
    }

    func testThreeAnswersStackOnePerLineDefaultFirstCancelLast() {
        // No two-row hybrid: a row that does not fit stacks every answer, in
        // reverse display order.
        let stack = [
            GlassAlertAction("Cancel", role: .outline) {},
            GlassAlertAction("Merge this pull request", role: .outline) {},
            GlassAlertAction("Merge stack", role: .primary) {},
        ]
        XCTAssertEqual(
            ids(GlassAlertLayout.stacked(stack)),
            ["Merge stack", "Merge this pull request", "Cancel"]
        )
        XCTAssertEqual(ids(GlassAlertLayout.stacked(deleteConfirm)), ["Delete", "Cancel"])
    }

    func testReturnTakesThePrimaryOnly() {
        XCTAssertEqual(GlassAlertLayout.defaultActionId(leaveDialog), "Save draft")
        // A plain destructive confirm built by hand has no default.
        XCTAssertNil(GlassAlertLayout.defaultActionId(deleteConfirm))
    }

    func testReturnNeverTakesADestructiveAnswer() {
        let forced = GlassAlertAction("Delete", role: .destructive, isDefault: true) {}
        XCTAssertFalse(forced.isDefault)
        let quiet = GlassAlertAction("Discard", role: .quietDestructive, isDefault: true) {}
        XCTAssertFalse(quiet.isDefault)
    }

    // EXP-1215: a contract prompt maps onto the card by id: roles, display
    // order and focus straight from `prompts.json`.

    func testPromptDestructiveConfirmFocusesCancel() {
        let alert = GlassAlert(prompt: Prompts.DeleteIssue.copy(identifier: "EXP-12"), handlers: [:])
        let actions = Mirror(reflecting: alert).descendant("actions") as? [GlassAlertAction] ?? []
        XCTAssertEqual(ids(actions), ["cancel", "delete"])
        XCTAssertEqual(actions.map(\.label), ["Cancel", "Delete"])
        XCTAssertEqual(actions.map(\.role), [.outline, .destructive])
        XCTAssertEqual(GlassAlertLayout.defaultActionId(actions), "cancel")
    }

    func testPromptPrimaryTakesFocus() {
        let alert = GlassAlert(
            prompt: Prompts.MakeOwner.copy(name: "Ada"),
            enabled: ["make-owner": false],
            handlers: [:]
        )
        let actions = Mirror(reflecting: alert).descendant("actions") as? [GlassAlertAction] ?? []
        XCTAssertEqual(actions.map(\.role), [.outline, .primary])
        XCTAssertEqual(actions.map(\.label), ["Cancel", "Make owner"])
        XCTAssertEqual(actions.map(\.enabled), [true, false])
        XCTAssertEqual(GlassAlertLayout.defaultActionId(actions), "make-owner")
    }

    func testPromptRolesMapToPaints() {
        XCTAssertEqual(GlassAlertAction.Role(PromptRole.cancel), .outline)
        XCTAssertEqual(GlassAlertAction.Role(PromptRole.default), .outline)
        XCTAssertEqual(GlassAlertAction.Role(PromptRole.primary), .primary)
        XCTAssertEqual(GlassAlertAction.Role(PromptRole.destructive), .destructive)
        XCTAssertEqual(GlassAlertAction.Role(PromptRole.quietDestructive), .quietDestructive)
    }

    func testExplicitIdsOverrideTheLabel() {
        let action = GlassAlertAction("Merge stack", role: .primary, id: "merge-stack") {}
        XCTAssertEqual(action.id, "merge-stack")
        XCTAssertEqual(action.label, "Merge stack")
    }
}
