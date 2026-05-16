import Foundation
import GRDB

// MARK: - Electric Offset

struct ElectricOffset: Codable, FetchableRecord, PersistableRecord {
    static let databaseTableName = "electric_offset"

    let shape: String
    let handle: String
    let offset: String
}

// MARK: - Workspace

struct WorkspaceEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    static let databaseTableName = "workspace"

    let id: String
    let name: String
    let slug: String
    let iconUrl: String?
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, slug
        case iconUrl = "icon_url"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - Project

struct ProjectEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    static let databaseTableName = "project"

    let id: String
    let workspaceId: String
    let name: String
    let slug: String
    let prefix: String
    let color: String?
    let sortOrder: Double?
    let archivedAt: String?
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, slug, prefix, color
        case workspaceId = "workspace_id"
        case sortOrder = "sort_order"
        case archivedAt = "archived_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - Issue

struct IssueEntity: FetchableRecord, PersistableRecord, Identifiable, Sendable {
    static let databaseTableName = "issue"

    let id: String
    let projectId: String
    let number: Int?
    let identifier: String?
    let title: String
    let description: String?
    let status: String
    let priority: String
    let assigneeId: String?
    let creatorId: String?
    let dueDate: String?
    let dueTime: String?
    let endTime: String?
    let sortOrder: Double?
    let completedAt: String?
    let archivedAt: String?
    let recurrenceInterval: Int?
    let recurrenceUnit: String?
    let googleCalendarEventId: String?
    let googleCalendarLastSyncedAt: String?
    let googleCalendarLastSyncError: String?
    let createdAt: String
    let updatedAt: String

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
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// Custom Codable for IssueEntity to handle JSONB description field
// Electric delivers JSONB as raw JSON elements — could be object, string, or null
extension IssueEntity: Codable {
    init(from decoder: Decoder) throws {
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

struct LabelEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    static let databaseTableName = "label"

    let id: String
    let workspaceId: String
    let name: String
    let color: String
    let sortOrder: Double?
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, color
        case workspaceId = "workspace_id"
        case sortOrder = "sort_order"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - IssueLabel

struct IssueLabelEntity: Codable, FetchableRecord, PersistableRecord, Sendable {
    static let databaseTableName = "issue_label"

    let id: String?
    let issueId: String
    let labelId: String
    let workspaceId: String?
    let createdAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case issueId = "issue_id"
        case labelId = "label_id"
        case workspaceId = "workspace_id"
        case createdAt = "created_at"
    }
}

// MARK: - User

struct UserEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    static let databaseTableName = "user"

    let id: String
    let name: String?
    let email: String
    let image: String?
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, email, image
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - WorkspaceMember

struct WorkspaceMemberEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    static let databaseTableName = "workspace_member"

    let id: String
    let workspaceId: String
    let userId: String
    let role: String
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, role
        case workspaceId = "workspace_id"
        case userId = "user_id"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - WorkspaceInvite

struct WorkspaceInviteEntity: Codable, FetchableRecord, PersistableRecord, Identifiable, Sendable {
    static let databaseTableName = "workspace_invite"

    let id: String
    let workspaceId: String
    let role: String
    let token: String
    let expiresAt: String
    let acceptedAt: String?
    let createdAt: String
    let updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, role, token
        case workspaceId = "workspace_id"
        case expiresAt = "expires_at"
        case acceptedAt = "accepted_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

// MARK: - AnyCodableValue (for JSONB handling)

struct AnyCodableValue: Codable, Sendable {
    let jsonString: String

    init(from decoder: Decoder) throws {
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

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(jsonString)
    }
}
