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
    public let archivedAt: String?
    public let githubRepo: String?
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
    // dogfood board) can't be deleted/archived/retyped/repointed. Rides along on
    // the boards shape; clients hide the destructive affordances for it.
    public let isProtected: Bool
    // Display-only mirror of the preview run targets + feedback routing target
    // (jsonb in Postgres). Stored as the raw JSON text; never executed.
    public let previewConfig: String?
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
        archivedAt: String?,
        githubRepo: String?,
        repositoryId: String?,
        icon: String? = nil,
        isProtected: Bool = false,
        previewConfig: String?,
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
        self.archivedAt = archivedAt
        self.githubRepo = githubRepo
        self.repositoryId = repositoryId
        self.icon = icon
        self.isProtected = isProtected
        self.previewConfig = previewConfig
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, slug, prefix, color, icon
        case teamId = "team_id"
        case sortOrder = "sort_order"
        case archivedAt = "archived_at"
        case githubRepo = "github_repo"
        case repositoryId = "repository_id"
        case isProtected = "is_protected"
        case previewConfig = "preview_config"
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
        archivedAt = try c.decodeIfPresent(String.self, forKey: .archivedAt)
        githubRepo = try c.decodeIfPresent(String.self, forKey: .githubRepo)
        repositoryId = try c.decodeIfPresent(String.self, forKey: .repositoryId)
        icon = try c.decodeIfPresent(String.self, forKey: .icon)
        isProtected = c.decodeWireBool(forKey: .isProtected, default: false)
        previewConfig = try c.decodeIfPresent(String.self, forKey: .previewConfig)
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
    public let status: String
    public let priority: String
    public let assigneeId: String?
    public let creatorId: String?
    public let dueDate: String?
    public let dueTime: String?
    public let endTime: String?
    public let sortOrder: Double?
    public let completedAt: String?
    public let archivedAt: String?
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
        priority: String,
        assigneeId: String?,
        creatorId: String?,
        dueDate: String?,
        dueTime: String?,
        endTime: String?,
        sortOrder: Double?,
        completedAt: String?,
        archivedAt: String?,
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
        self.priority = priority
        self.assigneeId = assigneeId
        self.creatorId = creatorId
        self.dueDate = dueDate
        self.dueTime = dueTime
        self.endTime = endTime
        self.sortOrder = sortOrder
        self.completedAt = completedAt
        self.archivedAt = archivedAt
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
        case id, title, description, status, priority, number, identifier, branch
        case boardId = "board_id"
        case assigneeId = "assignee_id"
        case creatorId = "creator_id"
        case dueDate = "due_date"
        case dueTime = "due_time"
        case endTime = "end_time"
        case sortOrder = "sort_order"
        case completedAt = "completed_at"
        case archivedAt = "archived_at"
        case duplicateOfId = "duplicate_of_id"
        case prUrl = "pr_url"
        case prNumber = "pr_number"
        case prState = "pr_state"
        case prMergedAt = "pr_merged_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable for IssueEntity to handle JSONB description field
// Electric delivers JSONB as raw JSON elements — could be object, string, or null
extension IssueEntity: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        boardId = try container.decode(String.self, forKey: .boardId)
        number = try container.decodeWireInt(forKey: .number)
        identifier = try container.decodeIfPresent(String.self, forKey: .identifier)
        title = try container.decode(String.self, forKey: .title)
        status = try container.decode(String.self, forKey: .status)
        priority = try container.decode(String.self, forKey: .priority)
        assigneeId = try container.decodeIfPresent(String.self, forKey: .assigneeId)
        creatorId = try container.decodeIfPresent(String.self, forKey: .creatorId)
        dueDate = try container.decodeIfPresent(String.self, forKey: .dueDate)
        dueTime = try container.decodeIfPresent(String.self, forKey: .dueTime)
        endTime = try container.decodeIfPresent(String.self, forKey: .endTime)
        sortOrder = try container.decodeWireDouble(forKey: .sortOrder)
        completedAt = try container.decodeIfPresent(String.self, forKey: .completedAt)
        archivedAt = try container.decodeIfPresent(String.self, forKey: .archivedAt)
        duplicateOfId = try container.decodeIfPresent(String.self, forKey: .duplicateOfId)
        prUrl = try container.decodeIfPresent(String.self, forKey: .prUrl)
        prNumber = try container.decodeWireInt(forKey: .prNumber)
        prState = try container.decodeIfPresent(String.self, forKey: .prState)
        branch = try container.decodeIfPresent(String.self, forKey: .branch)
        prMergedAt = try container.decodeIfPresent(String.self, forKey: .prMergedAt)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)

        // Handle JSONB description: could be a JSON object, string, or null
        if container.contains(.description) {
            if let stringValue = try? container.decode(String.self, forKey: .description) {
                description = stringValue
            } else if let _ = try? container.decodeNil(forKey: .description) {
                description = nil
            } else {
                // Decode as raw JSON and stringify
                let rawJSON = try container.decode(AnyCodableValue.self, forKey: .description)
                description = rawJSON.jsonString
            }
        } else {
            description = nil
        }
    }
}

// MARK: - CodingSession

// The live "coding now" record — one row per interactive desktop coding session
// (one terminal + one CLI child in one worktree). Synced as the 14th Electric
// shape so every coordination client can show a "coding now" badge. No plan or
// approval state; the PR outcome lives on `issues`. `userId` is the REAL user
// driving the session (not a synthetic bot). Mirrors packages/db-schema
// codingSessions.
public struct CodingSessionEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
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
        case startedAt = "started_at"
        case endedAt = "ended_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
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
    public let isAgent: Bool
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        name: String?,
        email: String,
        image: String?,
        isAgent: Bool = false,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.name = name
        self.email = email
        self.image = image
        self.isAgent = isAgent
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, email, image
        case isAgent = "is_agent"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    // is_agent comes off the Electric wire as a JSON string ("true"/"t"/"1"
    // etc.), a native bool from tRPC/fixtures, or may be absent on an older row —
    // decode it permissively through the type-aware wire helper (absent → the
    // false default). Without the string form, a wire "true" silently defaulted
    // to false and iOS never saw an agent user from a full-row message.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decodeIfPresent(String.self, forKey: .name)
        email = try c.decode(String.self, forKey: .email)
        image = try c.decodeIfPresent(String.self, forKey: .image)
        isAgent = c.decodeWireBool(forKey: .isAgent, default: false)
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
    public let expiresAt: String
    public let acceptedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        role: String,
        token: String?,
        expiresAt: String,
        acceptedAt: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.role = role
        self.token = token
        self.expiresAt = expiresAt
        self.acceptedAt = acceptedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, role, token
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
    // JSON body — Electric delivers as object (e.g. {"text": "..."}). Stored
    // as the stringified JSON; UI decodes lazily via getCommentBodyText().
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

extension CommentEntity: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        issueId = try container.decode(String.self, forKey: .issueId)
        teamId = try container.decode(String.self, forKey: .teamId)
        authorId = try container.decode(String.self, forKey: .authorId)
        kind = (try? container.decodeIfPresent(String.self, forKey: .kind)) ?? "regular"
        editedAt = try container.decodeIfPresent(String.self, forKey: .editedAt)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)

        // Handle JSONB body: object, string, or null
        if container.contains(.body) {
            if let stringValue = try? container.decode(String.self, forKey: .body) {
                body = stringValue
            } else if let _ = try? container.decodeNil(forKey: .body) {
                body = nil
            } else {
                let rawJSON = try container.decode(AnyCodableValue.self, forKey: .body)
                body = rawJSON.jsonString
            }
        } else {
            body = nil
        }
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
    // notification_type: issue_assigned|issue_comment|issue_status_changed|
    //                    issue_mention|pr_opened|pr_merged
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

public func getIssueDescriptionText(_ description: String?) -> String {
    guard let description, let data = description.data(using: .utf8) else { return "" }
    if let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
       let text = dict["text"] as? String {
        return text
    }
    return description
}

public func getCommentBodyText(_ body: String?) -> String {
    guard let body, let data = body.data(using: .utf8) else { return "" }
    if let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
       let text = dict["text"] as? String {
        return text
    }
    // Fallback: if body was stored as a bare string
    return body
}

// MARK: - AnyCodableValue (for JSONB handling)

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
