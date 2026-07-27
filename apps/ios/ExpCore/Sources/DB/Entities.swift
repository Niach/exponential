import Foundation
import GRDB

// MARK: - Electric Offset

public struct ElectricOffset: Codable, FetchableRecord, PersistableRecord, Sendable {
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

public struct TeamEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "teams"

    public let id: String
    public let name: String
    public let slug: String
    public let iconUrl: String?
    // Team-level helpdesk switch (EXP-180): when true, every member sees the
    // Support inbox (standalone tickets via the helpdesk tRPC router).
    public let helpdeskEnabled: Bool
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        name: String,
        slug: String,
        iconUrl: String?,
        helpdeskEnabled: Bool = false,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.name = name
        self.slug = slug
        self.iconUrl = iconUrl
        self.helpdeskEnabled = helpdeskEnabled
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
        case helpdeskEnabled = "helpdesk_enabled"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom decode: `helpdesk_enabled` arrives as Postgres text off the Electric
// wire ("t"/"true"/…) but as a native scalar from tRPC/fixtures, and a
// pre-rotation snapshot may omit it — decode permissively with the schema
// default (the BoardEntity `is_protected` precedent).
extension TeamEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        slug = try c.decode(String.self, forKey: .slug)
        iconUrl = try c.decodeIfPresent(String.self, forKey: .iconUrl)
        helpdeskEnabled = c.decodeWireBool(forKey: .helpdeskEnabled, default: false)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Board

public struct BoardEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
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
    // Server-managed protection flag: a protected board (the bootstrap
    // dogfood board) can't be deleted/retyped/repointed. Rides along on
    // the boards shape; clients hide the destructive affordances for it.
    public let isProtected: Bool
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
        isProtected: Bool = false,
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
        self.isProtected = isProtected
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, slug, prefix, color, icon
        case teamId = "team_id"
        case sortOrder = "sort_order"
        case repositoryId = "repository_id"
        case isProtected = "is_protected"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: the icon / is_protected columns land in a shape rotation; a
// pre-rotation snapshot (or a partial update touching other columns) may omit
// them, so decode each permissively with the schema default instead of
// throwing. Booleans and sort_order come off the Electric wire as
// JSON strings (Postgres text — "true"/"false"/"t"/"f"/"1"/"0" for bools,
// "2"/"3.5" for sort_order) but as native scalars from tRPC/fixtures, so they go
// through the type-aware wire decoders.
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
        isProtected = c.decodeWireBool(forKey: .isProtected, default: false)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Issue

public struct IssueEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
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
    /// pre-backfill snapshot or an older server — resolution falls back to the
    /// team row matching `status`, then to a constructed builtin default.
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
    // PR linkage (one issue = one PR = one branch). Written server-side by the
    // MCP open_pr tool + the merge webhook/cron; synced to every client so the
    // PR badge works without parsing comments.
    public let prUrl: String?
    public let prNumber: Int?
    public let prState: String?
    public let branch: String?
    public let prMergedAt: String?
    public let createdAt: String
    public let updatedAt: String

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
        prUrl: String?,
        prNumber: Int?,
        prState: String?,
        branch: String?,
        prMergedAt: String?,
        createdAt: String,
        updatedAt: String
    ) {
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
        self.prUrl = prUrl
        self.prNumber = prNumber
        self.prState = prState
        self.branch = branch
        self.prMergedAt = prMergedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, title, description, status, priority, number, identifier, branch, source
        case statusId = "status_id"
        case boardId = "board_id"
        case assigneeId = "assignee_id"
        case creatorId = "creator_id"
        case dueDate = "due_date"
        case sortOrder = "sort_order"
        case completedAt = "completed_at"
        case duplicateOfId = "duplicate_of_id"
        case prUrl = "pr_url"
        case prNumber = "pr_number"
        case prState = "pr_state"
        case prMergedAt = "pr_merged_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: number / sort_order / pr_number come off the Electric wire as
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
        prUrl = try container.decodeIfPresent(String.self, forKey: .prUrl)
        prNumber = try container.decodeWireInt(forKey: .prNumber)
        prState = try container.decodeIfPresent(String.self, forKey: .prState)
        branch = try container.decodeIfPresent(String.self, forKey: .branch)
        prMergedAt = try container.decodeIfPresent(String.self, forKey: .prMergedAt)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - CodingSession

// The live "coding now" record — one row per interactive desktop coding session
// (one terminal + one CLI child in one worktree). Synced as the 14th Electric
// shape so every coordination client can show a "coding now" badge. No plan or
// approval state; the PR outcome lives on `issues`. `userId` is the REAL user
// driving the session (not a synthetic bot). Mirrors packages/db-schema
// codingSessions.
public struct CodingSessionEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "coding_sessions"

    public let id: String
    // Nullable: NULL for a desktop batch (multi-issue) run's issueless session.
    public let issueId: String?
    // Set for issue-scoped sessions (trigger-denormalized server-side); NULL
    // for a batch run's session (it spans boards).
    public let boardId: String?
    public let teamId: String
    public let userId: String
    public let deviceLabel: String?
    public let status: String
    // Desktop-written attention flag (EXP-214): the agent is parked on a
    // plan-approval / AskUserQuestion picker and waits for a human.
    public let needsInput: Bool
    // Action run linkage (EXP-253): set on a session started from a team
    // action. `actionId` nulls if the action is later deleted (server FK SET
    // NULL) while `actionName` — a display snapshot — keeps labeling the run.
    // Both NULL on ordinary issue/batch sessions.
    public let actionId: String?
    public let actionName: String?
    public let startedAt: String
    public let endedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        issueId: String?,
        boardId: String? = nil,
        teamId: String,
        userId: String,
        deviceLabel: String?,
        status: String,
        needsInput: Bool = false,
        actionId: String? = nil,
        actionName: String? = nil,
        startedAt: String,
        endedAt: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.issueId = issueId
        self.boardId = boardId
        self.teamId = teamId
        self.userId = userId
        self.deviceLabel = deviceLabel
        self.status = status
        self.needsInput = needsInput
        self.actionId = actionId
        self.actionName = actionName
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, status
        case issueId = "issue_id"
        case boardId = "board_id"
        case teamId = "team_id"
        case userId = "user_id"
        case deviceLabel = "device_label"
        case needsInput = "needs_input"
        case actionId = "action_id"
        case actionName = "action_name"
        case startedAt = "started_at"
        case endedAt = "ended_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom decode: `needs_input` arrives as Postgres text off the Electric wire
// ("t"/"f") but as a native scalar from fixtures, and a pre-rotation snapshot
// may omit it — decode permissively with the schema default (the TeamEntity
// `helpdesk_enabled` precedent).
extension CodingSessionEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        issueId = try c.decodeIfPresent(String.self, forKey: .issueId)
        boardId = try c.decodeIfPresent(String.self, forKey: .boardId)
        teamId = try c.decode(String.self, forKey: .teamId)
        userId = try c.decode(String.self, forKey: .userId)
        deviceLabel = try c.decodeIfPresent(String.self, forKey: .deviceLabel)
        status = try c.decode(String.self, forKey: .status)
        needsInput = c.decodeWireBool(forKey: .needsInput, default: false)
        actionId = try c.decodeIfPresent(String.self, forKey: .actionId)
        actionName = try c.decodeIfPresent(String.self, forKey: .actionName)
        startedAt = try c.decode(String.self, forKey: .startedAt)
        endedAt = try c.decodeIfPresent(String.self, forKey: .endedAt)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Action

// Team action prompts (EXP-268 — the 15th Electric shape). The server-side
// columns allowlist deliberately EXCLUDES `body`: the ≤64KB markdown prompt
// never rides sync, tRPC `actions.get` stays the only body path. Mirrors
// packages/db-schema actions minus body.
public struct ActionEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "actions"

    public let id: String
    public let teamId: String
    // Nil for repo-less actions (the desktop runs those in a scratch dir).
    public let repositoryId: String?
    public let name: String
    public let description: String?
    /// EXP-273: curated registry icon name (the same set as `boards.icon`);
    /// nil = the generic action glyph.
    public let icon: String?
    /// Typed inputs schema (jsonb array of {key,label,type,required,
    /// placeholder}) — Electric delivers it as a JSON value; stored as the
    /// stringified JSON, decoded lazily by the UI. Null when the action
    /// declares no inputs.
    public let inputs: String?
    public let sortOrder: Double?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        repositoryId: String?,
        name: String,
        description: String?,
        icon: String?,
        inputs: String?,
        sortOrder: Double?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.repositoryId = repositoryId
        self.name = name
        self.description = description
        self.icon = icon
        self.inputs = inputs
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, description, icon, inputs
        case teamId = "team_id"
        case repositoryId = "repository_id"
        case sortOrder = "sort_order"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom decode: sort_order goes through the type-aware wire helper
// (Postgres text off the Electric wire, native scalar from fixtures), and
// `inputs` follows the IssueEventEntity permissive jsonb pattern — string,
// object/array, or null — re-encoded to a stored string. Unlike the payload
// path it re-encodes through the type-FAITHFUL JSONWireValue: the stored
// string is later parsed as typed data (ActionInputDto's `required` Bool),
// which AnyCodableValue's stringified nested scalars would break.
extension ActionEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        repositoryId = try c.decodeIfPresent(String.self, forKey: .repositoryId)
        name = try c.decode(String.self, forKey: .name)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        icon = try c.decodeIfPresent(String.self, forKey: .icon)
        sortOrder = try c.decodeWireDouble(forKey: .sortOrder)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)

        // Handle JSONB inputs: string, null, or object/array.
        if c.contains(.inputs) {
            if let stringValue = try? c.decode(String.self, forKey: .inputs) {
                inputs = stringValue
            } else if (try? c.decodeNil(forKey: .inputs)) == true {
                inputs = nil
            } else {
                let rawJSON = try c.decode(JSONWireValue.self, forKey: .inputs)
                let data = try JSONEncoder().encode(rawJSON)
                inputs = String(data: data, encoding: .utf8)
            }
        } else {
            inputs = nil
        }
    }
}

// MARK: - Label

public struct LabelEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
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
public struct IssueStatusEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
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

public struct IssueLabelEntity: Codable, FetchableRecord, PersistableRecord, Sendable {
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

// MARK: - User

public struct UserEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
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

public struct TeamMemberEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
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

public struct TeamInviteEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
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
        self.expiresAt = expiresAt
        self.acceptedAt = acceptedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, role, token, email
        case teamId = "team_id"
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

public struct CommentEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "comments"

    public let id: String
    public let issueId: String
    public let teamId: String
    public let authorId: String
    // Plain GFM markdown (was jsonb `{ text }`) — stored and rendered verbatim.
    public let body: String?
    public let kind: String
    public let editedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public var commentKind: CommentKind { CommentKind(rawString: kind) }

    public init(
        id: String,
        issueId: String,
        teamId: String,
        authorId: String,
        body: String?,
        kind: String,
        editedAt: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.issueId = issueId
        self.teamId = teamId
        self.authorId = authorId
        self.body = body
        self.kind = kind
        self.editedAt = editedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, body, kind
        case issueId = "issue_id"
        case teamId = "team_id"
        case authorId = "author_id"
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
        authorId = try container.decode(String.self, forKey: .authorId)
        body = try container.decodeIfPresent(String.self, forKey: .body)
        kind = (try? container.decodeIfPresent(String.self, forKey: .kind)) ?? "regular"
        editedAt = try container.decodeIfPresent(String.self, forKey: .editedAt)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Attachment

public struct AttachmentEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "attachments"

    public let id: String
    public let teamId: String
    public let issueId: String
    public let commentId: String?
    public let uploaderId: String
    public let filename: String
    public let contentType: String
    public let sizeBytes: Int
    public let storageKey: String
    public let url: String
    public let width: Int?
    public let height: Int?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        issueId: String,
        commentId: String?,
        uploaderId: String,
        filename: String,
        contentType: String,
        sizeBytes: Int,
        storageKey: String,
        url: String,
        width: Int?,
        height: Int?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.issueId = issueId
        self.commentId = commentId
        self.uploaderId = uploaderId
        self.filename = filename
        self.contentType = contentType
        self.sizeBytes = sizeBytes
        self.storageKey = storageKey
        self.url = url
        self.width = width
        self.height = height
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, filename, url, width, height
        case teamId = "team_id"
        case issueId = "issue_id"
        case commentId = "comment_id"
        case uploaderId = "uploader_id"
        case contentType = "content_type"
        case sizeBytes = "size_bytes"
        case storageKey = "storage_key"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: size_bytes / width / height arrive off the Electric wire as
// JSON strings (Postgres text) but as native numbers from tRPC/fixtures — decode
// them through the type-aware wire helpers. A same-file extension keeps
// encode(to:) synthesis. size_bytes is NOT NULL; a hypothetical absent value
// falls back to 0 (the SQLite column default) rather than killing the row.
extension AttachmentEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        issueId = try c.decode(String.self, forKey: .issueId)
        commentId = try c.decodeIfPresent(String.self, forKey: .commentId)
        uploaderId = try c.decode(String.self, forKey: .uploaderId)
        filename = try c.decode(String.self, forKey: .filename)
        contentType = try c.decode(String.self, forKey: .contentType)
        sizeBytes = try c.decodeWireInt(forKey: .sizeBytes) ?? 0
        storageKey = try c.decode(String.self, forKey: .storageKey)
        url = try c.decode(String.self, forKey: .url)
        width = try c.decodeWireInt(forKey: .width)
        height = try c.decodeWireInt(forKey: .height)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Notification

public struct NotificationEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "notifications"

    public let id: String
    public let userId: String
    public let issueId: String?
    // Set on issue-less support_reply rows (the ticket's team); NULL on
    // issue-anchored rows (their team resolves through the issue).
    public let teamId: String?
    // notification_type: issue_assigned|issue_comment|issue_status_changed|
    //                    issue_mention|issue_created|pr_opened|pr_merged|
    //                    support_reply
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
        case readAt = "read_at"
        case pushedAt = "pushed_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - IssueSubscriber

public struct IssueSubscriberEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
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

public struct IssueEventEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
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

    public init(
        id: String,
        issueId: String,
        teamId: String,
        actorUserId: String?,
        type: String,
        payload: String?,
        createdAt: String,
        updatedAt: String
    ) {
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
/// back into TYPED data (actions.inputs → ActionInputDto). Unlike
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
