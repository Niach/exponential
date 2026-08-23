import Foundation
import XCTest
@testable import ExpCore

// EXP-615: the three builtin action definitions must mirror
// apps/web/src/lib/builtin-actions.ts field-for-field — clients construct them
// locally, so a drifting name/input silently breaks the server's
// `resolveActionInputs`. "Chat" is additionally HIDDEN: it belongs to no list
// on any client, only to the Start-coding sheet's Chat tab.
final class BuiltinActionsTests: XCTestCase {
    func testTheChatBuiltinMatchesTheWebDefinition() {
        let chat = ActionDto.builtinChatAction(teamId: "t-1")
        XCTAssertEqual(chat.id, DomainContract.builtinChatId)
        XCTAssertEqual(chat.id, "builtin:chat")
        XCTAssertEqual(chat.name, "Chat")
        XCTAssertEqual(chat.description, "Chat with your agent on a repository")
        XCTAssertEqual(chat.icon, "message-circle")
        XCTAssertTrue(chat.isBuiltin)
        XCTAssertNil(chat.repositoryId)
        XCTAssertEqual(chat.body, "")

        let inputs = chat.inputs ?? []
        XCTAssertEqual(inputs.map(\.key), ["prompt", "repo"])
        XCTAssertEqual(inputs[0].label, "Prompt")
        XCTAssertEqual(inputs[0].type, "textarea")
        XCTAssertTrue(inputs[0].isRequired)
        XCTAssertEqual(inputs[0].placeholder, "What should the agent do?")
        XCTAssertEqual(inputs[1].label, "Repository")
        XCTAssertEqual(inputs[1].type, "repo")
        XCTAssertTrue(inputs[1].isRequired)
    }

    // The leakage guard: chat is in NO list constructor.
    func testChatIsNeverListed() {
        let listed = ActionDto.builtinActions(teamId: "t-1")
        XCTAssertEqual(
            listed.map(\.id),
            [DomainContract.builtinCreateActionId, DomainContract.builtinFixConflictsId]
        )
        XCTAssertFalse(listed.contains { $0.id == DomainContract.builtinChatId })
    }

    // EXP-615 added the optional `name` input AFTER `description` — the order
    // is the web's, and the creator prompt reads it positionally on desktop.
    func testTheCreateBuiltinCarriesTheOptionalNameInput() {
        let inputs = ActionDto.builtinCreateAction(teamId: "t-1").inputs ?? []
        XCTAssertEqual(inputs.map(\.key), ["description", "name", "repo", "icon"])
        let name = inputs[1]
        XCTAssertEqual(name.label, "Name")
        XCTAssertEqual(name.type, "text")
        XCTAssertFalse(name.isRequired)
        XCTAssertEqual(name.placeholder, "Name (optional)")
    }

    func testChatCapabilityIsCapGated() {
        let capable = SteerDevice(
            deviceId: "d-1",
            deviceLabel: "Mac",
            agents: ["claude"],
            caps: ["actions", "action-inputs", "chat"],
            online: true
        )
        let old = SteerDevice(
            deviceId: "d-2",
            deviceLabel: "Old Mac",
            agents: ["claude"],
            caps: ["actions", "action-inputs"],
            online: true
        )
        XCTAssertTrue(capable.canChat)
        XCTAssertFalse(old.canChat)
    }
}
