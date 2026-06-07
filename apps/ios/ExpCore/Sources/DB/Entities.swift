import Foundation
import GRDB

// MARK: - Electric Offset

public struct ElectricOffset: Codable, FetchableRecord, PersistableRecord, Sendable {
    public static let databaseTableName = "electric_offsets"

    public let shape: String
    public let handle: String
    public let offset: String

    public init(shape: String, handle: String, offset: String) {
        self.shape = shape
        self.handle = handle
        self.offset = offset
    }
}

// MARK: - Workspace

public struct WorkspaceEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "workspaces"

    public let id: String
    public let name: String
    public let slug: String
    public let iconUrl: String?
    public let isPublic: Bool
    public let publicWritePolicy: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        name: String,
        slug: String,
        iconUrl: String?,
        isPublic: Bool,
        publicWritePolicy: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.name = name
        self.slug = slug
        self.iconUrl = iconUrl
        self.isPublic = isPublic
        self.publicWritePolicy = publicWritePolicy
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, slug
        case iconUrl = "icon_url"
        case isPublic = "is_public"
        case publicWritePolicy = "public_write_policy"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable: Electric may deliver `is_public` as JSON boolean (true/false)
// or as the integer 0/1 (SQLite-style). Decode permissively.
extension WorkspaceEntity: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        slug = try container.decode(String.self, forKey: .slug)
        iconUrl = try container.decodeIfPresent(String.self, forKey: .iconUrl)
        if let boolValue = try? container.decode(Bool.self, forKey: .isPublic) {
            isPublic = boolValue
        } else if let intValue = try? container.decode(Int.self, forKey: .isPublic) {
            isPublic = intValue != 0
        } else {
            isPublic = false
        }
        publicWritePolicy = try container.decodeIfPresent(String.self, forKey: .publicWritePolicy)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)
    }
}

// MARK: - Project

public struct ProjectEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
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
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, name, slug, prefix, color
        case workspaceId = "workspace_id"
        case sortOrder = "sort_order"
        case archivedAt = "archived_at"
        case githubRepo = "github_repo"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
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
    public let googleCalendarEventId: String?
    public let googleCalendarLastSyncedAt: String?
    public let googleCalendarLastSyncError: String?
    // Agent run state lives in `agent_runs` (its own synced shape) as of Phase F;
    // issues keeps only the summary `agentPlanState` + the PR summary columns.
    public let agentPlanState: String?
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
        googleCalendarEventId: String?,
        googleCalendarLastSyncedAt: String?,
        googleCalendarLastSyncError: String?,
        agentPlanState: String?,
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
        self.googleCalendarEventId = googleCalendarEventId
        self.googleCalendarLastSyncedAt = googleCalendarLastSyncedAt
        self.googleCalendarLastSyncError = googleCalendarLastSyncError
        self.agentPlanState = agentPlanState
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
        case googleCalendarEventId = "google_calendar_event_id"
        case googleCalendarLastSyncedAt = "google_calendar_last_synced_at"
        case googleCalendarLastSyncError = "google_calendar_last_sync_error"
        case agentPlanState = "agent_plan_state"
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
        googleCalendarEventId = try container.decodeIfPresent(String.self, forKey: .googleCalendarEventId)
        googleCalendarLastSyncedAt = try container.decodeIfPresent(String.self, forKey: .googleCalendarLastSyncedAt)
        googleCalendarLastSyncError = try container.decodeIfPresent(String.self, forKey: .googleCalendarLastSyncError)
        agentPlanState = try container.decodeIfPresent(String.self, forKey: .agentPlanState)
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

// MARK: - AgentRun

// The agent run state for an issue (one row per issue). Split out of `issues`
// into its own synced shape in Phase F; mirrors packages/db-schema agentRuns.
// `planText` and `question` are server-authored jsonb `{text}` — stored as the
// raw stringified JSON and unwrapped lazily via getCommentBodyText() (the same
// envelope as comments.body / issues.description legacy rows).
public struct AgentRunEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    public static let databaseTableName = "agent_runs"

    public let issueId: String
    public let workspaceId: String
    public let planText: String?
    public let question: String?
    public let questionAskedAt: String?
    public let planRevision: Int
    public let approvedAt: String?
    public let approvedBy: String?
    public let lastCommentSeenAt: String?
    public let sessionId: String?
    public let runMode: String?
    public let interactiveClaimedAt: String?
    public let interactiveClaimedExpiresAt: String?
    public let lastError: String?
    public let createdAt: String
    public let updatedAt: String

    public var id: String { issueId }

    public init(
        issueId: String,
        workspaceId: String,
        planText: String?,
        question: String?,
        questionAskedAt: String?,
        planRevision: Int,
        approvedAt: String?,
        approvedBy: String?,
        lastCommentSeenAt: String?,
        sessionId: String?,
        runMode: String?,
        interactiveClaimedAt: String?,
        interactiveClaimedExpiresAt: String?,
        lastError: String?,
        createdAt: String,
        updatedAt: String
    ) {
        self.issueId = issueId
        self.workspaceId = workspaceId
        self.planText = planText
        self.question = question
        self.questionAskedAt = questionAskedAt
        self.planRevision = planRevision
        self.approvedAt = approvedAt
        self.approvedBy = approvedBy
        self.lastCommentSeenAt = lastCommentSeenAt
        self.sessionId = sessionId
        self.runMode = runMode
        self.interactiveClaimedAt = interactiveClaimedAt
        self.interactiveClaimedExpiresAt = interactiveClaimedExpiresAt
        self.lastError = lastError
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case question
        case issueId = "issue_id"
        case workspaceId = "workspace_id"
        case planText = "plan_text"
        case questionAskedAt = "question_asked_at"
        case planRevision = "plan_revision"
        case approvedAt = "approved_at"
        case approvedBy = "approved_by"
        case lastCommentSeenAt = "last_comment_seen_at"
        case sessionId = "session_id"
        case runMode = "run_mode"
        case interactiveClaimedAt = "interactive_claimed_at"
        case interactiveClaimedExpiresAt = "interactive_claimed_expires_at"
        case lastError = "last_error"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

extension AgentRunEntity: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        issueId = try container.decode(String.self, forKey: .issueId)
        workspaceId = try container.decode(String.self, forKey: .workspaceId)
        questionAskedAt = try container.decodeIfPresent(String.self, forKey: .questionAskedAt)
        planRevision = (try? container.decodeIfPresent(Int.self, forKey: .planRevision)) ?? 0
        approvedAt = try container.decodeIfPresent(String.self, forKey: .approvedAt)
        approvedBy = try container.decodeIfPresent(String.self, forKey: .approvedBy)
        lastCommentSeenAt = try container.decodeIfPresent(String.self, forKey: .lastCommentSeenAt)
        sessionId = try container.decodeIfPresent(String.self, forKey: .sessionId)
        runMode = try container.decodeIfPresent(String.self, forKey: .runMode)
        interactiveClaimedAt = try container.decodeIfPresent(String.self, forKey: .interactiveClaimedAt)
        interactiveClaimedExpiresAt = try container.decodeIfPresent(String.self, forKey: .interactiveClaimedExpiresAt)
        lastError = try container.decodeIfPresent(String.self, forKey: .lastError)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)

        // jsonb {text}: object, string, or null (same handling as CommentEntity.body)
        planText = AgentRunEntity.decodeJsonbText(container, forKey: .planText)
        question = AgentRunEntity.decodeJsonbText(container, forKey: .question)
    }

    private static func decodeJsonbText(
        _ container: KeyedDecodingContainer<CodingKeys>, forKey key: CodingKeys
    ) -> String? {
        guard container.contains(key) else { return nil }
        if let stringValue = try? container.decode(String.self, forKey: key) {
            return stringValue
        }
        if (try? container.decodeNil(forKey: key)) == true {
            return nil
        }
        if let rawJSON = try? container.decode(AnyCodableValue.self, forKey: key) {
            return rawJSON.jsonString
        }
        return nil
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
    // notification_type: issue_assigned|issue_comment|issue_status_changed|issue_mention
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
    public let userId: String
    public let workspaceId: String
    // source: creator|assignee|commenter|manual|mention
    public let source: String
    public let unsubscribed: Bool
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        issueId: String,
        userId: String,
        workspaceId: String,
        source: String,
        unsubscribed: Bool,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.issueId = issueId
        self.userId = userId
        self.workspaceId = workspaceId
        self.source = source
        self.unsubscribed = unsubscribed
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, source, unsubscribed
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
        userId = try container.decode(String.self, forKey: .userId)
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
    //       pr_opened|pr_merged|plan_ready|agent_error
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
