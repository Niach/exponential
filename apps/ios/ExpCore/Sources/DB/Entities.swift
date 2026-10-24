import Foundation
import GRDB

// MARK: - Electric Offset

public struct ElectricOffset: Codable, FetchableRecord, PersistableRecord, Equatable, Sendable {
    public static let databaseTableName = "electric_offsets"

    public let shape: String
    public let handle: String
    public let offset: String
    /// A 409 / must-refetch happened and the next poll must refetch from
    /// scratch (offset -1, atomic DELETE+reinsert). Persisted so a quit
    /// between the 409 and the refetch can't strand stale rows. `handle` then
    /// holds the replacement handle from the 409 response ("" when unknown).
    public let needsRefetch: Bool
    /// True once up-to-date was seen for the current handle — only then do
    /// polls switch to live long-polling.
    public let isLive: Bool

    public init(shape: String, handle: String, offset: String, needsRefetch: Bool = false, isLive: Bool = false) {
        self.shape = shape
        self.handle = handle
        self.offset = offset
        self.needsRefetch = needsRefetch
        self.isLive = isLive
    }

    enum CodingKeys: String, CodingKey {
        case shape, handle, offset
        case needsRefetch = "needs_refetch"
        case isLive = "is_live"
    }
}

// MARK: - Team

public struct TeamEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "teams"

    public let id: String
    public let name: String
    public let slug: String
    public let iconUrl: String?
    /// EXP-1105 yolo mode: PRs auto-merge, so the Reviews tab hides unless
    /// an open PR (= a failed auto-merge) is waiting.
    public let yoloMode: Bool
    /// EXP-630: the team's estimate scale (contract `issueEstimation`). NULL
    /// (a pre-rotation snapshot) reads as `none` = estimates off.
    public let estimationType: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        name: String,
        slug: String,
        iconUrl: String?,
        yoloMode: Bool = false,
        estimationType: String? = nil,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.name = name
        self.slug = slug
        self.iconUrl = iconUrl
        self.yoloMode = yoloMode
        self.estimationType = estimationType
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    // The team shape no longer carries the long-dropped legacy
    // `is_public` / `public_write_policy` columns. This decoder simply ignores
    // any such legacy keys Electric might still deliver during a shape
    // rotation (unknown keys are dropped by Codable).
    enum CodingKeys: String, CodingKey {
        case id, name, slug
        case iconUrl = "icon_url"
        case yoloMode = "yolo_mode"
        case estimationType = "estimation_type"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom decode: `yolo_mode` arrives as Postgres text off the Electric wire
// ("t"/"true"/…) but as a native scalar from tRPC/fixtures, and a
// pre-rotation snapshot may omit it — decode permissively with the schema
// default (the BoardEntity wire-bool precedent).
extension TeamEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        slug = try c.decode(String.self, forKey: .slug)
        iconUrl = try c.decodeIfPresent(String.self, forKey: .iconUrl)
        yoloMode = c.decodeWireBool(forKey: .yoloMode, default: false)
        estimationType = try c.decodeIfPresent(String.self, forKey: .estimationType)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Board

public struct BoardEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "boards"

    public let id: String
    public let teamId: String
    public let name: String
    public let slug: String
    public let prefix: String
    public let color: String?
    public let sortOrder: Double?
    // v4: the repo backing this board (server-only `repositories` registry
    // row). Synced ride-along on the boards shape — the uuid resolves to a
    // fullName/defaultBranch via the repositories tRPC API (cached per
    // team). Nullable — repos are optional on every board; coding
    // affordances gate on presence.
    public let repositoryId: String?
    // Curated glyph name (DomainContract.boardIconValues) — nil means fall
    // back to a derived icon. Rendered to an SF Symbol client-side.
    public let icon: String?
    // EXP-712: the branch THIS board's coding sessions branch from and its PRs
    // target. Nil = follow the repo (its team-pinned override, else GitHub's
    // default), so two boards on one repo can develop on different branches.
    // Reset to nil server-side whenever the board is retargeted.
    public let defaultBranch: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        name: String,
        slug: String,
        prefix: String,
        color: String?,
        sortOrder: Double?,
        repositoryId: String?,
        icon: String? = nil,
        defaultBranch: String? = nil,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.name = name
        self.slug = slug
        self.prefix = prefix
        self.color = color
        self.sortOrder = sortOrder
        self.repositoryId = repositoryId
        self.icon = icon
        self.defaultBranch = defaultBranch
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, slug, prefix, color, icon
        case teamId = "team_id"
        case sortOrder = "sort_order"
        case repositoryId = "repository_id"
        case defaultBranch = "default_branch"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: the icon column landed in a shape rotation; a pre-rotation
// snapshot (or a partial update touching other columns) may omit it, so decode
// permissively with the schema default instead of throwing. `is_protected` was
// dropped server-side (EXP-364 — protected boards are gone); an unknown key on
// the wire is simply ignored by Codable. sort_order comes off the Electric wire
// as a JSON string (Postgres text — "2"/"3.5") but as a native scalar from
// tRPC/fixtures, so it goes through the type-aware wire decoder.
extension BoardEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        name = try c.decode(String.self, forKey: .name)
        slug = try c.decode(String.self, forKey: .slug)
        prefix = try c.decode(String.self, forKey: .prefix)
        color = try c.decodeIfPresent(String.self, forKey: .color)
        sortOrder = try c.decodeWireDouble(forKey: .sortOrder)
        repositoryId = try c.decodeIfPresent(String.self, forKey: .repositoryId)
        icon = try c.decodeIfPresent(String.self, forKey: .icon)
        // Same permissive read as `icon`: the column landed in a shape
        // rotation (EXP-712), so a pre-rotation snapshot simply omits it.
        defaultBranch = try c.decodeIfPresent(String.self, forKey: .defaultBranch)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Issue

public struct IssueEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "issues"

    public let id: String
    public let boardId: String
    public let number: Int?
    public let identifier: String?
    public let title: String
    public let description: String?
    // The BUILTIN ANCHOR (EXP-314): still the 7-value `issue_status` enum, and
    // still dual-written by the server for every status change. Custom statuses
    // anchor to their category's builtin, so anchor-keyed surfaces (cross-team
    // grouping, terminal-set checks, coding gates) keep working verbatim.
    public let status: String
    /// The team's `issue_statuses` row this issue sits in (EXP-314). NULL on a
    /// pre-backfill snapshot — resolution falls back to the team row matching
    /// `status`, then to a constructed builtin default.
    public let statusId: String?
    public let priority: String
    public let assigneeId: String?
    public let creatorId: String?
    // Issue origin ('user' | 'widget'). Tolerant/nullable — a pre-rotation
    // snapshot or an older row may omit it.
    public let source: String?
    public let dueDate: String?
    public let sortOrder: Double?
    public let completedAt: String?
    // Duplicate resolution: the canonical issue this one duplicates (pairs with
    // status='duplicate'). 1:1, no relation graph.
    public let duplicateOfId: String?
    // EXP-1291: PR and branch state left `issues` — it lives on the synced
    // `pull_requests` + `pull_request_issues` rows.
    /// EXP-630: story points — a point number whatever the team's scale
    /// (`teams.estimation_type` decides how it renders). NULL = no estimate.
    public let estimate: Int?
    /// EXP-1231: the issue draft this issue was created from (`issues.draft_id`).
    /// Every open draft page watches for it: an issue carrying its draft id =
    /// the draft was created elsewhere. NULL for issues not made from a draft.
    public let draftId: String?
    public let createdAt: String
    public let updatedAt: String
    /// #EXP-1383: a team AGENT assignee (`issues.assignee_agent_id`),
    /// mutually exclusive with `assigneeId` (agent-settings.json `assignee`).
    public let assigneeAgentId: String?
    /// #EXP-1383: the agent whose session created the issue (`creatorId`
    /// keeps the member whose conversation it was).
    public let creatorAgentId: String?

    public init(
        id: String,
        boardId: String,
        number: Int?,
        identifier: String?,
        title: String,
        description: String?,
        status: String,
        statusId: String? = nil,
        priority: String,
        assigneeId: String?,
        creatorId: String?,
        source: String?,
        dueDate: String?,
        sortOrder: Double?,
        completedAt: String?,
        duplicateOfId: String?,
        estimate: Int? = nil,
        draftId: String? = nil,
        createdAt: String,
        updatedAt: String,
        assigneeAgentId: String? = nil,
        creatorAgentId: String? = nil
    ) {
        self.assigneeAgentId = assigneeAgentId
        self.creatorAgentId = creatorAgentId
        self.id = id
        self.boardId = boardId
        self.number = number
        self.identifier = identifier
        self.title = title
        self.description = description
        self.status = status
        self.statusId = statusId
        self.priority = priority
        self.assigneeId = assigneeId
        self.creatorId = creatorId
        self.source = source
        self.dueDate = dueDate
        self.sortOrder = sortOrder
        self.completedAt = completedAt
        self.duplicateOfId = duplicateOfId
        self.estimate = estimate
        self.draftId = draftId
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, title, description, status, priority, number, identifier, source
        case statusId = "status_id"
        case boardId = "board_id"
        case assigneeId = "assignee_id"
        case creatorId = "creator_id"
        case dueDate = "due_date"
        case sortOrder = "sort_order"
        case completedAt = "completed_at"
        case duplicateOfId = "duplicate_of_id"
        case estimate
        case draftId = "draft_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case assigneeAgentId = "assignee_agent_id"
        case creatorAgentId = "creator_agent_id"
    }
}

// Custom Codable: number / sort_order come off the Electric wire as
// JSON strings (Postgres text) but as native scalars from tRPC/fixtures — decode
// them through the type-aware wire helpers. `description` is plain GFM markdown
// text (was jsonb `{ text }`) and decodes as an ordinary optional string.
extension IssueEntity: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        boardId = try container.decode(String.self, forKey: .boardId)
        number = try container.decodeWireInt(forKey: .number)
        identifier = try container.decodeIfPresent(String.self, forKey: .identifier)
        title = try container.decode(String.self, forKey: .title)
        description = try container.decodeIfPresent(String.self, forKey: .description)
        status = try container.decode(String.self, forKey: .status)
        statusId = try container.decodeIfPresent(String.self, forKey: .statusId)
        priority = try container.decode(String.self, forKey: .priority)
        assigneeId = try container.decodeIfPresent(String.self, forKey: .assigneeId)
        creatorId = try container.decodeIfPresent(String.self, forKey: .creatorId)
        source = try container.decodeIfPresent(String.self, forKey: .source)
        dueDate = try container.decodeIfPresent(String.self, forKey: .dueDate)
        sortOrder = try container.decodeWireDouble(forKey: .sortOrder)
        completedAt = try container.decodeIfPresent(String.self, forKey: .completedAt)
        duplicateOfId = try container.decodeIfPresent(String.self, forKey: .duplicateOfId)
        // EXP-630: an integer off the wire as Postgres text, a scalar from
        // tRPC; absent on a pre-rotation snapshot.
        estimate = try container.decodeWireInt(forKey: .estimate)
        // EXP-1231: absent on a pre-rotation snapshot.
        draftId = try container.decodeIfPresent(String.self, forKey: .draftId)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)
        // #EXP-1383: absent on a pre-rotation snapshot.
        assigneeAgentId = try container.decodeIfPresent(String.self, forKey: .assigneeAgentId)
        creatorAgentId = try container.decodeIfPresent(String.self, forKey: .creatorAgentId)
    }
}

// MARK: - Session (EXP-1291)

// ONE session tree. A thread IS a root session (`parent_id` NULL); an
// agent's conversation is a root with `agent_id` (its name, icon, color and
// order come from that `agents` definition); children are sub-sessions on
// any device (`depth` > 0). The root carries the thread facets (title, the
// ONE Guide); issues and PRs hang off it through `session_issues` /
// `pull_requests`. NEW-8: the legacy columns (`kind`, `dispatch`, `caption`,
// `aside`, `move_to_device_id`, the action and agent mirrors) are never read;
// the decoder ignores them while the server still sends them. Visibility is the shape's
// (`shared OR user_id = me`), so every row here may be shown. Mirrors the
// sessions shape allowlist (`host_user_id`, `mcp_server_ids` and an agent's
// `instructions` stay server-only).
public struct SessionEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "sessions"

    public let id: String
    public let teamId: String
    /// nil = a root (a thread or an agent).
    public let parentId: String?
    /// The root's id — its own on a root.
    public let rootId: String
    /// 0 on a root.
    public let depth: Int
    /// The STARTER (visibility keys on it).
    public let userId: String
    /// The steer device id of the machine that runs it.
    public let deviceId: String
    /// Contract `codingAgent` (claude/codex); nil = the device's default.
    public let cli: String?
    /// The agent login profile on that device; nil = its last used one.
    public let account: String?
    public let model: String?
    public let effort: String?
    public let planMode: Bool
    public let computerUse: Bool
    public let ultracode: Bool
    public let repositoryId: String?
    /// The working branch, device-written at claim.
    public let branch: String?
    /// `queued` | `running` | `idle` | `ended` (`DomainContract.sessionStatusValues`).
    public let status: String
    /// Inside a turn right now — the ONLY list-spinner input (EXP-848).
    public let busy: Bool
    /// A question/plan card waits for a person.
    public let needsInput: Bool
    /// EXP-804: the usage wall, the raw jsonb TEXT; nil = not blocked.
    public let blocked: String?
    public let startedAt: String?
    public let lastTurnAt: String?
    public let endedAt: String?
    /// Stamped at creation: the device is shared with the team.
    public let shared: Bool
    /// Roots: the thread title (nil = derived for display); children: the
    /// CLI's auto-name.
    public let title: String?
    /// The thread's ONE Guide (every PR body), raw jsonb TEXT in the
    /// `{topic, …}` entry shape `parseSessionResults` reads; nil / `[]` =
    /// nothing yet.
    public let guide: String?
    public let createdAt: String
    public let updatedAt: String
    /// NEW-2: set = this root is an agent's conversation session (one per
    /// agent and member); the kind derives from it (`sessionKindOf`).
    public let agentId: String?
    /// VAPP-110: set = this root is a member's EDITOR session of that app
    /// (one per app and member); like an agent's conversation it is no
    /// thread (`SessionTree.partitionThreads`).
    public let appId: String?
    /// #EXP-1383: when this session last fetched its agent's brief
    /// (`sessions.agent_instructions_at`; chats.json `agentHint`).
    public let agentInstructionsAt: String?
    /// #EXP-1383: the agent whose session started this root (an agent tree).
    public let startedByAgentId: String?

    public init(
        id: String,
        teamId: String,
        parentId: String? = nil,
        rootId: String? = nil,
        depth: Int = 0,
        userId: String,
        deviceId: String,
        cli: String? = nil,
        account: String? = nil,
        model: String? = nil,
        effort: String? = nil,
        planMode: Bool = false,
        computerUse: Bool = false,
        ultracode: Bool = false,
        repositoryId: String? = nil,
        branch: String? = nil,
        status: String = DomainContract.sessionStatusQueued,
        busy: Bool = false,
        needsInput: Bool = false,
        blocked: String? = nil,
        startedAt: String? = nil,
        lastTurnAt: String? = nil,
        endedAt: String? = nil,
        shared: Bool = false,
        title: String? = nil,
        guide: String? = nil,
        createdAt: String,
        updatedAt: String,
        agentId: String? = nil,
        appId: String? = nil,
        agentInstructionsAt: String? = nil,
        startedByAgentId: String? = nil
    ) {
        self.agentId = agentId
        self.appId = appId
        self.agentInstructionsAt = agentInstructionsAt
        self.startedByAgentId = startedByAgentId
        self.id = id
        self.teamId = teamId
        self.parentId = parentId
        self.rootId = rootId ?? id
        self.depth = depth
        self.userId = userId
        self.deviceId = deviceId
        self.cli = cli
        self.account = account
        self.model = model
        self.effort = effort
        self.planMode = planMode
        self.computerUse = computerUse
        self.ultracode = ultracode
        self.repositoryId = repositoryId
        self.branch = branch
        self.status = status
        self.busy = busy
        self.needsInput = needsInput
        self.blocked = blocked
        self.startedAt = startedAt
        self.lastTurnAt = lastTurnAt
        self.endedAt = endedAt
        self.shared = shared
        self.title = title
        self.guide = guide
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    public var isRoot: Bool { parentId == nil }
    /// NEW-2: an agent's conversation session (`agentId` set).
    public var isAgent: Bool { sessionKindOf(agentId: agentId) == DomainContract.sessionKindAgent }
    /// EXP-1341: the kind's traits (`persistent`, `triggers`): branch on
    /// these, never on the kind string (NEW-2: the kind derives from `agentId`).
    public var traits: SessionTraits { SessionKind.traits(sessionKindOf(agentId: agentId)) }
    /// Not `ended` (queued, running and idle sessions all take messages).
    public var isLive: Bool { status != DomainContract.sessionStatusEnded }

    enum CodingKeys: String, CodingKey {
        case id, depth, cli, account, model, effort, ultracode, branch, status
        case busy, blocked, shared, title, guide
        case teamId = "team_id"
        case parentId = "parent_id"
        case rootId = "root_id"
        case userId = "user_id"
        case deviceId = "device_id"
        case planMode = "plan_mode"
        case computerUse = "computer_use"
        case repositoryId = "repository_id"
        case needsInput = "needs_input"
        case startedAt = "started_at"
        case lastTurnAt = "last_turn_at"
        case endedAt = "ended_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case agentId = "agent_id"
        case appId = "app_id"
        case agentInstructionsAt = "agent_instructions_at"
        case startedByAgentId = "started_by_agent_id"
    }
}

// Custom decode: bools/numbers arrive as Postgres text off the Electric wire
// and as native scalars from fixtures/tRPC; jsonb columns are stored as their
// TEXT through the non-throwing helper, so a malformed Guide degrades one
// column, never the row. Keys this type no longer names (the NEW-8 legacy
// columns) are ignored, present or not. A same-file extension keeps
// encode(to:) synthesis.
extension SessionEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        parentId = try c.decodeIfPresent(String.self, forKey: .parentId)
        rootId = try c.decodeIfPresent(String.self, forKey: .rootId) ?? id
        depth = try c.decodeWireInt(forKey: .depth) ?? 0
        userId = try c.decode(String.self, forKey: .userId)
        deviceId = try c.decode(String.self, forKey: .deviceId)
        cli = try c.decodeIfPresent(String.self, forKey: .cli)
        account = try c.decodeIfPresent(String.self, forKey: .account)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        effort = try c.decodeIfPresent(String.self, forKey: .effort)
        planMode = c.decodeWireBool(forKey: .planMode, default: false)
        computerUse = c.decodeWireBool(forKey: .computerUse, default: false)
        ultracode = c.decodeWireBool(forKey: .ultracode, default: false)
        repositoryId = try c.decodeIfPresent(String.self, forKey: .repositoryId)
        branch = try c.decodeIfPresent(String.self, forKey: .branch)
        status = try c.decodeIfPresent(String.self, forKey: .status) ?? DomainContract.sessionStatusQueued
        busy = c.decodeWireBool(forKey: .busy, default: false)
        needsInput = c.decodeWireBool(forKey: .needsInput, default: false)
        blocked = c.decodeWireJsonString(forKey: .blocked)
        startedAt = try c.decodeIfPresent(String.self, forKey: .startedAt)
        lastTurnAt = try c.decodeIfPresent(String.self, forKey: .lastTurnAt)
        endedAt = try c.decodeIfPresent(String.self, forKey: .endedAt)
        shared = c.decodeWireBool(forKey: .shared, default: false)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        guide = c.decodeWireJsonString(forKey: .guide)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
        // NEW-2: absent on a pre-NEW-2 server = a thread.
        agentId = try c.decodeIfPresent(String.self, forKey: .agentId)
        // VAPP-110: absent on a pre-apps server = not an editor session.
        appId = try c.decodeIfPresent(String.self, forKey: .appId)
        // #EXP-1383: absent on a pre-rotation snapshot.
        agentInstructionsAt = try c.decodeIfPresent(String.self, forKey: .agentInstructionsAt)
        startedByAgentId = try c.decodeIfPresent(String.self, forKey: .startedByAgentId)
    }
}

// MARK: - SessionIssue (EXP-1291)

// A thread's issue (n ↔ n: an issue may sit in several threads), always on
// the ROOT session. Visible like its thread and hidden with the issue's
// board (the shape's static trash/archive predicates); the board and
// visibility mirrors stay server-only. `source` ∈ contract
// `sessionIssueSource` (`user` | `agent`; an agent link also names the
// session that made it).
public struct SessionIssueEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "session_issues"

    public let id: String
    /// The ROOT session.
    public let sessionId: String
    public let issueId: String
    public let teamId: String
    public let source: String
    public let addedBy: String?
    public let addedBySessionId: String?
    /// Ascending = display order within the thread.
    public let sortOrder: Double
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        sessionId: String,
        issueId: String,
        teamId: String,
        source: String = DomainContract.sessionIssueSourceUser,
        addedBy: String? = nil,
        addedBySessionId: String? = nil,
        sortOrder: Double = 0,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.sessionId = sessionId
        self.issueId = issueId
        self.teamId = teamId
        self.source = source
        self.addedBy = addedBy
        self.addedBySessionId = addedBySessionId
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, source
        case sessionId = "session_id"
        case issueId = "issue_id"
        case teamId = "team_id"
        case addedBy = "added_by"
        case addedBySessionId = "added_by_session_id"
        case sortOrder = "sort_order"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension SessionIssueEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        sessionId = try c.decode(String.self, forKey: .sessionId)
        issueId = try c.decode(String.self, forKey: .issueId)
        teamId = try c.decode(String.self, forKey: .teamId)
        source = try c.decodeIfPresent(String.self, forKey: .source)
            ?? DomainContract.sessionIssueSourceUser
        addedBy = try c.decodeIfPresent(String.self, forKey: .addedBy)
        addedBySessionId = try c.decodeIfPresent(String.self, forKey: .addedBySessionId)
        sortOrder = try c.decodeWireDouble(forKey: .sortOrder) ?? 0
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Message (EXP-1291)

// The Conversation: one row per person message, per agent turn reply (the
// turn's FINAL text) and per card, in the CONVERSATION that holds it
// (`root_id` names its thread/agent). NEW-8: a message never names its
// session; it belongs to the session of its conversation
// (`MessageConversations`, read through `SessionMessage`). `card` = the raw
// jsonb TEXT of a `MessageCard` (`{kind, …}`, contract `messageCardKind`; an
// unknown kind renders as nothing); `consumed_at` nil on a person's message =
// still queued for the device.
public struct MessageEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "messages"

    public let id: String
    /// NEW-3/NEW-8: the conversation it lives in (`messages.conversation_id`,
    /// NOT NULL on the server); a legacy `session_id` on the wire is ignored.
    public let conversationId: String
    public let rootId: String
    public let teamId: String
    /// `user` | `agent` | `system` (`DomainContract.messageAuthorKindValues`).
    public let authorKind: String
    /// The person (author kind `user`); nil for agent/system rows.
    public let authorId: String?
    /// Plain GFM; empty on a pure card.
    public let body: String
    public let card: String?
    public let answeredAt: String?
    public let consumedAt: String?
    public let createdAt: String
    public let updatedAt: String
    /// EXP-1323: the session that posted it (a child asking its parent);
    /// nil = none.
    public let authorSessionId: String?

    public init(
        id: String,
        conversationId: String,
        rootId: String,
        teamId: String,
        authorKind: String,
        authorId: String? = nil,
        body: String = "",
        card: String? = nil,
        answeredAt: String? = nil,
        consumedAt: String? = nil,
        createdAt: String,
        updatedAt: String,
        authorSessionId: String? = nil
    ) {
        self.authorSessionId = authorSessionId
        self.conversationId = conversationId
        self.id = id
        self.rootId = rootId
        self.teamId = teamId
        self.authorKind = authorKind
        self.authorId = authorId
        self.body = body
        self.card = card
        self.answeredAt = answeredAt
        self.consumedAt = consumedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, body, card
        case rootId = "root_id"
        case teamId = "team_id"
        case authorKind = "author_kind"
        case authorId = "author_id"
        case authorSessionId = "author_session_id"
        case conversationId = "conversation_id"
        case answeredAt = "answered_at"
        case consumedAt = "consumed_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension MessageEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        conversationId = try c.decode(String.self, forKey: .conversationId)
        rootId = try c.decode(String.self, forKey: .rootId)
        teamId = try c.decode(String.self, forKey: .teamId)
        authorKind = try c.decode(String.self, forKey: .authorKind)
        authorId = try c.decodeIfPresent(String.self, forKey: .authorId)
        body = try c.decodeIfPresent(String.self, forKey: .body) ?? ""
        card = c.decodeWireJsonString(forKey: .card)
        answeredAt = try c.decodeIfPresent(String.self, forKey: .answeredAt)
        consumedAt = try c.decodeIfPresent(String.self, forKey: .consumedAt)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
        authorSessionId = try c.decodeIfPresent(String.self, forKey: .authorSessionId)
    }
}

// NEW-8: a message KEYED to the session of its conversation
// (`MessageConversations.keyMessagesBySession`, contract
// `message-conversation.json`): fetched by `SessionQueries.messages`, which
// joins the synced `conversations` — a dm/channel message or one whose
// conversation has not synced yet never reaches it.
@dynamicMemberLookup
public struct SessionMessage: FetchableRecord, Identifiable, Equatable, Sendable {
    public let message: MessageEntity
    /// The RESOLVED session: its conversation's `session_id`.
    public let sessionId: String

    public var id: String { message.id }

    public init(message: MessageEntity, sessionId: String) {
        self.message = message
        self.sessionId = sessionId
    }

    /// A `messages` row joined with its conversation's session as
    /// `conversation_session_id` (`SessionQueries.keyedMessagesSQL`).
    public init(row: Row) throws {
        message = try MessageEntity(row: row)
        sessionId = row["conversation_session_id"]
    }

    public subscript<T>(dynamicMember keyPath: KeyPath<MessageEntity, T>) -> T {
        message[keyPath: keyPath]
    }
}

// MARK: - Conversation (NEW-3)

// The chat primitive's row: `kind` dm | channel | session (contract
// `conversationKind`). In step 1 every row is a session's own (kind
// `session`, `session_id` set) and syncs on the session rule; dm/channel
// readers (`conversation_members`) stay server-only until NEW-8.
public struct ConversationEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "conversations"

    public let id: String
    public let teamId: String
    public let kind: String
    public let sessionId: String?
    public let title: String?
    public let createdAt: String
    public let updatedAt: String
    /// Thread e4a04d22: the server-kept list digest (`conversations.digest`
    /// jsonb, stored as its JSON text like `messages.card`): every Chats list
    /// reads `Chats.digestMessages` over it instead of the synced messages.
    public let digest: String?

    public init(
        id: String,
        teamId: String,
        kind: String = "session",
        sessionId: String? = nil,
        title: String? = nil,
        createdAt: String,
        updatedAt: String,
        digest: String? = nil
    ) {
        self.id = id
        self.teamId = teamId
        self.kind = kind
        self.sessionId = sessionId
        self.title = title
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.digest = digest
    }

    enum CodingKeys: String, CodingKey {
        case id, kind, title, digest
        case teamId = "team_id"
        case sessionId = "session_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension ConversationEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? "session"
        sessionId = try c.decodeIfPresent(String.self, forKey: .sessionId)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
        digest = c.decodeWireJsonString(forKey: .digest)
    }
}

// MARK: - SessionRead (EXP-1323)

// The viewer's read marks: ONE row per (user, session), stamped by
// `sessions.markRead` when a thread or a session view opens. Static per user
// (`user_id = me`) like `pins`; the Threads list folds them into `unread`
// (bold titles + the Threads tab dot).
public struct SessionReadEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "session_reads"

    public let id: String
    public let userId: String
    public let sessionId: String
    public let readAt: String
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        userId: String,
        sessionId: String,
        readAt: String,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.userId = userId
        self.sessionId = sessionId
        self.readAt = readAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id
        case userId = "user_id"
        case sessionId = "session_id"
        case readAt = "read_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - PullRequest (EXP-1291)

// A thread's pull request (a tree, a stack or independent) — all PR and
// branch state lives here, never on `issues`. Team-visible.
// `basePullRequestId` = the open PR this one is stacked on (native GitHub
// stacks); `state` ∈ contract `prState`.
public struct PullRequestEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "pull_requests"

    public let id: String
    public let teamId: String
    public let rootSessionId: String?
    /// The session that opened it.
    public let sessionId: String?
    public let repositoryId: String
    public let number: Int
    public let url: String
    public let title: String?
    public let head: String
    public let base: String
    public let basePullRequestId: String?
    public let state: String
    public let mergedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        rootSessionId: String? = nil,
        sessionId: String? = nil,
        repositoryId: String,
        number: Int,
        url: String,
        title: String? = nil,
        head: String,
        base: String,
        basePullRequestId: String? = nil,
        state: String = DomainContract.prStateOpen,
        mergedAt: String? = nil,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.rootSessionId = rootSessionId
        self.sessionId = sessionId
        self.repositoryId = repositoryId
        self.number = number
        self.url = url
        self.title = title
        self.head = head
        self.base = base
        self.basePullRequestId = basePullRequestId
        self.state = state
        self.mergedAt = mergedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    public var isOpen: Bool { state == DomainContract.prStateOpen }

    enum CodingKeys: String, CodingKey {
        case id, number, url, title, head, base, state
        case teamId = "team_id"
        case rootSessionId = "root_session_id"
        case sessionId = "session_id"
        case repositoryId = "repository_id"
        case basePullRequestId = "base_pull_request_id"
        case mergedAt = "merged_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension PullRequestEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        rootSessionId = try c.decodeIfPresent(String.self, forKey: .rootSessionId)
        sessionId = try c.decodeIfPresent(String.self, forKey: .sessionId)
        repositoryId = try c.decode(String.self, forKey: .repositoryId)
        number = try c.decodeWireInt(forKey: .number) ?? 0
        url = try c.decode(String.self, forKey: .url)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        head = try c.decode(String.self, forKey: .head)
        base = try c.decode(String.self, forKey: .base)
        basePullRequestId = try c.decodeIfPresent(String.self, forKey: .basePullRequestId)
        state = try c.decodeIfPresent(String.self, forKey: .state) ?? DomainContract.prStateOpen
        mergedAt = try c.decodeIfPresent(String.self, forKey: .mergedAt)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - PullRequestIssue (EXP-1291)

// Which issues a PR closes (n ↔ n). Team-scoped with the issue's static
// trash/archive predicates; the board mirrors stay server-only.
public struct PullRequestIssueEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "pull_request_issues"

    public let id: String
    public let pullRequestId: String
    public let issueId: String
    public let teamId: String
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        pullRequestId: String,
        issueId: String,
        teamId: String,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.pullRequestId = pullRequestId
        self.issueId = issueId
        self.teamId = teamId
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id
        case pullRequestId = "pull_request_id"
        case issueId = "issue_id"
        case teamId = "team_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - Agents, events, prompts (NEW-2)

// NEW-2: a team agent is a DEFINITION, never a session; it runs as one
// conversation session per member (`sessions.agent_id`). The `agents` shape
// allowlist; `instructions` + `trigger_state` stay server-only. `triggers` =
// the raw jsonb TEXT (`AgentTriggers.parse` reads it); "[]" = none.
public struct AgentDefinitionEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "agents"

    public let id: String
    public let teamId: String
    public let name: String
    public let icon: String?
    public let color: String?
    public let triggers: String
    /// The steer device its conversations run on; nil = unset.
    public let deviceId: String?
    public let cli: String?
    public let account: String?
    public let model: String?
    public let ownerId: String?
    public let sortOrder: Double
    public let createdAt: String
    public let updatedAt: String
    /// #EXP-1383: when the instructions TEXT last changed
    /// (`agents.instructions_updated_at`; chats.json `agentHint`).
    public let instructionsUpdatedAt: String?

    public init(
        id: String,
        teamId: String,
        name: String,
        icon: String? = nil,
        color: String? = nil,
        triggers: String = "[]",
        deviceId: String? = nil,
        cli: String? = nil,
        account: String? = nil,
        model: String? = nil,
        ownerId: String? = nil,
        sortOrder: Double = 0,
        createdAt: String,
        updatedAt: String,
        instructionsUpdatedAt: String? = nil
    ) {
        self.instructionsUpdatedAt = instructionsUpdatedAt
        self.id = id
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.color = color
        self.triggers = triggers
        self.deviceId = deviceId
        self.cli = cli
        self.account = account
        self.model = model
        self.ownerId = ownerId
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, icon, color, triggers, cli, account, model
        case teamId = "team_id"
        case deviceId = "device_id"
        case ownerId = "owner_id"
        case sortOrder = "sort_order"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case instructionsUpdatedAt = "instructions_updated_at"
    }
}

extension AgentDefinitionEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        icon = try c.decodeIfPresent(String.self, forKey: .icon)
        color = try c.decodeIfPresent(String.self, forKey: .color)
        triggers = c.decodeWireJsonString(forKey: .triggers) ?? "[]"
        deviceId = try c.decodeIfPresent(String.self, forKey: .deviceId)
        cli = try c.decodeIfPresent(String.self, forKey: .cli)
        account = try c.decodeIfPresent(String.self, forKey: .account)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        ownerId = try c.decodeIfPresent(String.self, forKey: .ownerId)
        sortOrder = try c.decodeWireDouble(forKey: .sortOrder) ?? 0
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
        // #EXP-1383: absent on a pre-rotation snapshot.
        instructionsUpdatedAt = try c.decodeIfPresent(String.self, forKey: .instructionsUpdatedAt)
    }
}

// NEW-2: an event (free-text `name`, jsonb `payload` stored as TEXT) the
// server, a person or a session emitted; team-scoped, append-only. Agent
// triggers of kind `event` fire on its name (server-side).
public struct EventEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "events"

    public let id: String
    public let teamId: String
    public let name: String
    /// The raw jsonb TEXT; nil = none.
    public let payload: String?
    /// `session` | `user` | `server` (contract `eventEmitterValues`).
    public let emittedBy: String
    public let sessionId: String?
    public let userId: String?
    public let createdAt: String

    public init(
        id: String,
        teamId: String,
        name: String,
        payload: String? = nil,
        emittedBy: String = "server",
        sessionId: String? = nil,
        userId: String? = nil,
        createdAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.name = name
        self.payload = payload
        self.emittedBy = emittedBy
        self.sessionId = sessionId
        self.userId = userId
        self.createdAt = createdAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, payload
        case teamId = "team_id"
        case emittedBy = "emitted_by"
        case sessionId = "session_id"
        case userId = "user_id"
        case createdAt = "created_at"
    }
}

extension EventEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        payload = c.decodeWireJsonString(forKey: .payload)
        emittedBy = try c.decodeIfPresent(String.self, forKey: .emittedBy) ?? "server"
        sessionId = try c.decodeIfPresent(String.self, forKey: .sessionId)
        userId = try c.decodeIfPresent(String.self, forKey: .userId)
        createdAt = try c.decode(String.self, forKey: .createdAt)
    }
}

// NEW-2: a saved prompt (replaces actions): the `prompts` shape allowlist;
// the `body` is server-only (tRPC `prompts.get`).
public struct PromptEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "prompts"

    public let id: String
    public let teamId: String
    public let name: String
    /// Curated registry icon name; nil = the generic prompt glyph.
    public let icon: String?
    public let sortOrder: Double
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        name: String,
        icon: String? = nil,
        sortOrder: Double = 0,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, icon
        case teamId = "team_id"
        case sortOrder = "sort_order"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension PromptEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        icon = try c.decodeIfPresent(String.self, forKey: .icon)
        sortOrder = try c.decodeWireDouble(forKey: .sortOrder) ?? 0
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Pin

// MARK: - Apps (VAPP-110)

// VAPP-110: a team's APP (fixtures/apps.json) — a declarative Exponential UI
// package rendered through the client's app host. The `apps` shape allowlist;
// `package` = the HEAD version's package as its raw jsonb TEXT (nil = no
// version yet), so the editor's preview repaints on every synced upsert.
// `deleted_at` is the trash mirror (readers hide trashed rows).
public struct AppEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "apps"

    public let id: String
    public let teamId: String
    public let slug: String
    public let name: String
    /// The board icon set (a curated registry name); nil = none.
    public let icon: String?
    public let color: String?
    public let description: String
    public let package: String?
    public let headVersion: Int
    public let createdBy: String
    /// VAPP-120: the exact `exponential.*` function names the app may run as
    /// the viewer without the consent card (`AppActions.grantAllows`).
    public let grantedTools: [String]
    public let deletedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        slug: String,
        name: String,
        icon: String? = nil,
        color: String? = nil,
        description: String = "",
        package: String? = nil,
        headVersion: Int = 0,
        createdBy: String,
        grantedTools: [String] = [],
        deletedAt: String? = nil,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.slug = slug
        self.name = name
        self.icon = icon
        self.color = color
        self.description = description
        self.package = package
        self.headVersion = headVersion
        self.createdBy = createdBy
        self.grantedTools = grantedTools
        self.deletedAt = deletedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, slug, name, icon, color, description, package
        case grantedTools = "granted_tools"
        case teamId = "team_id"
        case headVersion = "head_version"
        case createdBy = "created_by"
        case deletedAt = "deleted_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension AppEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        slug = try c.decodeIfPresent(String.self, forKey: .slug) ?? ""
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        icon = try c.decodeIfPresent(String.self, forKey: .icon)
        color = try c.decodeIfPresent(String.self, forKey: .color)
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        package = c.decodeWireJsonString(forKey: .package)
        headVersion = try c.decodeWireInt(forKey: .headVersion) ?? 0
        createdBy = try c.decodeIfPresent(String.self, forKey: .createdBy) ?? ""
        // The jsonb list as its JSON text (wire or store); missing or
        // garbage = no grants.
        grantedTools = c.decodeWireJsonString(forKey: .grantedTools)
            .flatMap { try? JSONDecoder().decode([String].self, from: Data($0.utf8)) } ?? []
        deletedAt = try c.decodeIfPresent(String.self, forKey: .deletedAt)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// VAPP-110: one VERSION of an app's package (the editor's Versions list).
// The `app_versions` shape allowlist EXCLUDES `package`: an older version's
// package loads on demand (tRPC `apps.get {version}`).
public struct AppVersionEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "app_versions"

    public let id: String
    public let appId: String
    public let teamId: String
    public let number: Int
    public let note: String?
    /// The editor session that wrote it; nil = a person's restore.
    public let sessionId: String?
    /// The person who wrote it; nil = the editor session.
    public let createdBy: String?
    public let createdAt: String

    public init(
        id: String,
        appId: String,
        teamId: String,
        number: Int,
        note: String? = nil,
        sessionId: String? = nil,
        createdBy: String? = nil,
        createdAt: String
    ) {
        self.id = id
        self.appId = appId
        self.teamId = teamId
        self.number = number
        self.note = note
        self.sessionId = sessionId
        self.createdBy = createdBy
        self.createdAt = createdAt
    }

    enum CodingKeys: String, CodingKey {
        case id, number, note
        case appId = "app_id"
        case teamId = "team_id"
        case sessionId = "session_id"
        case createdBy = "created_by"
        case createdAt = "created_at"
    }
}

extension AppVersionEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        appId = try c.decode(String.self, forKey: .appId)
        teamId = try c.decode(String.self, forKey: .teamId)
        number = try c.decodeWireInt(forKey: .number) ?? 0
        note = try c.decodeIfPresent(String.self, forKey: .note)
        sessionId = try c.decodeIfPresent(String.self, forKey: .sessionId)
        createdBy = try c.decodeIfPresent(String.self, forKey: .createdBy)
        createdAt = try c.decode(String.self, forKey: .createdAt)
    }
}

// VAPP-122: one RECORD of an app (fixtures/app-records.json) — a JSON object
// (`data`) of a `kind` the app's manifest declares, stored per team. The
// `app-records` shape allowlist; `data` = the jsonb as its raw TEXT (like
// `apps.package`). Trash never syncs (static shape predicates).
public struct AppRecordEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "app_records"

    public let id: String
    public let appId: String
    public let teamId: String
    public let kind: String
    /// The record's `data` object as JSON text ("{}" when absent).
    public let data: String
    public let createdBy: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        appId: String,
        teamId: String,
        kind: String,
        data: String = "{}",
        createdBy: String? = nil,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.appId = appId
        self.teamId = teamId
        self.kind = kind
        self.data = data
        self.createdBy = createdBy
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, kind, data
        case appId = "app_id"
        case teamId = "team_id"
        case createdBy = "created_by"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension AppRecordEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        appId = try c.decode(String.self, forKey: .appId)
        teamId = try c.decode(String.self, forKey: .teamId)
        kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? ""
        data = c.decodeWireJsonString(forKey: .data) ?? "{}"
        createdBy = try c.decodeIfPresent(String.self, forKey: .createdBy)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// EXP-778: personal pins — the 21st Electric shape. ONE row per (user,
// target): an issue or a session the user pinned into the sidebar. The shape
// is static per user (`user_id = me`), NOT team/trash scoped, so the local
// table holds every team's pins. Synced for client parity only: the phone has
// no sidebar, so nothing here renders a pin (EXP-858 dropped the controls,
// EXP-976 the switcher's read-only list). `kind` ∈ contract `pinKind`; NEW-8:
// a legacy `action` pin has no target here (`targetId` nil = ignored).
// Mirrors packages/db-schema pins minus `action_id`.
public struct PinEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "pins"

    public let id: String
    public let userId: String
    public let teamId: String
    /// `issue` | `session` (`DomainContract.pinKindValues`; a legacy
    /// `action` pin is ignored).
    public let kind: String
    public let issueId: String?
    public let sessionId: String?
    /// Ascending = display order.
    public let sortOrder: Double?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        userId: String,
        teamId: String,
        kind: String,
        issueId: String? = nil,
        sessionId: String? = nil,
        sortOrder: Double?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.userId = userId
        self.teamId = teamId
        self.kind = kind
        self.issueId = issueId
        self.sessionId = sessionId
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    /// The one target id the row points at, whichever column carries it;
    /// nil = no target (a legacy `action` pin or an unknown kind: ignored).
    public var targetId: String? {
        switch kind {
        case DomainContract.pinKindIssue: issueId
        case DomainContract.pinKindSession: sessionId
        default: nil
        }
    }

    enum CodingKeys: String, CodingKey {
        case id, kind
        case userId = "user_id"
        case teamId = "team_id"
        case issueId = "issue_id"
        case sessionId = "session_id"
        case sortOrder = "sort_order"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom decode: `sort_order` comes off the Electric wire as Postgres text
// (the issues precedent) and as a native number from fixtures.
extension PinEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        userId = try c.decode(String.self, forKey: .userId)
        teamId = try c.decode(String.self, forKey: .teamId)
        kind = try c.decode(String.self, forKey: .kind)
        issueId = try c.decodeIfPresent(String.self, forKey: .issueId)
        sessionId = try c.decodeIfPresent(String.self, forKey: .sessionId)
        sortOrder = try c.decodeWireDouble(forKey: .sortOrder)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - IssueDraft (EXP-878)

// A composed-but-unfiled issue, kept per USER (22nd Electric shape, static
// `user_id = me` like `pins` — never team- or trash-scoped). A row renders
// only when its board resolves locally, exactly like a pin resolves its
// target. `status_id` NULL means the team's Backlog builtin; `label_ids` is a
// Postgres `uuid[]` on the wire (`{a,b}`), stored here the way
// `devices.shared_team_ids` is.
public struct IssueDraftEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "issue_drafts"

    public let id: String
    public let userId: String
    public let teamId: String
    public let boardId: String
    /// May be empty — a draft is content-bearing through its description or
    /// its attachments too.
    public let title: String
    public let description: String?
    /// A team `issue_statuses` row id; nil = the team's Backlog builtin.
    public let statusId: String?
    /// The issue-priority enum value (`none` when the wire omits it).
    public let priority: String
    public let assigneeId: String?
    public let labelIds: [String]
    /// `YYYY-MM-DD`, date-only like `issues.due_date` (REV2-49).
    public let dueDate: String?
    /// EXP-1291: the root session (thread) a "New issue in this thread"
    /// draft files into (FK SET NULL server-side: a deleted thread leaves a
    /// plain draft).
    public let sessionId: String?
    public let createdAt: String
    public let updatedAt: String
    /// #EXP-1383: a team AGENT assignee, exclusive with `assigneeId`.
    public let assigneeAgentId: String?

    public init(
        id: String,
        userId: String,
        teamId: String,
        boardId: String,
        title: String = "",
        description: String? = nil,
        statusId: String? = nil,
        priority: String = IssuePriority.none.rawValue,
        assigneeId: String? = nil,
        labelIds: [String] = [],
        dueDate: String? = nil,
        sessionId: String? = nil,
        createdAt: String = "",
        updatedAt: String = "",
        assigneeAgentId: String? = nil
    ) {
        self.assigneeAgentId = assigneeAgentId
        self.id = id
        self.userId = userId
        self.teamId = teamId
        self.boardId = boardId
        self.title = title
        self.description = description
        self.statusId = statusId
        self.priority = priority
        self.assigneeId = assigneeId
        self.labelIds = labelIds
        self.dueDate = dueDate
        self.sessionId = sessionId
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, title, description, priority
        case userId = "user_id"
        case teamId = "team_id"
        case boardId = "board_id"
        case statusId = "status_id"
        case assigneeId = "assignee_id"
        case labelIds = "label_ids"
        case dueDate = "due_date"
        case sessionId = "session_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case assigneeAgentId = "assignee_agent_id"
    }
}

// Custom decode: a PARTIAL update off the wire carries only the touched
// columns, so everything but the identity columns degrades to its schema
// default rather than killing the row; `label_ids` reads `{a,b}` off the wire
// and `["a","b"]` back out of GRDB through the shared helper (FEED-33's
// precedent). A same-file extension keeps encode(to:) synthesis.
extension IssueDraftEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        userId = try c.decode(String.self, forKey: .userId)
        teamId = try c.decode(String.self, forKey: .teamId)
        boardId = try c.decode(String.self, forKey: .boardId)
        // NOT NULL server-side but empty-able, and absent on a partial.
        title = (try? c.decode(String.self, forKey: .title)) ?? ""
        description = try c.decodeIfPresent(String.self, forKey: .description)
        statusId = try c.decodeIfPresent(String.self, forKey: .statusId)
        priority = (try? c.decode(String.self, forKey: .priority)) ?? IssuePriority.none.rawValue
        assigneeId = try c.decodeIfPresent(String.self, forKey: .assigneeId)
        labelIds = c.decodeWireStringList(forKey: .labelIds)
        dueDate = try c.decodeIfPresent(String.self, forKey: .dueDate)
        sessionId = try c.decodeIfPresent(String.self, forKey: .sessionId)
        createdAt = (try? c.decode(String.self, forKey: .createdAt)) ?? ""
        updatedAt = (try? c.decode(String.self, forKey: .updatedAt)) ?? ""
        assigneeAgentId = try c.decodeIfPresent(String.self, forKey: .assigneeAgentId)
    }
}

// MARK: - Label

public struct LabelEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "labels"

    public let id: String
    public let teamId: String
    public let name: String
    public let color: String
    public let sortOrder: Double?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        name: String,
        color: String,
        sortOrder: Double?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.name = name
        self.color = color
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, color
        case teamId = "team_id"
        case sortOrder = "sort_order"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: sort_order arrives off the Electric wire as a JSON string
// (Postgres text) but as a native number from tRPC/fixtures — decode it through
// the type-aware wire helper. A same-file extension keeps encode(to:) synthesis
// (the same pattern IssueEntity / CommentEntity use).
extension LabelEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        name = try c.decode(String.self, forKey: .name)
        color = try c.decode(String.self, forKey: .color)
        sortOrder = try c.decodeWireDouble(forKey: .sortOrder)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - IssueStatus row

// Custom issue statuses (EXP-314 — the 16th Electric shape). Team-scoped like
// labels. Every team carries 7 LOCKED builtin rows (`builtin_key` = the
// `issue_status` enum value they anchor); custom rows have a NULL
// `builtin_key`. Builtin rows render TODAY's design-token colors, not the
// synced `color` hex — see `ResolvedIssueStatus.color` in ExpUI.
public struct IssueStatusEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "issue_statuses"

    public let id: String
    public let teamId: String
    /// One of `DomainContract.issueStatusCategoryValues`. Kept as a raw string
    /// so a newer server's category never fails hydration — it is typed
    /// tolerantly at use through `IssueStatusCategory.from(_:)`.
    public let category: String
    public let name: String
    /// Hex swatch from the shared label palette. Only CUSTOM rows render it.
    public let color: String?
    public let sortOrder: Double?
    /// Non-nil ⇒ one of the 7 locked builtin rows; nil ⇒ a custom status.
    public let builtinKey: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        category: String,
        name: String,
        color: String? = nil,
        sortOrder: Double? = nil,
        builtinKey: String? = nil,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.category = category
        self.name = name
        self.color = color
        self.sortOrder = sortOrder
        self.builtinKey = builtinKey
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, category, name, color
        case teamId = "team_id"
        case sortOrder = "sort_order"
        case builtinKey = "builtin_key"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom decode: sort_order arrives off the Electric wire as a JSON string
// (Postgres text) but as a native number from tRPC/fixtures — the LabelEntity
// pattern. A same-file extension keeps encode(to:) synthesis.
extension IssueStatusEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        category = try c.decode(String.self, forKey: .category)
        name = try c.decode(String.self, forKey: .name)
        color = try c.decodeIfPresent(String.self, forKey: .color)
        sortOrder = try c.decodeWireDouble(forKey: .sortOrder)
        builtinKey = try c.decodeIfPresent(String.self, forKey: .builtinKey)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - IssueLabel

public struct IssueLabelEntity: Codable, FetchableRecord, PersistableRecord, Equatable, Sendable {
    public static let databaseTableName = "issue_labels"

    public let issueId: String
    public let labelId: String
    public let teamId: String

    public init(issueId: String, labelId: String, teamId: String) {
        self.issueId = issueId
        self.labelId = labelId
        self.teamId = teamId
    }

    enum CodingKeys: String, CodingKey {
        case issueId = "issue_id"
        case labelId = "label_id"
        case teamId = "team_id"
    }
}

// MARK: - IssueRelation

/// EXP-736: a typed link between two issues (blocks / parent / duplicate /
/// related), synced as the 20th shape. The row is stored in its type's
/// CANONICAL direction — `issueId` is the forward side — so a screen renders
/// each row from whichever side it is looking at (see `IssueRelationType`).
/// `boardId` is the SOURCE issue's board (nullable, like the server column).
public struct IssueRelationEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "issue_relations"

    public let id: String
    public let issueId: String
    public let relatedIssueId: String
    /// An `IssueRelationType` raw value.
    public let type: String
    /// `user` (added by hand) or `reference` (an inline `#IDENTIFIER`).
    public let source: String
    public let teamId: String
    public let boardId: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        issueId: String,
        relatedIssueId: String,
        type: String,
        source: String,
        teamId: String,
        boardId: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.issueId = issueId
        self.relatedIssueId = relatedIssueId
        self.type = type
        self.source = source
        self.teamId = teamId
        self.boardId = boardId
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, type, source
        case issueId = "issue_id"
        case relatedIssueId = "related_issue_id"
        case teamId = "team_id"
        case boardId = "board_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - User

public struct UserEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "users"

    public let id: String
    public let name: String?
    public let email: String
    public let image: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        name: String?,
        email: String,
        image: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.name = name
        self.email = email
        self.image = image
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, email, image
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decodeIfPresent(String.self, forKey: .name)
        email = try c.decode(String.self, forKey: .email)
        image = try c.decodeIfPresent(String.self, forKey: .image)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - TeamMember

public struct TeamMemberEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "team_members"

    public let id: String
    public let teamId: String
    public let userId: String
    public let role: String
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        userId: String,
        role: String,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.userId = userId
        self.role = role
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, role
        case teamId = "team_id"
        case userId = "user_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - TeamInvite

public struct TeamInviteEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "team_invites"

    public let id: String
    public let teamId: String
    public let role: String
    // No longer synced (server columns allowlist — the invite token is a
    // bearer secret; owners get it once from the create mutation). Kept
    // nullable for pre-fix local rows.
    public let token: String?
    // Optional recipient address (EXP-188): set when the owner sent the
    // invite by email; rides the team-invites shape for the pending list.
    public let email: String?
    // The placeholder member this invite created (EXP-630): an emailed invite
    // puts the invitee on the roster at once as a credential-less `users` row,
    // and the member list badges that row "invited, not joined" while the
    // invite is unaccepted. NULL for link invites and invites to an existing
    // account; rides the team-invites shape.
    public let placeholderUserId: String?
    // EXP-1076: when the invite LINK was issued. NULL = a roster row nobody
    // was ever invited (the Linear import seats its placeholders without a
    // mail) — the member list reads "Not invited", never "Invite expired"
    // (such rows are minted with `expires_at = created_at`). Rides the
    // team-invites shape.
    public let sentAt: String?
    public let expiresAt: String
    public let acceptedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        role: String,
        token: String?,
        email: String? = nil,
        placeholderUserId: String? = nil,
        sentAt: String? = nil,
        expiresAt: String,
        acceptedAt: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.role = role
        self.token = token
        self.email = email
        self.placeholderUserId = placeholderUserId
        self.sentAt = sentAt
        self.expiresAt = expiresAt
        self.acceptedAt = acceptedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, role, token, email
        case teamId = "team_id"
        case placeholderUserId = "placeholder_user_id"
        case sentAt = "sent_at"
        case expiresAt = "expires_at"
        case acceptedAt = "accepted_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - Comment

public enum CommentKind: String, Codable, Sendable {
    case regular
    case question
    case plan

    public init(rawString: String?) {
        switch rawString {
        case "question": self = .question
        case "plan": self = .plan
        default: self = .regular
        }
    }
}

public struct CommentEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "comments"

    public let id: String
    public let issueId: String
    public let teamId: String
    /// NULLABLE (SLOP-4): a widget reporter's reply (`source` reporter) has
    /// no member author, and the FK is SET NULL on account deletion.
    public let authorId: String?
    // Plain GFM markdown (was jsonb `{ text }`) — stored and rendered verbatim.
    public let body: String?
    public let kind: String
    public let editedAt: String?
    public let createdAt: String
    public let updatedAt: String
    /// EXP-741: the top-level comment this one replies to (one level deep);
    /// nil = a top-level card.
    public let parentId: String?
    /// EXP-741: `user` | `mcp` — an agent posted it over MCP ("via MCP");
    /// SLOP-4: `reporter` — the widget reporter answered from their page.
    public let source: String?
    /// SLOP-4: `team` | `reporter` — who the words were for. A member's
    /// `reporter` comment was emailed to the reporter ("to reporter").
    public let audience: String
    /// #EXP-1383: the agent whose session wrote it (`authorId` keeps the
    /// member whose conversation it was); the card names the AGENT.
    public let authorAgentId: String?

    public var commentKind: CommentKind { CommentKind(rawString: kind) }
    public var isViaMcp: Bool { source == DomainContract.commentSourceMcp }
    /// The reporter wrote it (author NULL, named after the submission).
    public var isFromReporter: Bool { source == DomainContract.commentSourceReporter }
    /// A member wrote it FOR the reporter (the words left the team).
    public var isToReporter: Bool {
        !isFromReporter && audience == DomainContract.commentAudienceReporter
    }

    public init(
        id: String,
        issueId: String,
        teamId: String,
        authorId: String?,
        body: String?,
        kind: String,
        editedAt: String?,
        createdAt: String,
        updatedAt: String,
        parentId: String? = nil,
        source: String? = nil,
        audience: String = DomainContract.commentAudienceTeam,
        authorAgentId: String? = nil
    ) {
        self.authorAgentId = authorAgentId
        self.id = id
        self.issueId = issueId
        self.teamId = teamId
        self.authorId = authorId
        self.body = body
        self.kind = kind
        self.editedAt = editedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.parentId = parentId
        self.source = source
        self.audience = audience
    }

    enum CodingKeys: String, CodingKey {
        case id, body, kind, source, audience
        case parentId = "parent_id"
        case issueId = "issue_id"
        case teamId = "team_id"
        case authorId = "author_id"
        case authorAgentId = "author_agent_id"
        case editedAt = "edited_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: a pre-rotation snapshot may omit `kind`, so it decodes
// permissively with the schema default. `body` is plain GFM markdown text
// (was jsonb `{ text }`) and decodes as an ordinary optional string.
extension CommentEntity: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        issueId = try container.decode(String.self, forKey: .issueId)
        teamId = try container.decode(String.self, forKey: .teamId)
        authorId = try container.decodeIfPresent(String.self, forKey: .authorId)
        body = try container.decodeIfPresent(String.self, forKey: .body)
        kind = (try? container.decodeIfPresent(String.self, forKey: .kind)) ?? "regular"
        editedAt = try container.decodeIfPresent(String.self, forKey: .editedAt)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)
        // EXP-741: both absent from a pre-rotation snapshot — a row without
        // them is a top-level, person-written comment.
        parentId = try? container.decodeIfPresent(String.self, forKey: .parentId)
        source = try? container.decodeIfPresent(String.self, forKey: .source)
        // SLOP-4: absent from a pre-rotation snapshot — a team comment.
        audience = (try? container.decodeIfPresent(String.self, forKey: .audience))
            ?? DomainContract.commentAudienceTeam
        // #EXP-1383: absent on a pre-rotation snapshot.
        authorAgentId = try? container.decodeIfPresent(String.self, forKey: .authorAgentId)
    }
}

// MARK: - Attachment

public struct AttachmentEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "attachments"

    public let id: String
    public let teamId: String
    public let issueId: String
    public let commentId: String?
    // NULLABLE (REV-7), mirroring the server column: a widget screenshot
    // attachment has no human uploader, and the FK is ON DELETE SET NULL, so a
    // deleted account nulls the uploader on attachments it left behind in a
    // surviving team.
    public let uploaderId: String?
    public let filename: String
    public let contentType: String
    public let sizeBytes: Int
    public let storageKey: String
    public let url: String
    public let width: Int?
    public let height: Int?
    /// EXP-824: a `video/*` / `audio/*` row's probed length, for the duration
    /// chip on the player. NULL on every other row (and on media rows the
    /// server could not probe).
    public let durationMs: Int?
    /// EXP-824: set when a poster frame is stored beside the bytes — served
    /// as `/api/attachments/{id}?poster=1`. The key itself is never used by a
    /// client; non-nil IS the "has a poster" signal.
    public let posterStorageKey: String?
    /// EXP-1247: uploaded through a FILE button (the Files paperclip, the
    /// editor's Attach file), so it lists under Files and never inlines, even
    /// when it is an image or a clip.
    public let asFile: Bool
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        issueId: String,
        commentId: String?,
        uploaderId: String?,
        filename: String,
        contentType: String,
        sizeBytes: Int,
        storageKey: String,
        url: String,
        width: Int?,
        height: Int?,
        durationMs: Int? = nil,
        posterStorageKey: String? = nil,
        asFile: Bool = false,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.issueId = issueId
        self.commentId = commentId
        self.asFile = asFile
        self.uploaderId = uploaderId
        self.filename = filename
        self.contentType = contentType
        self.sizeBytes = sizeBytes
        self.storageKey = storageKey
        self.url = url
        self.width = width
        self.height = height
        self.durationMs = durationMs
        self.posterStorageKey = posterStorageKey
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    /// Whether a poster frame exists for this row (EXP-824).
    public var hasPoster: Bool { posterStorageKey != nil }

    enum CodingKeys: String, CodingKey {
        case id, filename, url, width, height
        case teamId = "team_id"
        case issueId = "issue_id"
        case commentId = "comment_id"
        case uploaderId = "uploader_id"
        case contentType = "content_type"
        case sizeBytes = "size_bytes"
        case storageKey = "storage_key"
        case durationMs = "duration_ms"
        case posterStorageKey = "poster_storage_key"
        case asFile = "as_file"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: size_bytes / width / height arrive off the Electric wire as
// JSON strings (Postgres text) but as native numbers from tRPC/fixtures — decode
// them through the type-aware wire helpers. A same-file extension keeps
// encode(to:) synthesis. size_bytes is NOT NULL; a hypothetical absent value
// falls back to 0 (the SQLite column default) rather than killing the row.
// uploader_id is decodeIfPresent (REV-7): the server column is nullable, and a
// strict decode would drop widget-screenshot inserts on the floor.
extension AttachmentEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        issueId = try c.decode(String.self, forKey: .issueId)
        commentId = try c.decodeIfPresent(String.self, forKey: .commentId)
        uploaderId = try c.decodeIfPresent(String.self, forKey: .uploaderId)
        filename = try c.decode(String.self, forKey: .filename)
        contentType = try c.decode(String.self, forKey: .contentType)
        sizeBytes = try c.decodeWireInt(forKey: .sizeBytes) ?? 0
        storageKey = try c.decode(String.self, forKey: .storageKey)
        url = try c.decode(String.self, forKey: .url)
        width = try c.decodeWireInt(forKey: .width)
        height = try c.decodeWireInt(forKey: .height)
        // EXP-824: both nullable and absent on pre-rotation snapshots.
        durationMs = try c.decodeWireInt(forKey: .durationMs)
        posterStorageKey = try c.decodeIfPresent(String.self, forKey: .posterStorageKey)
        // EXP-1247: absent on pre-rotation snapshots.
        asFile = c.decodeWireBool(forKey: .asFile, default: false)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Notification

public struct NotificationEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "notifications"

    public let id: String
    public let userId: String
    public let issueId: String?
    // Set on issue-less agent_message / session_blocked rows (the message's
    // resp. the run's team); NULL on issue-anchored rows (their team resolves
    // through the issue). SLOP-4: a reporter_reply is issue-anchored.
    public let teamId: String?
    /// EXP-980: the run a `session_blocked` row is about — the inbox row's tap
    /// target. NULL once the run has been pruned (the row still renders).
    public let sessionId: String?
    // notification_type: issue_assigned|issue_comment|issue_status_changed|
    //                    issue_mention|issue_created|pr_opened|pr_merged|
    //                    reporter_reply|agent_message|session_blocked
    public let type: String
    public let title: String
    public let body: String?
    public let readAt: String?
    public let pushedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        userId: String,
        issueId: String?,
        teamId: String? = nil,
        sessionId: String? = nil,
        type: String,
        title: String,
        body: String?,
        readAt: String?,
        pushedAt: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.userId = userId
        self.issueId = issueId
        self.teamId = teamId
        self.sessionId = sessionId
        self.type = type
        self.title = title
        self.body = body
        self.readAt = readAt
        self.pushedAt = pushedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, type, title, body
        case userId = "user_id"
        case issueId = "issue_id"
        case teamId = "team_id"
        case sessionId = "session_id"
        case readAt = "read_at"
        case pushedAt = "pushed_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - IssueSubscriber

public struct IssueSubscriberEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "issue_subscribers"

    public let id: String
    public let issueId: String
    // Nullable: widget_reporter rows carry `email` instead of a member `userId`.
    public let userId: String?
    // Set for widget_reporter rows; null for member rows.
    public let email: String?
    public let teamId: String
    // source: creator|assignee|commenter|manual|mention|widget_reporter
    public let source: String
    public let unsubscribed: Bool
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        issueId: String,
        userId: String?,
        email: String?,
        teamId: String,
        source: String,
        unsubscribed: Bool,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.issueId = issueId
        self.userId = userId
        self.email = email
        self.teamId = teamId
        self.source = source
        self.unsubscribed = unsubscribed
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, source, unsubscribed, email
        case issueId = "issue_id"
        case userId = "user_id"
        case teamId = "team_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: `unsubscribed` comes off the Electric wire as a JSON string
// ("t"/"true"/"1" or "f"/"false"/"0"), a native bool from tRPC/fixtures, or the
// integer 0/1. Decode permissively through the type-aware wire helper. Without
// the string form, a wire "t"/"true" silently defaulted to false and iOS never
// saw an unsubscribed=true row from a full-row message.
extension IssueSubscriberEntity {
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        issueId = try container.decode(String.self, forKey: .issueId)
        userId = try container.decodeIfPresent(String.self, forKey: .userId)
        email = try container.decodeIfPresent(String.self, forKey: .email)
        teamId = try container.decode(String.self, forKey: .teamId)
        source = try container.decode(String.self, forKey: .source)
        unsubscribed = container.decodeWireBool(forKey: .unsubscribed, default: false)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - IssueEvent

public struct IssueEventEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "issue_events"

    public let id: String
    public let issueId: String
    public let teamId: String
    public let actorUserId: String?
    // type: status_changed|assignee_changed|label_added|label_removed|
    //       pr_opened|pr_merged
    public let type: String
    // JSON payload — Electric delivers as object; stored as the stringified
    // JSON, decoded lazily by the UI. Null when the event has no payload.
    public let payload: String?
    public let createdAt: String
    public let updatedAt: String
    /// #EXP-1383: the agent whose session acted (`actorUserId` keeps its
    /// member); the row names the AGENT and the fold keys on it.
    public let actorAgentId: String?

    public init(
        id: String,
        issueId: String,
        teamId: String,
        actorUserId: String?,
        type: String,
        payload: String?,
        createdAt: String,
        updatedAt: String,
        actorAgentId: String? = nil
    ) {
        self.actorAgentId = actorAgentId
        self.id = id
        self.issueId = issueId
        self.teamId = teamId
        self.actorUserId = actorUserId
        self.type = type
        self.payload = payload
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, type, payload
        case issueId = "issue_id"
        case teamId = "team_id"
        case actorUserId = "actor_user_id"
        case actorAgentId = "actor_agent_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension IssueEventEntity: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        issueId = try container.decode(String.self, forKey: .issueId)
        teamId = try container.decode(String.self, forKey: .teamId)
        actorUserId = try container.decodeIfPresent(String.self, forKey: .actorUserId)
        // #EXP-1383: absent on a pre-rotation snapshot.
        actorAgentId = try container.decodeIfPresent(String.self, forKey: .actorAgentId)
        type = try container.decode(String.self, forKey: .type)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)

        // Handle JSONB payload: object, string, or null
        if container.contains(.payload) {
            if let stringValue = try? container.decode(String.self, forKey: .payload) {
                payload = stringValue
            } else if let _ = try? container.decodeNil(forKey: .payload) {
                payload = nil
            } else {
                let rawJSON = try container.decode(AnyCodableValue.self, forKey: .payload)
                payload = rawJSON.jsonString
            }
        } else {
            payload = nil
        }
    }
}

// MARK: - Device (EXP-481)

// The per-user machine registry — the 17th Electric shape (own rows plus
// teammates' team-shared SERVER rows; scope is server-side). Rows are
// SERVER-AUTHORITATIVE device state: `launch_defaults` is the canonical copy
// of the machine's agent defaults (its local settings.json converges), and
// online-ness derives CLIENT-side from `last_seen_at` freshness
// (`DeviceLiveness`) — no relay presence in the sync path. Every non-PK field
// is optional/defaulted: a required field absent on the wire silently drops
// rows forever (the attachments `uploader_id` lesson).
public struct DeviceEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "devices"

    public let id: String
    public let userId: String
    /// The steer deviceId (the start-target identifier every devices.* tRPC
    /// mutation takes) — NOT the row id.
    public let deviceId: String
    public let label: String
    /// `desktop` | `server` — documented varchar, no contract enum.
    public let kind: String?
    /// EXP-924: the owner-picked display icon — a registry name from the
    /// DEVICE set (contract `deviceIcon`). NULL = the kind default; resolve it
    /// through `DeviceIconDisplay`, never raw.
    public let icon: String?
    public let platform: String?
    public let version: String?
    /// jsonb string[] columns, stored as stringified JSON (decoded lazily by
    /// the SteerDevice mapping in DeviceRows.swift).
    public let agents: String?
    public let caps: String?
    public let unauthedAgents: String?
    /// EXP-749: jsonb string[] — the subset of `agents` the machine's ACP
    /// engine can drive. Still NULLABLE server-side (a row last written by a
    /// pre-EXP-749 build), and NULL now maps to EMPTY: that machine starts
    /// nothing (`SteerDevice.acpAgents` in DeviceRows.swift).
    public let acpAgents: String?
    /// jsonb object (defaultAgent + per-agent model/effort/toggles), stored as
    /// stringified JSON. Inner keys are camelCase verbatim on the wire.
    public let launchDefaults: String?
    public let launchDefaultsUpdatedAt: String?
    /// EXP-484: per-agent auth status and rate-limit usage the machine reports
    /// on register/heartbeat — jsonb objects keyed by contract `codingAgent`,
    /// stored as stringified JSON (decoded by `AgentUsagePresentation`).
    /// READ-ONLY here: the device is the only writer.
    public let agentAccounts: String?
    public let agentUsage: String?
    /// When the server last stored `agentUsage`. Moves every few minutes, so
    /// it must never be a re-sync nudge trigger.
    public let agentUsageAt: String?
    /// EXP-1196: the machine's readiness report (`{checkedAt, items[]}`,
    /// contract fixture `device-doctor.json`), stored as stringified JSON and
    /// decoded by `DeviceDoctor`. NULL = a build that reports none, which
    /// renders no readiness block. READ-ONLY here: the device is the writer.
    public let doctor: String?
    public let activeSessions: Int
    public let lastSeenAt: String?
    /// FEED-33: the teams this machine is shared with (`uuid[]`, sorted +
    /// deduped server-side; empty = private). Stored as JSON text like
    /// `agents`/`caps`; decoded from the wire's `{a,b}` literal too.
    public let sharedTeamIds: [String]
    /// EXP-622: the ROW OWNER's default machine — the one every device picker
    /// prefills. Honoured only when `userId` is the signed-in user: a
    /// teammate's shared server carries THEIR preference, not ours.
    public let isDefault: Bool
    public let updateRequestedAt: String?
    public let createdAt: String?
    public let updatedAt: String?

    public init(
        id: String,
        userId: String,
        deviceId: String,
        label: String,
        kind: String? = nil,
        icon: String? = nil,
        platform: String? = nil,
        version: String? = nil,
        agents: String? = nil,
        caps: String? = nil,
        unauthedAgents: String? = nil,
        acpAgents: String? = nil,
        launchDefaults: String? = nil,
        launchDefaultsUpdatedAt: String? = nil,
        agentAccounts: String? = nil,
        agentUsage: String? = nil,
        agentUsageAt: String? = nil,
        doctor: String? = nil,
        activeSessions: Int = 0,
        lastSeenAt: String? = nil,
        sharedTeamIds: [String] = [],
        isDefault: Bool = false,
        updateRequestedAt: String? = nil,
        createdAt: String? = nil,
        updatedAt: String? = nil
    ) {
        self.id = id
        self.userId = userId
        self.deviceId = deviceId
        self.label = label
        self.kind = kind
        self.icon = icon
        self.platform = platform
        self.version = version
        self.agents = agents
        self.caps = caps
        self.unauthedAgents = unauthedAgents
        self.acpAgents = acpAgents
        self.launchDefaults = launchDefaults
        self.launchDefaultsUpdatedAt = launchDefaultsUpdatedAt
        self.agentAccounts = agentAccounts
        self.agentUsage = agentUsage
        self.agentUsageAt = agentUsageAt
        self.doctor = doctor
        self.activeSessions = activeSessions
        self.lastSeenAt = lastSeenAt
        self.sharedTeamIds = sharedTeamIds
        self.isDefault = isDefault
        self.updateRequestedAt = updateRequestedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, label, kind, icon, platform, version, agents, caps, doctor
        case userId = "user_id"
        case deviceId = "device_id"
        case unauthedAgents = "unauthed_agents"
        case acpAgents = "acp_agents"
        case launchDefaults = "launch_defaults"
        case launchDefaultsUpdatedAt = "launch_defaults_updated_at"
        case agentAccounts = "agent_accounts"
        case agentUsage = "agent_usage"
        case agentUsageAt = "agent_usage_at"
        case activeSessions = "active_sessions"
        case lastSeenAt = "last_seen_at"
        case sharedTeamIds = "shared_team_ids"
        case isDefault = "is_default"
        case updateRequestedAt = "update_requested_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom decode: the jsonb columns arrive as objects/arrays off the Electric
// wire (stored as stringified JSON via the type-FAITHFUL JSONWireValue — the
// stored string is parsed back into TYPED data, launchDefaults booleans
// included) or as pre-stringified JSON from fixtures; `active_sessions` is
// Postgres int-as-text on the wire, and everything else decodes permissively
// with schema defaults.
extension DeviceEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        userId = try c.decode(String.self, forKey: .userId)
        deviceId = try c.decode(String.self, forKey: .deviceId)
        label = (try? c.decode(String.self, forKey: .label)) ?? ""
        kind = try c.decodeIfPresent(String.self, forKey: .kind)
        // EXP-924: absent on a pre-EXP-924 snapshot and NULL until the owner
        // picks one — both read as "the kind default" downstream.
        icon = try c.decodeIfPresent(String.self, forKey: .icon)
        platform = try c.decodeIfPresent(String.self, forKey: .platform)
        version = try c.decodeIfPresent(String.self, forKey: .version)
        agents = c.decodeWireJsonString(forKey: .agents)
        caps = c.decodeWireJsonString(forKey: .caps)
        unauthedAgents = c.decodeWireJsonString(forKey: .unauthedAgents)
        // EXP-749: jsonb like agents/caps. NULL on a row last written by a
        // pre-EXP-749 build; the SteerDevice mapping reads that as EMPTY.
        acpAgents = c.decodeWireJsonString(forKey: .acpAgents)
        launchDefaults = c.decodeWireJsonString(forKey: .launchDefaults)
        launchDefaultsUpdatedAt = try c.decodeIfPresent(String.self, forKey: .launchDefaultsUpdatedAt)
        // EXP-484: jsonb like agents/caps — object off the wire, pre-stringified
        // from fixtures. Absent on a pre-EXP-484 snapshot, so never required.
        agentAccounts = c.decodeWireJsonString(forKey: .agentAccounts)
        agentUsage = c.decodeWireJsonString(forKey: .agentUsage)
        agentUsageAt = try c.decodeIfPresent(String.self, forKey: .agentUsageAt)
        // EXP-1196: jsonb like agent_accounts; nullable.
        doctor = c.decodeWireJsonString(forKey: .doctor)
        // SE-0230 flattens the try?-of-optional; unparseable text degrades to
        // 0 rather than dropping the row (activeSessions only gates a badge).
        activeSessions = (try? c.decodeWireInt(forKey: .activeSessions)) ?? 0
        lastSeenAt = try c.decodeIfPresent(String.self, forKey: .lastSeenAt)
        // FEED-33: `{a,b}` off the wire, `["a","b"]` back out of GRDB (a
        // partial update stores the raw literal, a full row the JSON form) —
        // both parse; absent on a pre-FEED-33 server = private.
        sharedTeamIds = c.decodeWireStringList(forKey: .sharedTeamIds)
        isDefault = c.decodeWireBool(forKey: .isDefault, default: false)
        updateRequestedAt = try c.decodeIfPresent(String.self, forKey: .updateRequestedAt)
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
        updatedAt = try c.decodeIfPresent(String.self, forKey: .updatedAt)
    }
}

// MARK: - DeviceWorktree (EXP-481)

// Per-device worktree inventory — the 18th Electric shape, reported by the
// device itself (powers the composer's resume offers, from persisted data
// even while the machine is offline; EXP-1042 took the phone's worktree list
// away, the IDE owns that surface). `device_row_id`
// references the devices ROW id (uuid), never the steer device-id string.
// The server-side scoping mirrors (user_id/shared_team_ids) stay out of the
// allowlist and never reach this decoder.
public struct DeviceWorktreeEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "device_worktrees"

    public let id: String
    public let deviceRowId: String
    public let repoFullName: String
    public let branch: String
    /// `exp/<IDENTIFIER>` linkage as the device parsed it; nil for batch or
    /// foreign-prefix branches.
    public let issueIdentifier: String?
    /// jsonb string[] of the agents recorded in the worktree's .exp-agents
    /// resume marker, stored as stringified JSON; nil = pre-marker worktree
    /// (any agent may resume).
    public let agents: String?
    /// `clean` | `untracked` | `tracked` | `unknown` — documented varchar.
    public let dirty: String?
    /// A live session on the device currently holds this worktree's branch.
    public let busy: Bool
    public let reportedAt: String?
    public let createdAt: String?
    public let updatedAt: String?

    public init(
        id: String,
        deviceRowId: String,
        repoFullName: String,
        branch: String,
        issueIdentifier: String? = nil,
        agents: String? = nil,
        dirty: String? = nil,
        busy: Bool = false,
        reportedAt: String? = nil,
        createdAt: String? = nil,
        updatedAt: String? = nil
    ) {
        self.id = id
        self.deviceRowId = deviceRowId
        self.repoFullName = repoFullName
        self.branch = branch
        self.issueIdentifier = issueIdentifier
        self.agents = agents
        self.dirty = dirty
        self.busy = busy
        self.reportedAt = reportedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, branch, agents, dirty, busy
        case deviceRowId = "device_row_id"
        case repoFullName = "repo_full_name"
        case issueIdentifier = "issue_identifier"
        case reportedAt = "reported_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension DeviceWorktreeEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        deviceRowId = try c.decode(String.self, forKey: .deviceRowId)
        repoFullName = try c.decode(String.self, forKey: .repoFullName)
        branch = try c.decode(String.self, forKey: .branch)
        issueIdentifier = try c.decodeIfPresent(String.self, forKey: .issueIdentifier)
        agents = c.decodeWireJsonString(forKey: .agents)
        dirty = try c.decodeIfPresent(String.self, forKey: .dirty)
        busy = c.decodeWireBool(forKey: .busy, default: false)
        reportedAt = try c.decodeIfPresent(String.self, forKey: .reportedAt)
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
        updatedAt = try c.decodeIfPresent(String.self, forKey: .updatedAt)
    }
}

// `issues.description` / `comments.body` are plain GFM markdown text — return
// them verbatim (mirrors the web helper in packages/db-schema/src/domain.ts;
// never parse: a body that happens to be a bare JSON object is legit content).
public func getIssueDescriptionText(_ description: String?) -> String {
    description ?? ""
}

public func getCommentBodyText(_ body: String?) -> String {
    body ?? ""
}

// MARK: - JSONWireValue (type-faithful JSONB re-encoding)

/// A faithful JSON tree for jsonb columns whose stored string is later parsed
/// back into TYPED data (e.g. `blocked`, a widget's `customData`). Unlike
/// AnyCodableValue below — which stringifies every nested scalar, fine for the
/// display-only issue_events payload — re-encoding this preserves
/// bools/numbers/null exactly, so a wire `"required": true` survives the
/// store-as-string round trip as a real boolean.
public indirect enum JSONWireValue: Codable, Sendable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case null
    case array([JSONWireValue])
    case object([String: JSONWireValue])

    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() {
            self = .null
        } else if let bool = try? c.decode(Bool.self) {
            self = .bool(bool)
        } else if let string = try? c.decode(String.self) {
            self = .string(string)
        } else if let number = try? c.decode(Double.self) {
            self = .number(number)
        } else if let array = try? c.decode([JSONWireValue].self) {
            self = .array(array)
        } else if let object = try? c.decode([String: JSONWireValue].self) {
            self = .object(object)
        } else {
            throw DecodingError.dataCorruptedError(
                in: c, debugDescription: "Unsupported JSON value"
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case let .string(value): try c.encode(value)
        case let .number(value): try c.encode(value)
        case let .bool(value): try c.encode(value)
        case .null: try c.encodeNil()
        case let .array(value): try c.encode(value)
        case let .object(value): try c.encode(value)
        }
    }
}

// MARK: - AnyCodableValue (for issue_events.payload JSONB handling)

public struct AnyCodableValue: Codable, Sendable {
    public let jsonString: String

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let dict = try? container.decode([String: AnyCodableValue].self) {
            let data = try JSONEncoder().encode(dict)
            jsonString = String(data: data, encoding: .utf8) ?? "{}"
        } else if let arr = try? container.decode([AnyCodableValue].self) {
            let data = try JSONEncoder().encode(arr)
            jsonString = String(data: data, encoding: .utf8) ?? "[]"
        } else if let str = try? container.decode(String.self) {
            jsonString = str
        } else if let num = try? container.decode(Double.self) {
            jsonString = String(num)
        } else if let bool = try? container.decode(Bool.self) {
            jsonString = String(bool)
        } else {
            jsonString = "null"
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(jsonString)
    }
}
