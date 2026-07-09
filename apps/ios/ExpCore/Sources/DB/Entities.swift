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

// MARK: - Workspace

public struct WorkspaceEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "workspaces"

    public let id: String
    public let name: String
    public let slug: String
    public let iconUrl: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        name: String,
        slug: String,
        iconUrl: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.name = name
        self.slug = slug
        self.iconUrl = iconUrl
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    // The public-board machinery moved to a per-project `type` — the workspace
    // shape no longer carries `is_public` / `public_write_policy`. This decoder
    // simply ignores any such legacy keys Electric might still deliver during
    // the one-time shape rotation (unknown keys are dropped by Codable).
    enum CodingKeys: String, CodingKey {
        case id, name, slug
        case iconUrl = "icon_url"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - Project

public struct ProjectEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "projects"

    public let id: String
    public let workspaceId: String
    public let name: String
    public let slug: String
    public let prefix: String
    public let color: String?
    public let sortOrder: Double?
    public let archivedAt: String?
    public let githubRepo: String?
    // v4: the repo backing this project (server-only `repositories` registry
    // row). Synced ride-along on the projects shape — the uuid resolves to a
    // fullName/defaultBranch via the repositories tRPC API (cached per
    // workspace). Now nullable at the source too: only `dev` projects require a
    // repo; `tasks`/`feedback` boards can exist without one.
    public let repositoryId: String?
    // Board type: dev | tasks | feedback (DomainContract.projectType*). Drives
    // the type icon + the repo-required gate on creation. Defaults to `dev`.
    public let type: String
    // Anonymous-visitor visibility toggles — only meaningful on `feedback`
    // boards, inert otherwise.
    public let publicShowComments: Bool
    public let publicShowActivity: Bool
    // off | badge | live (DomainContract.publicCodingVisibility*).
    public let publicShowCoding: String
    // Server-managed protection flag: a protected project (the bootstrap
    // dogfood board) can't be deleted/archived/retyped/repointed. Rides along on
    // the projects shape; clients hide the destructive affordances for it.
    public let isProtected: Bool
    // Display-only mirror of the preview run targets + feedback routing target
    // (jsonb in Postgres). Stored as the raw JSON text; never executed.
    public let previewConfig: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        workspaceId: String,
        name: String,
        slug: String,
        prefix: String,
        color: String?,
        sortOrder: Double?,
        archivedAt: String?,
        githubRepo: String?,
        repositoryId: String?,
        type: String = DomainContract.projectTypeDev,
        publicShowComments: Bool = true,
        publicShowActivity: Bool = false,
        publicShowCoding: String = DomainContract.publicCodingVisibilityOff,
        isProtected: Bool = false,
        previewConfig: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.workspaceId = workspaceId
        self.name = name
        self.slug = slug
        self.prefix = prefix
        self.color = color
        self.sortOrder = sortOrder
        self.archivedAt = archivedAt
        self.githubRepo = githubRepo
        self.repositoryId = repositoryId
        self.type = type
        self.publicShowComments = publicShowComments
        self.publicShowActivity = publicShowActivity
        self.publicShowCoding = publicShowCoding
        self.isProtected = isProtected
        self.previewConfig = previewConfig
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    /// True for `feedback` boards (public, read-only for anonymous visitors).
    public var isFeedbackBoard: Bool { type == DomainContract.projectTypeFeedback }

    enum CodingKeys: String, CodingKey {
        case id, name, slug, prefix, color, type
        case workspaceId = "workspace_id"
        case sortOrder = "sort_order"
        case archivedAt = "archived_at"
        case githubRepo = "github_repo"
        case repositoryId = "repository_id"
        case publicShowComments = "public_show_comments"
        case publicShowActivity = "public_show_activity"
        case publicShowCoding = "public_show_coding"
        case isProtected = "is_protected"
        case previewConfig = "preview_config"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: the type + public-visibility columns land in the one-time
// shape rotation; a pre-rotation snapshot (or a partial update touching other
// columns) may omit them, so decode each permissively with the schema default
// instead of throwing. `public_show_*` may arrive as JSON bool or 0/1.
extension ProjectEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        workspaceId = try c.decode(String.self, forKey: .workspaceId)
        name = try c.decode(String.self, forKey: .name)
        slug = try c.decode(String.self, forKey: .slug)
        prefix = try c.decode(String.self, forKey: .prefix)
        color = try c.decodeIfPresent(String.self, forKey: .color)
        sortOrder = try c.decodeIfPresent(Double.self, forKey: .sortOrder)
        archivedAt = try c.decodeIfPresent(String.self, forKey: .archivedAt)
        githubRepo = try c.decodeIfPresent(String.self, forKey: .githubRepo)
        repositoryId = try c.decodeIfPresent(String.self, forKey: .repositoryId)
        type = (try? c.decodeIfPresent(String.self, forKey: .type))
            .flatMap { $0 } ?? DomainContract.projectTypeDev
        publicShowComments = Self.decodeBool(c, .publicShowComments, default: true)
        publicShowActivity = Self.decodeBool(c, .publicShowActivity, default: false)
        isProtected = Self.decodeBool(c, .isProtected, default: false)
        publicShowCoding = (try? c.decodeIfPresent(String.self, forKey: .publicShowCoding))
            .flatMap { $0 } ?? DomainContract.publicCodingVisibilityOff
        previewConfig = try c.decodeIfPresent(String.self, forKey: .previewConfig)
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }

    private static func decodeBool(
        _ c: KeyedDecodingContainer<CodingKeys>, _ key: CodingKeys, default def: Bool
    ) -> Bool {
        if let b = try? c.decode(Bool.self, forKey: key) { return b }
        if let i = try? c.decode(Int.self, forKey: key) { return i != 0 }
        return def
    }
}

// MARK: - Issue

public struct IssueEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "issues"

    public let id: String
    public let projectId: String
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
    public let recurrenceInterval: Int?
    public let recurrenceUnit: String?
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
        projectId: String,
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
        recurrenceInterval: Int?,
        recurrenceUnit: String?,
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
        self.projectId = projectId
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
        self.recurrenceInterval = recurrenceInterval
        self.recurrenceUnit = recurrenceUnit
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
        case projectId = "project_id"
        case assigneeId = "assignee_id"
        case creatorId = "creator_id"
        case dueDate = "due_date"
        case dueTime = "due_time"
        case endTime = "end_time"
        case sortOrder = "sort_order"
        case completedAt = "completed_at"
        case archivedAt = "archived_at"
        case recurrenceInterval = "recurrence_interval"
        case recurrenceUnit = "recurrence_unit"
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
        projectId = try container.decode(String.self, forKey: .projectId)
        number = try container.decodeIfPresent(Int.self, forKey: .number)
        identifier = try container.decodeIfPresent(String.self, forKey: .identifier)
        title = try container.decode(String.self, forKey: .title)
        status = try container.decode(String.self, forKey: .status)
        priority = try container.decode(String.self, forKey: .priority)
        assigneeId = try container.decodeIfPresent(String.self, forKey: .assigneeId)
        creatorId = try container.decodeIfPresent(String.self, forKey: .creatorId)
        dueDate = try container.decodeIfPresent(String.self, forKey: .dueDate)
        dueTime = try container.decodeIfPresent(String.self, forKey: .dueTime)
        endTime = try container.decodeIfPresent(String.self, forKey: .endTime)
        sortOrder = try container.decodeIfPresent(Double.self, forKey: .sortOrder)
        completedAt = try container.decodeIfPresent(String.self, forKey: .completedAt)
        archivedAt = try container.decodeIfPresent(String.self, forKey: .archivedAt)
        recurrenceInterval = try container.decodeIfPresent(Int.self, forKey: .recurrenceInterval)
        recurrenceUnit = try container.decodeIfPresent(String.self, forKey: .recurrenceUnit)
        duplicateOfId = try container.decodeIfPresent(String.self, forKey: .duplicateOfId)
        prUrl = try container.decodeIfPresent(String.self, forKey: .prUrl)
        prNumber = try container.decodeIfPresent(Int.self, forKey: .prNumber)
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
    public let issueId: String
    public let workspaceId: String
    public let userId: String
    public let deviceLabel: String?
    public let status: String
    public let startedAt: String
    public let endedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        issueId: String,
        workspaceId: String,
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
        self.workspaceId = workspaceId
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
        case workspaceId = "workspace_id"
        case userId = "user_id"
        case deviceLabel = "device_label"
        case startedAt = "started_at"
        case endedAt = "ended_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - Label

public struct LabelEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "labels"

    public let id: String
    public let workspaceId: String
    public let name: String
    public let color: String
    public let sortOrder: Double?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        workspaceId: String,
        name: String,
        color: String,
        sortOrder: Double?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.workspaceId = workspaceId
        self.name = name
        self.color = color
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, color
        case workspaceId = "workspace_id"
        case sortOrder = "sort_order"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - IssueLabel

public struct IssueLabelEntity: Codable, FetchableRecord, PersistableRecord, Sendable {
    public static let databaseTableName = "issue_labels"

    public let issueId: String
    public let labelId: String
    public let workspaceId: String

    public init(issueId: String, labelId: String, workspaceId: String) {
        self.issueId = issueId
        self.labelId = labelId
        self.workspaceId = workspaceId
    }

    enum CodingKeys: String, CodingKey {
        case issueId = "issue_id"
        case labelId = "label_id"
        case workspaceId = "workspace_id"
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

    // Electric may omit is_agent or deliver it as 0/1; decode permissively so an
    // older row (or a non-agent payload without the field) doesn't fail.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decodeIfPresent(String.self, forKey: .name)
        email = try c.decode(String.self, forKey: .email)
        image = try c.decodeIfPresent(String.self, forKey: .image)
        if let b = try? c.decode(Bool.self, forKey: .isAgent) {
            isAgent = b
        } else if let i = try? c.decode(Int.self, forKey: .isAgent) {
            isAgent = i != 0
        } else {
            isAgent = false
        }
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - WorkspaceMember

public struct WorkspaceMemberEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "workspace_members"

    public let id: String
    public let workspaceId: String
    public let userId: String
    public let role: String
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        workspaceId: String,
        userId: String,
        role: String,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.workspaceId = workspaceId
        self.userId = userId
        self.role = role
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, role
        case workspaceId = "workspace_id"
        case userId = "user_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - WorkspaceInvite

public struct WorkspaceInviteEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "workspace_invites"

    public let id: String
    public let workspaceId: String
    public let role: String
    public let token: String
    public let expiresAt: String
    public let acceptedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        workspaceId: String,
        role: String,
        token: String,
        expiresAt: String,
        acceptedAt: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.workspaceId = workspaceId
        self.role = role
        self.token = token
        self.expiresAt = expiresAt
        self.acceptedAt = acceptedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, role, token
        case workspaceId = "workspace_id"
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
    public let workspaceId: String
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
        workspaceId: String,
        authorId: String,
        body: String?,
        kind: String,
        editedAt: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.issueId = issueId
        self.workspaceId = workspaceId
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
        case workspaceId = "workspace_id"
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
        workspaceId = try container.decode(String.self, forKey: .workspaceId)
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

public struct AttachmentEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "attachments"

    public let id: String
    public let workspaceId: String
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
        workspaceId: String,
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
        self.workspaceId = workspaceId
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
        case workspaceId = "workspace_id"
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
    public let workspaceId: String
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
        workspaceId: String,
        source: String,
        unsubscribed: Bool,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.issueId = issueId
        self.userId = userId
        self.email = email
        self.workspaceId = workspaceId
        self.source = source
        self.unsubscribed = unsubscribed
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, source, unsubscribed, email
        case issueId = "issue_id"
        case userId = "user_id"
        case workspaceId = "workspace_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: Electric may deliver `unsubscribed` as JSON boolean (true/false)
// or as the integer 0/1 (SQLite-style). Decode permissively.
extension IssueSubscriberEntity {
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        issueId = try container.decode(String.self, forKey: .issueId)
        userId = try container.decodeIfPresent(String.self, forKey: .userId)
        email = try container.decodeIfPresent(String.self, forKey: .email)
        workspaceId = try container.decode(String.self, forKey: .workspaceId)
        source = try container.decode(String.self, forKey: .source)
        if let boolValue = try? container.decode(Bool.self, forKey: .unsubscribed) {
            unsubscribed = boolValue
        } else if let intValue = try? container.decode(Int.self, forKey: .unsubscribed) {
            unsubscribed = intValue != 0
        } else {
            unsubscribed = false
        }
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - IssueEvent

public struct IssueEventEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "issue_events"

    public let id: String
    public let issueId: String
    public let workspaceId: String
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
        workspaceId: String,
        actorUserId: String?,
        type: String,
        payload: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.issueId = issueId
        self.workspaceId = workspaceId
        self.actorUserId = actorUserId
        self.type = type
        self.payload = payload
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, type, payload
        case issueId = "issue_id"
        case workspaceId = "workspace_id"
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
        workspaceId = try container.decode(String.self, forKey: .workspaceId)
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
