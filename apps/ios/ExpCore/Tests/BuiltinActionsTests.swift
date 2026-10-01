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

    // EXP-981: the Plan-workflow builtin — the planner run of ONE draft
    // workflow, hidden like Chat. It takes NO inputs: the workflow it plans
    // rides the start as `workflowId`, and the free text is optional extra
    // instructions.
    func testThePlanWorkflowBuiltinMatchesTheWebDefinition() {
        let plan = ActionDto.builtinPlanWorkflowAction(teamId: "t-1")
        XCTAssertEqual(plan.id, DomainContract.builtinPlanWorkflowId)
        XCTAssertEqual(plan.id, "builtin:plan-workflow")
        XCTAssertEqual(plan.name, "Plan workflow")
        XCTAssertEqual(
            plan.description,
            "Let your agent turn a workflow's issues into a shallow, parallel plan"
        )
        XCTAssertEqual(plan.icon, "layers")
        XCTAssertTrue(plan.isBuiltin)
        XCTAssertNil(plan.repositoryId)
        XCTAssertEqual(plan.body, "")
        XCTAssertEqual(plan.sortOrder, 1e9 + 3)
        XCTAssertEqual(plan.inputs ?? [], [])
        XCTAssertEqual(plan.promptPlaceholder, "Anything the plan should respect (optional)…")
    }

    // The leakage guard: chat and the planner are in NO list constructor.
    func testChatAndPlanWorkflowAreNeverListed() {
        let listed = ActionDto.builtinActions(teamId: "t-1")
        XCTAssertEqual(
            listed.map(\.id),
            [
                DomainContract.builtinCreateActionId,
                DomainContract.builtinFixConflictsId,
                DomainContract.builtinTidyUpId,
            ]
        )
        XCTAssertFalse(listed.contains { $0.id == DomainContract.builtinChatId })
        XCTAssertFalse(listed.contains { $0.id == DomainContract.builtinPlanWorkflowId })
    }

    // FEED-50: "Tidy up" — the third LISTED builtin. Pinned literals,
    // byte-identical to the web.
    func testTheTidyUpBuiltinMatchesTheWebDefinition() {
        let tidy = ActionDto.builtinTidyUpAction(teamId: "t-1")
        XCTAssertEqual(tidy.id, DomainContract.builtinTidyUpId)
        XCTAssertEqual(tidy.id, "builtin:tidy-up")
        XCTAssertEqual(tidy.teamId, "t-1")
        XCTAssertEqual(tidy.name, "Tidy up")
        XCTAssertEqual(
            tidy.description,
            "Let your agent dedupe, label and link a board's issues. Nothing is deleted"
        )
        XCTAssertEqual(tidy.icon, "brush-cleaning")
        XCTAssertTrue(tidy.isBuiltin)
        XCTAssertNil(tidy.repositoryId)
        XCTAssertEqual(tidy.body, "")
        XCTAssertEqual(tidy.sortOrder, 1e9 + 4)
        XCTAssertEqual(
            tidy.promptPlaceholder,
            "Anything the tidy-up should focus on or leave alone (optional)…"
        )

        let inputs = tidy.inputs ?? []
        XCTAssertEqual(inputs.map(\.key), ["board", "repo"])
        XCTAssertEqual(inputs.map(\.label), ["Board", "Repository"])
        XCTAssertEqual(inputs.map(\.type), ["board", "repo"])
        XCTAssertFalse(inputs.contains { $0.isRequired })
    }

    // SLOP-2: builtins never carry triggers.
    func testNoBuiltinCarriesTriggers() {
        let builtins = ActionDto.builtinActions(teamId: "t-1") + [
            ActionDto.builtinChatAction(teamId: "t-1"),
            ActionDto.builtinPlanWorkflowAction(teamId: "t-1"),
        ]
        XCTAssertTrue(builtins.allSatisfy(\.triggers.isEmpty))
    }

    // SLOP-2: a team's REAL row named exactly "Tidy up" hides the virtual
    // builtin from every list (web `hasOwnTidyUpAction`).
    func testARealTidyUpRowHidesTheBuiltin() {
        func row(_ name: String) -> ActionDto {
            ActionDto(
                id: "a-\(name)",
                teamId: "t-1",
                repositoryId: nil,
                name: name,
                description: nil,
                body: "",
                sortOrder: 0,
                createdAt: "2026-01-01T00:00:00Z",
                updatedAt: "2026-01-01T00:00:00Z"
            )
        }
        let allThree = [
            DomainContract.builtinCreateActionId,
            DomainContract.builtinFixConflictsId,
            DomainContract.builtinTidyUpId,
        ]
        XCTAssertFalse(ActionDto.hasOwnTidyUpAction([]))
        XCTAssertFalse(ActionDto.hasOwnTidyUpAction([row("Tidy up boards"), row("tidy up")]))
        // The builtin itself is not the team's own row.
        XCTAssertFalse(ActionDto.hasOwnTidyUpAction([ActionDto.builtinTidyUpAction(teamId: "t-1")]))
        XCTAssertTrue(ActionDto.hasOwnTidyUpAction([row("Digest"), row("Tidy up")]))
        XCTAssertEqual(
            ActionDto.listedBuiltinActions(teamId: "t-1", teamActions: [row("Digest")]).map(\.id),
            allThree
        )
        XCTAssertEqual(
            ActionDto.listedBuiltinActions(teamId: "t-1", teamActions: [row("Tidy up")]).map(\.id),
            [DomainContract.builtinCreateActionId, DomainContract.builtinFixConflictsId]
        )
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

        // EXP-825: the retired free-text input's placeholder became the
        // builtin's composer hint — byte-identical to the web.
        XCTAssertEqual(
            create.promptPlaceholder,
            "Describe the action — what it should do, and its name if you have one…"
        )
    }

    // EXP-825: Create action carries a composer hint (EXP-981: so does Plan
    // workflow); the other two show the generic
    // "Additional instructions (optional)…".
    func testOnlyTheTwoAuthoringBuiltinsCarryAComposerHint() {
        XCTAssertNil(ActionDto.builtinFixConflictsAction(teamId: "t-1").promptPlaceholder)
        XCTAssertNil(ActionDto.builtinChatAction(teamId: "t-1").promptPlaceholder)
        XCTAssertNotNil(ActionDto.builtinPlanWorkflowAction(teamId: "t-1").promptPlaceholder)
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
