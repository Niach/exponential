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
    public let agentPlanState: String?
    public let agentPlanRevision: Int
    public let agentPlanApprovedAt: String?
    public let agentPlanApprovedBy: String?
    public let agentLastCommentSeenAt: String?
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
        agentPlanRevision: Int,
        agentPlanApprovedAt: String?,
        agentPlanApprovedBy: String?,
        agentLastCommentSeenAt: String?,
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
        self.agentPlanRevision = agentPlanRevision
        self.agentPlanApprovedAt = agentPlanApprovedAt
        self.agentPlanApprovedBy = agentPlanApprovedBy
        self.agentLastCommentSeenAt = agentLastCommentSeenAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, title, description, status, priority, number, identifier
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
        case agentPlanRevision = "agent_plan_revision"
        case agentPlanApprovedAt = "agent_plan_approved_at"
        case agentPlanApprovedBy = "agent_plan_approved_by"
        case agentLastCommentSeenAt = "agent_last_comment_seen_at"
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
        agentPlanRevision = (try? container.decodeIfPresent(Int.self, forKey: .agentPlanRevision)) ?? 0
        agentPlanApprovedAt = try container.decodeIfPresent(String.self, forKey: .agentPlanApprovedAt)
        agentPlanApprovedBy = try container.decodeIfPresent(String.self, forKey: .agentPlanApprovedBy)
        agentLastCommentSeenAt = try container.decodeIfPresent(String.self, forKey: .agentLastCommentSeenAt)
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
