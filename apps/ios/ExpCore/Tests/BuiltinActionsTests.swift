import Foundation
import XCTest
@testable import ExpCore

// EXP-615: the three builtin action definitions must mirror
// apps/web/src/lib/builtin-actions.ts field-for-field — clients construct them
// locally, so a drifting name/input silently breaks the server's
// `resolveActionInputs`. "Chat" is additionally HIDDEN: it belongs to no list
// on any client, only to the Agent page composer with no subject picked.
//
// EXP-825: the request itself rides the start's `prompt` — Chat keeps ONLY its
// optional `repo` input, Create action only `repo` + `icon` (the `prompt`,
// `description` and `name` inputs are gone).
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
        XCTAssertEqual(chat.sortOrder, 1e9 + 2)

        let inputs = chat.inputs ?? []
        XCTAssertEqual(inputs.map(\.key), ["repo"])
        XCTAssertEqual(inputs[0].label, "Repository")
        XCTAssertEqual(inputs[0].type, "repo")
        // EXP-739: the repo is an OPTIONAL anchor — a repo-less chat runs
        // worktree-less in a scratch dir.
        XCTAssertFalse(inputs[0].isRequired)
        XCTAssertNil(inputs[0].placeholder)
    }

    // EXP-756: the composer wires a chat through `ActionInputValues.wireValues`
    // over the builtin's inputs, so a "No repository" pick ("") must reach the
    // wire as NO `repo` key at all — an empty string would read as a bogus
    // repository id — while a picked one rides through untouched.
    func testARepoLessChatWiresNoInputs() {
        let inputs = ActionDto.builtinChatAction(teamId: "t-1").inputs ?? []
        XCTAssertEqual(ActionInputValues.wireValues(inputs, values: ["repo": ""]), [:])
        XCTAssertEqual(
            ActionInputValues.wireValues(inputs, values: ["repo": "repo-1"]),
            ["repo": "repo-1"]
        )
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

    func testTheCreateBuiltinMatchesTheWebDefinition() {
        let create = ActionDto.builtinCreateAction(teamId: "t-1")
        XCTAssertEqual(create.id, "builtin:create-action")
        XCTAssertEqual(create.name, "Create action")
        XCTAssertEqual(
            create.description,
            "Describe a new action and let your agent author it for the team"
        )
        XCTAssertEqual(create.icon, "sparkles")
        XCTAssertEqual(create.sortOrder, 1e9)
        XCTAssertTrue(create.isBuiltin)

        // EXP-825: only the two PICKS remain, in the web's order.
        let inputs = create.inputs ?? []
        XCTAssertEqual(inputs.map(\.key), ["repo", "icon"])
        XCTAssertEqual(inputs.map(\.type), ["repo", "icon"])
        XCTAssertEqual(inputs.map(\.label), ["Repository", "Icon"])
        XCTAssertFalse(inputs.contains { $0.isRequired })
        XCTAssertFalse(inputs.contains { $0.type == "text" || $0.type == "textarea" })
    }

    func testTheFixConflictsBuiltinIsUnchanged() {
        let fix = ActionDto.builtinFixConflictsAction(teamId: "t-1")
        XCTAssertEqual(fix.id, "builtin:fix-conflicts")
        XCTAssertEqual(fix.name, "Fix merge conflicts")
        XCTAssertEqual(
            fix.description,
            "Pick a conflicted pull request and let your agent rebase, resolve, and merge it"
        )
        XCTAssertEqual(fix.icon, "git-branch")
        XCTAssertEqual(fix.sortOrder, 1e9 + 1)
        let inputs = fix.inputs ?? []
        XCTAssertEqual(inputs.map(\.key), ["pr"])
        XCTAssertEqual(inputs[0].type, "pr")
        XCTAssertTrue(inputs[0].isRequired)
    }

    /// EXP-672: a chat run needs an online machine with a runnable agent and
    /// nothing else — the `chat` cap mirror is gone, and so is the pool it
    /// used to shrink (the server stopped refusing on it in EXP-624).
    func testChatNeedsOnlyAStartableMachine() {
        let startable = SteerDevice(
            deviceId: "d-1",
            deviceLabel: "Mac",
            agents: ["claude"],
            caps: [],
            online: true
        )
        let signedOut = SteerDevice(
            deviceId: "d-2",
            deviceLabel: "Other Mac",
            agents: [],
            unauthedAgents: ["claude"],
            caps: ["actions", "action-inputs", "chat"],
            online: true
        )
        XCTAssertTrue(startable.isOnline && startable.hasRunnableAgent)
        XCTAssertFalse(signedOut.hasRunnableAgent)
    }
}
