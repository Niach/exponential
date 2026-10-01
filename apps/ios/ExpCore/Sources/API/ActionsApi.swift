import Foundation

// Mirrors apps/web/src/lib/trpc/actions.ts (EXP-253). Since EXP-268 the team
// action prompts SYNC as the 15th Electric shape (minus `body` — the ≤64KB
// markdown prompt never rides sync; tRPC `actions.get` stays the only body
// path, and only the desktop ever executes a body behind its per-device trust
// prompt). Mobile lists actions from the local synced store and remote-starts
// them on a desktop via `steer.startSession({actionId})`; since EXP-694 it also
// EDITS them (the action page's Prompt tab), which is what `get` (body) and
// `update` are for. SLOP-2: an action carries its TRIGGERS (`triggers`, synced
// with the row); `update({ triggers })` — a whole-array replace — is their
// only write path.

/// One typed action input (EXP-257): filled in the Agent page composer and
/// injected into the prompt by the desktop. `type` is a contract value
/// (`DomainContract.actionInputTypeValues` — repo / board / pr / icon; EXP-825
/// retired the free-text `text` / `textarea` types, whatever the requester
/// types is the start's `prompt` now); tolerate unknown future types by gating
/// the run, never by silently degrading. `required` absent = optional (the
/// wire omits the default-false flag).
public struct ActionInputDto: Decodable, Sendable, Equatable {
    public let key: String
    public let label: String
    public let type: String
    public let required: Bool?
    public let placeholder: String?

    public init(
        key: String,
        label: String,
        type: String,
        required: Bool? = nil,
        placeholder: String? = nil
    ) {
        self.key = key
        self.label = label
        self.type = type
        self.required = required
        self.placeholder = placeholder
    }

    public var isRequired: Bool { required == true }
}

/// One team action (`actions.list` row). `repositoryId` is nil for repo-less
/// actions (the desktop runs those in a scratch dir); `description` is the
/// optional one-liner under the name. `builtin == true` marks a virtual
/// builtin row (EXP-257/EXP-539 — constructed locally by every client, pinned
/// FIRST by this flag and never by sort order; non-editable, `body` empty).
/// `inputs` is the typed inputs schema — nil when a synced row carries no
/// parseable inputs JSON. `triggers` holds the READABLE triggers in stored
/// order (SLOP-2; tolerant — see `ActionTrigger`), always empty on a builtin.
public struct ActionDto: Identifiable, Sendable {
    public let id: String
    public let teamId: String
    public let repositoryId: String?
    public let name: String
    public let description: String?
    /// EXP-273: curated registry icon name; nil = the generic action glyph.
    public let icon: String?
    public let body: String
    public let sortOrder: Double
    public let createdAt: String
    public let updatedAt: String
    public let inputs: [ActionInputDto]?
    public let builtin: Bool?
    /// EXP-825: the composer's field hint while this action is picked (≤200
    /// chars); nil = the generic "Additional instructions (optional)…".
    public let promptPlaceholder: String?
    public let triggers: [ActionTrigger]

    enum CodingKeys: String, CodingKey {
        case id, teamId, repositoryId, name, description, icon, body
        case sortOrder, createdAt, updatedAt, inputs, builtin, promptPlaceholder
        case triggers
    }

    public init(
        id: String,
        teamId: String,
        repositoryId: String?,
        name: String,
        description: String?,
        icon: String? = nil,
        body: String,
        sortOrder: Double,
        createdAt: String,
        updatedAt: String,
        inputs: [ActionInputDto]? = nil,
        builtin: Bool? = nil,
        promptPlaceholder: String? = nil,
        triggers: [ActionTrigger] = []
    ) {
        self.id = id
        self.teamId = teamId
        self.repositoryId = repositoryId
        self.name = name
        self.description = description
        self.icon = icon
        self.body = body
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.inputs = inputs
        self.builtin = builtin
        self.promptPlaceholder = promptPlaceholder
        self.triggers = triggers
    }

    /// The server's cap on `promptPlaceholder` (`MAX_ACTION_PROMPT_PLACEHOLDER`
    /// in db-schema/domain.ts) — the editor refuses at the field, not at
    /// submit.
    public static let promptPlaceholderMaxLength = 200

    /// `promptPlaceholder` cut to the server's cap. The cap is a JS string
    /// length, i.e. UTF-16 code units — never `String.count` (a grapheme can
    /// be many units, so a flag-heavy hint would pass here and be refused
    /// on the wire). A split surrogate pair is dropped rather than shipped.
    public static func clampPromptPlaceholder(_ value: String) -> String {
        let cap = promptPlaceholderMaxLength
        guard value.utf16.count > cap else { return value }
        var units = Array(value.utf16.prefix(cap))
        if let last = units.last, UTF16.isLeadSurrogate(last) {
            units.removeLast()
        }
        return String(decoding: units, as: UTF16.self)
    }

    /// The virtual builtin row (EXP-257).
    public var isBuiltin: Bool { builtin == true }

    /// A triggered run has nobody to fill an input in, so the server refuses
    /// to ENABLE a trigger on an action that declares a required one.
    public var hasRequiredInput: Bool {
        (inputs ?? []).contains(where: \.isRequired)
    }
}

// Hand-written (not synthesized) for `triggers` alone: a jsonb array over
// tRPC, read TOLERANTLY — an unreadable element is skipped, an absent key (an
// older server) is no triggers, and neither ever fails the row.
extension ActionDto: Decodable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        repositoryId = try c.decodeIfPresent(String.self, forKey: .repositoryId)
        name = try c.decode(String.self, forKey: .name)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        icon = try c.decodeIfPresent(String.self, forKey: .icon)
        body = try c.decode(String.self, forKey: .body)
        sortOrder = try c.decode(Double.self, forKey: .sortOrder)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
        inputs = try c.decodeIfPresent([ActionInputDto].self, forKey: .inputs)
        builtin = try c.decodeIfPresent(Bool.self, forKey: .builtin)
        promptPlaceholder = try c.decodeIfPresent(String.self, forKey: .promptPlaceholder)
        triggers = ActionTrigger.parseList(c.decodeWireJsonString(forKey: .triggers))
    }
}

public extension ActionDto {
    /// The virtual "Create action" builtin (EXP-257). The server used to
    /// append it in `actions.list`; with actions synced via Electric
    /// (EXP-268, the 15th shape) clients PREPEND it locally instead — synced
    /// rows can't carry a virtual entry. Mirrors
    /// apps/web/src/lib/builtin-actions.ts field-for-field (the huge
    /// sortOrder only keeps naive sortOrder-asc renderers from interleaving
    /// it; pinning goes by the `builtin` flag).
    ///
    /// EXP-825: the request itself (what the action should do, and its name
    /// if the user states one) is the start's `prompt`, never an input — the
    /// two remaining inputs are PICKS the creator run can't derive from prose.
    static func builtinCreateAction(teamId: String) -> ActionDto {
        ActionDto(
            id: DomainContract.builtinCreateActionId,
            teamId: teamId,
            repositoryId: nil,
            name: "Create action",
            description: "Describe a new action and let your agent author it for the team",
            icon: "sparkles",
            body: "",
            sortOrder: 1e9,
            createdAt: "1970-01-01T00:00:00.000Z",
            updatedAt: "1970-01-01T00:00:00.000Z",
            inputs: [
                ActionInputDto(key: "repo", label: "Repository", type: "repo", required: false),
                // EXP-273: the author picks the new action's glyph up front.
                ActionInputDto(key: "icon", label: "Icon", type: "icon", required: false),
            ],
            builtin: true,
            // EXP-825: the composer's hint for the request text (the retired
            // free-text input's placeholder, byte-identical to the web).
            promptPlaceholder: "Describe the action — what it should do, and its name if you have one…"
        )
    }

    /// The virtual "Fix merge conflicts" builtin (EXP-259/EXP-270): pick a
    /// conflicted open PR and let the desktop rebase, resolve, push and merge
    /// it. Its `pr` input value is the REPRESENTATIVE issue id of an
    /// issue-linked open pull request (a batch PR links several issues to one
    /// PR — the picker dedupes by prUrl). Mirrors
    /// apps/web/src/lib/builtin-actions.ts field-for-field.
    static func builtinFixConflictsAction(teamId: String) -> ActionDto {
        ActionDto(
            id: DomainContract.builtinFixConflictsId,
            teamId: teamId,
            repositoryId: nil,
            name: "Fix merge conflicts",
            description: "Pick a conflicted pull request and let your agent rebase, resolve, and merge it",
            icon: "git-branch",
            body: "",
            sortOrder: 1e9 + 1,
            createdAt: "1970-01-01T00:00:00.000Z",
            updatedAt: "1970-01-01T00:00:00.000Z",
            inputs: [
                ActionInputDto(key: "pr", label: "Pull request", type: "pr", required: true),
            ],
            builtin: true,
            promptPlaceholder: nil
        )
    }

    /// The virtual "Tidy up" builtin (FEED-50): the agent dedupes, labels
    /// and links a board's issues and deletes nothing. Both inputs are
    /// OPTIONAL picks. Like every builtin it carries no triggers. Mirrors
    /// apps/web/src/lib/builtin-actions.ts field-for-field.
    static func builtinTidyUpAction(teamId: String) -> ActionDto {
        ActionDto(
            id: DomainContract.builtinTidyUpId,
            teamId: teamId,
            repositoryId: nil,
            name: builtinTidyUpName,
            description: "Let your agent dedupe, label and link a board's issues. Nothing is deleted",
            icon: "brush-cleaning",
            body: "",
            sortOrder: 1e9 + 4,
            createdAt: "1970-01-01T00:00:00.000Z",
            updatedAt: "1970-01-01T00:00:00.000Z",
            inputs: [
                ActionInputDto(key: "board", label: "Board", type: "board", required: false),
                ActionInputDto(key: "repo", label: "Repository", type: "repo", required: false),
            ],
            builtin: true,
            promptPlaceholder: "Anything the tidy-up should focus on or leave alone (optional)…"
        )
    }

    /// The HIDDEN "Chat" builtin (EXP-615): a conversation with your agent over
    /// the tracker's MCP tools, OPTIONALLY anchored to a repository (EXP-739) —
    /// the iOS twin of the desktop's chat tab. Unlike the other two it is
    /// appended to NO list and belongs in NO picker: the Agent page composer
    /// constructs it directly when no subject is picked (EXP-825). The chat
    /// text is the start's `prompt` (required for this builtin), never an
    /// input. Mirrors apps/web/src/lib/builtin-actions.ts field-for-field.
    static func builtinChatAction(teamId: String) -> ActionDto {
        ActionDto(
            id: DomainContract.builtinChatId,
            teamId: teamId,
            repositoryId: nil,
            name: "Chat",
            description: "Chat with your agent on a repository",
            icon: "message-circle",
            body: "",
            sortOrder: 1e9 + 2,
            createdAt: "1970-01-01T00:00:00.000Z",
            updatedAt: "1970-01-01T00:00:00.000Z",
            inputs: [
                // EXP-739: OPTIONAL. A repo-less chat runs in the agent's
                // scratch dir with only the Exponential MCP server wired up;
                // with a repo it keeps its own `exp/chat-<id8>` worktree.
                ActionInputDto(key: "repo", label: "Repository", type: "repo", required: false),
            ],
            builtin: true,
            promptPlaceholder: nil
        )
    }

    /// The HIDDEN "Plan workflow" builtin (EXP-981): the planner run of ONE
    /// draft workflow — it reads that workflow's issues, sets the relations,
    /// splits what is too big, declares `touches` and `risk`, and keeps the
    /// graph shallow. Like Chat it is appended to NO list and belongs in NO
    /// picker: a workflow's own Plan button constructs it, because it is
    /// meaningless without that workflow's id (which rides the start as
    /// `workflowId`). It takes NO inputs — it plans over MCP and writes no
    /// code, so it runs in the agent's scratch dir. Mirrors
    /// apps/web/src/lib/builtin-actions.ts field-for-field.
    static func builtinPlanWorkflowAction(teamId: String) -> ActionDto {
        ActionDto(
            id: DomainContract.builtinPlanWorkflowId,
            teamId: teamId,
            repositoryId: nil,
            name: "Plan workflow",
            description: "Let your agent turn a workflow's issues into a shallow, parallel plan",
            icon: "layers",
            body: "",
            sortOrder: 1e9 + 3,
            createdAt: "1970-01-01T00:00:00.000Z",
            updatedAt: "1970-01-01T00:00:00.000Z",
            inputs: [],
            builtin: true,
            // The composer's hint: extra instructions the plan should respect.
            promptPlaceholder: "Anything the plan should respect (optional)…"
        )
    }

    /// The three LISTED builtins, in the order every client pins them (EXP-270 —
    /// mobile used to construct only "Create action", so "Fix merge
    /// conflicts" silently disappeared from iOS when EXP-268 moved the list
    /// onto the synced shape; FEED-50 appends "Tidy up").
    static func builtinActions(teamId: String) -> [ActionDto] {
        [
            builtinCreateAction(teamId: teamId),
            builtinFixConflictsAction(teamId: teamId),
            builtinTidyUpAction(teamId: teamId),
        ]
    }

    /// The Tidy up builtin's name — reserved: a real row may wear it only as
    /// the one `hasOwnTidyUpAction` looks for.
    static let builtinTidyUpName = "Tidy up"

    /// SLOP-2: a team that triggered Tidy up before triggers moved onto
    /// actions owns a REAL row of that name (a server migration made it, so
    /// it could carry them). Where it exists the virtual builtin steps aside,
    /// so no list shows Tidy up twice (web `hasOwnTidyUpAction`, ×4).
    static func hasOwnTidyUpAction(_ teamActions: [ActionDto]) -> Bool {
        teamActions.contains { !$0.isBuiltin && $0.name == builtinTidyUpName }
    }

    /// `builtinActions` as a list beside `teamActions` shows them: Tidy up
    /// hidden while the team owns its real row.
    static func listedBuiltinActions(teamId: String, teamActions: [ActionDto]) -> [ActionDto] {
        let builtins = builtinActions(teamId: teamId)
        guard hasOwnTidyUpAction(teamActions) else { return builtins }
        return builtins.filter { $0.id != DomainContract.builtinTidyUpId }
    }

    /// Build a list-surface DTO from the synced local row (EXP-268). `body`
    /// is deliberately empty — the actions shape excludes it (tRPC
    /// `actions.get` stays the only body path) and nothing on the mobile
    /// view+run surfaces needs it. `inputs` parses the stored JSON string
    /// (absent/unparseable → nil).
    init(entity: ActionEntity) {
        let parsedInputs = entity.inputs
            .flatMap { $0.data(using: .utf8) }
            .flatMap { try? JSONDecoder().decode([ActionInputDto].self, from: $0) }
        self.init(
            id: entity.id,
            teamId: entity.teamId,
            repositoryId: entity.repositoryId,
            name: entity.name,
            description: entity.description,
            icon: entity.icon,
            body: "",
            sortOrder: entity.sortOrder ?? 0,
            createdAt: entity.createdAt,
            updatedAt: entity.updatedAt,
            inputs: parsedInputs,
            builtin: false,
            promptPlaceholder: entity.promptPlaceholder,
            triggers: ActionTrigger.parseList(entity.triggers)
        )
    }
}

/// Server envelope: `actions.list` returns `{ actions: [<row>] }`.
public struct ActionsListResult: Decodable, Sendable {
    public let actions: [ActionDto]

    public init(actions: [ActionDto]) {
        self.actions = actions
    }
}

/// Server envelope: `actions.get` / `actions.update` return `{ action }`.
public struct ActionResult: Decodable, Sendable {
    public let action: ActionDto

    public init(action: ActionDto) {
        self.action = action
    }
}

/// A partial `actions.update` payload (EXP-694 — mobile edits actions now).
/// Mirrors the router's optional inputs, omit-vs-null per field: an OMITTED
/// field (nil) keeps what the row has, while the CLEARABLE ones are nested
/// optionals, so `.some(nil)` sends an explicit null — "no icon" / "no
/// repository" / "no composer hint" (EXP-825). `triggers` (SLOP-2) is the
/// WHOLE array or nothing: the server replaces it, never merges.
public struct ActionPatch: Sendable, Equatable {
    public var name: String?
    public var description: String??
    public var icon: String??
    public var repositoryId: String??
    public var body: String?
    public var promptPlaceholder: String??
    public var triggers: [ActionTriggerInput]?

    public init(
        name: String? = nil,
        description: String?? = nil,
        icon: String?? = nil,
        repositoryId: String?? = nil,
        body: String? = nil,
        promptPlaceholder: String?? = nil,
        triggers: [ActionTriggerInput]? = nil
    ) {
        self.name = name
        self.description = description
        self.icon = icon
        self.repositoryId = repositoryId
        self.body = body
        self.promptPlaceholder = promptPlaceholder
        self.triggers = triggers
    }

    /// Nothing changed — the editor keeps its Save disabled rather than
    /// sending an id-only mutation.
    public var isEmpty: Bool {
        name == nil && description == nil && icon == nil
            && repositoryId == nil && body == nil && promptPlaceholder == nil
            && triggers == nil
    }
}

private struct ListInput: Encodable {
    let teamId: String
}

private struct IdInput: Encodable {
    let id: String
}

/// Internal (not private) so the wire-format test can pin the omit-vs-null
/// encoding per field.
struct ActionUpdateInput: Encodable {
    let id: String
    let patch: ActionPatch

    enum CodingKeys: String, CodingKey {
        case id, name, description, icon, repositoryId, body, promptPlaceholder
        case triggers
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(patch.name, forKey: .name)
        try c.encodeIfPresent(patch.body, forKey: .body)
        // The whole array, an EMPTY one included ("no triggers left").
        try c.encodeIfPresent(patch.triggers, forKey: .triggers)
        // The unwrapped inner optional is encoded even when nil — an explicit
        // null is what clears the field server-side.
        if let description = patch.description {
            try c.encode(description, forKey: .description)
        }
        if let icon = patch.icon {
            try c.encode(icon, forKey: .icon)
        }
        if let repositoryId = patch.repositoryId {
            try c.encode(repositoryId, forKey: .repositoryId)
        }
        if let promptPlaceholder = patch.promptPlaceholder {
            try c.encode(promptPlaceholder, forKey: .promptPlaceholder)
        }
    }
}

public final class ActionsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Member-gated `actions.list` — the team's actions, sortOrder-then-name
    /// ordered server-side.
    public func list(accountId: String, teamId: String) async throws -> [ActionDto] {
        let result: ActionsListResult = try await trpc.query(
            accountId: accountId,
            path: "actions.list",
            input: ListInput(teamId: teamId)
        )
        return result.actions
    }

    /// Member-gated `actions.get` — the ONLY body path (the synced shape
    /// excludes the ≤64KB prompt), so the edit sheet fetches it on open.
    /// Refuses the virtual builtins server-side: they have no row.
    public func get(accountId: String, id: String) async throws -> ActionDto {
        let result: ActionResult = try await trpc.query(
            accountId: accountId,
            path: "actions.get",
            input: IdInput(id: id)
        )
        return result.action
    }

    /// Owner-gated `actions.update` — a partial patch (EXP-694). The synced
    /// row echoes the metadata back, so success needs no local write. With
    /// `triggers` the server refuses, by message, an enabled trigger on an
    /// action with required inputs, a device that is not yours/team-shared
    /// or lacks the `automations` cap, an agent the device doesn't advertise,
    /// and an account pinned without one.
    @discardableResult
    public func update(
        accountId: String,
        id: String,
        patch: ActionPatch
    ) async throws -> ActionDto {
        let result: ActionResult = try await trpc.mutation(
            accountId: accountId,
            path: "actions.update",
            input: ActionUpdateInput(id: id, patch: patch)
        )
        return result.action
    }
}
