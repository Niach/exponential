package com.exponential.app.data.db

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonNames

// Wire-format inconsistency we have to live with: Electric SQL delivers
// rows in PostgreSQL snake_case, but Drizzle queries return rows with the
// JS-side camelCase property names — and tRPC handlers forward those.
// @JsonNames lets each field accept either name on deserialization.

@Entity(tableName = "workspaces")
@Serializable
data class WorkspaceEntity(
    @PrimaryKey val id: String,
    val name: String,
    val slug: String,
    @ColumnInfo(name = "icon_url") @SerialName("icon_url") @JsonNames("iconUrl") val iconUrl: String? = null,
    @ColumnInfo(name = "is_public") @SerialName("is_public") @JsonNames("isPublic") val isPublic: Boolean = false,
    @ColumnInfo(name = "public_write_policy") @SerialName("public_write_policy") @JsonNames("publicWritePolicy") val publicWritePolicy: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "projects",
    indices = [Index("workspace_id")],
)
@Serializable
data class ProjectEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") @JsonNames("workspaceId") val workspaceId: String,
    val name: String,
    val slug: String,
    val prefix: String,
    val color: String,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
    @ColumnInfo(name = "archived_at") @SerialName("archived_at") @JsonNames("archivedAt") val archivedAt: String? = null,
    @ColumnInfo(name = "github_repo") @SerialName("github_repo") @JsonNames("githubRepo") val githubRepo: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "issues",
    indices = [Index("project_id"), Index("status"), Index("assignee_id"), Index("due_date")],
)
@Serializable
data class IssueEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "project_id") @SerialName("project_id") @JsonNames("projectId") val projectId: String,
    val number: Int,
    val identifier: String,
    val title: String,
    @Serializable(with = JsonAsStringSerializer::class) val description: String? = null,
    val status: String,
    val priority: String,
    @ColumnInfo(name = "assignee_id") @SerialName("assignee_id") @JsonNames("assigneeId") val assigneeId: String? = null,
    @ColumnInfo(name = "creator_id") @SerialName("creator_id") @JsonNames("creatorId") val creatorId: String,
    @ColumnInfo(name = "due_date") @SerialName("due_date") @JsonNames("dueDate") val dueDate: String? = null,
    @ColumnInfo(name = "due_time") @SerialName("due_time") @JsonNames("dueTime") val dueTime: String? = null,
    @ColumnInfo(name = "end_time") @SerialName("end_time") @JsonNames("endTime") val endTime: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
    @ColumnInfo(name = "completed_at") @SerialName("completed_at") @JsonNames("completedAt") val completedAt: String? = null,
    @ColumnInfo(name = "archived_at") @SerialName("archived_at") @JsonNames("archivedAt") val archivedAt: String? = null,
    @ColumnInfo(name = "recurrence_interval") @SerialName("recurrence_interval") @JsonNames("recurrenceInterval") val recurrenceInterval: Int? = null,
    @ColumnInfo(name = "recurrence_unit") @SerialName("recurrence_unit") @JsonNames("recurrenceUnit") val recurrenceUnit: String? = null,
    @ColumnInfo(name = "google_calendar_event_id") @SerialName("google_calendar_event_id") @JsonNames("googleCalendarEventId") val googleCalendarEventId: String? = null,
    @ColumnInfo(name = "google_calendar_last_synced_at") @SerialName("google_calendar_last_synced_at") @JsonNames("googleCalendarLastSyncedAt") val googleCalendarLastSyncedAt: String? = null,
    @ColumnInfo(name = "google_calendar_last_sync_error") @SerialName("google_calendar_last_sync_error") @JsonNames("googleCalendarLastSyncError") val googleCalendarLastSyncError: String? = null,
    @ColumnInfo(name = "agent_plan_state") @SerialName("agent_plan_state") @JsonNames("agentPlanState") val agentPlanState: String? = null,
    @ColumnInfo(name = "agent_plan_revision") @SerialName("agent_plan_revision") @JsonNames("agentPlanRevision") val agentPlanRevision: Int = 0,
    @ColumnInfo(name = "agent_plan_approved_at") @SerialName("agent_plan_approved_at") @JsonNames("agentPlanApprovedAt") val agentPlanApprovedAt: String? = null,
    @ColumnInfo(name = "agent_plan_approved_by") @SerialName("agent_plan_approved_by") @JsonNames("agentPlanApprovedBy") val agentPlanApprovedBy: String? = null,
    @ColumnInfo(name = "agent_last_comment_seen_at") @SerialName("agent_last_comment_seen_at") @JsonNames("agentLastCommentSeenAt") val agentLastCommentSeenAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "labels",
    indices = [Index("workspace_id")],
)
@Serializable
data class LabelEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") @JsonNames("workspaceId") val workspaceId: String,
    val name: String,
    val color: String,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "issue_labels",
    primaryKeys = ["issue_id", "label_id"],
    indices = [Index("label_id"), Index("workspace_id")],
)
@Serializable
data class IssueLabelEntity(
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    @ColumnInfo(name = "label_id") @SerialName("label_id") @JsonNames("labelId") val labelId: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") @JsonNames("workspaceId") val workspaceId: String,
)

@Entity(tableName = "users")
@Serializable
data class UserEntity(
    @PrimaryKey val id: String,
    val name: String? = null,
    val email: String,
    val image: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "workspace_members",
    indices = [Index("workspace_id"), Index("user_id")],
)
@Serializable
data class WorkspaceMemberEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") @JsonNames("workspaceId") val workspaceId: String,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String,
    val role: String,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "workspace_invites",
    indices = [Index("workspace_id"), Index("token")],
)
@Serializable
data class WorkspaceInviteEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") @JsonNames("workspaceId") val workspaceId: String,
    val role: String,
    val token: String,
    @ColumnInfo(name = "expires_at") @SerialName("expires_at") @JsonNames("expiresAt") val expiresAt: String,
    @ColumnInfo(name = "accepted_at") @SerialName("accepted_at") @JsonNames("acceptedAt") val acceptedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "comments",
    indices = [Index("issue_id"), Index("workspace_id")],
)
@Serializable
data class CommentEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") @JsonNames("workspaceId") val workspaceId: String,
    @ColumnInfo(name = "author_id") @SerialName("author_id") @JsonNames("authorId") val authorId: String,
    @Serializable(with = JsonAsStringSerializer::class) val body: String? = null,
    val kind: String = "regular",
    @ColumnInfo(name = "edited_at") @SerialName("edited_at") @JsonNames("editedAt") val editedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

enum class CommentKind { Regular, Question, Plan }

fun commentKindOf(raw: String?): CommentKind = when (raw) {
    "question" -> CommentKind.Question
    "plan" -> CommentKind.Plan
    else -> CommentKind.Regular
}

@Entity(
    tableName = "attachments",
    indices = [Index("issue_id"), Index("workspace_id")],
)
@Serializable
data class AttachmentEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "workspace_id") @SerialName("workspace_id") @JsonNames("workspaceId") val workspaceId: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    @ColumnInfo(name = "comment_id") @SerialName("comment_id") @JsonNames("commentId") val commentId: String? = null,
    @ColumnInfo(name = "uploader_id") @SerialName("uploader_id") @JsonNames("uploaderId") val uploaderId: String,
    val filename: String,
    @ColumnInfo(name = "content_type") @SerialName("content_type") @JsonNames("contentType") val contentType: String,
    @ColumnInfo(name = "size_bytes") @SerialName("size_bytes") @JsonNames("sizeBytes") val sizeBytes: Long,
    @ColumnInfo(name = "storage_key") @SerialName("storage_key") @JsonNames("storageKey") val storageKey: String,
    val url: String,
    // Probed image dimensions (parity with iOS) so the client can pre-size and
    // avoid layout shift. Nullable for non-image / not-yet-probed attachments.
    val width: Int? = null,
    val height: Int? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(tableName = "electric_offsets")
data class ElectricOffsetEntity(
    @PrimaryKey @ColumnInfo(name = "shape") val shape: String,
    val handle: String,
    val offset: String,
)
