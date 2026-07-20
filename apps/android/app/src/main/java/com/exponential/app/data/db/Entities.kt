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

@Entity(tableName = "teams")
@Serializable
data class TeamEntity(
    @PrimaryKey val id: String,
    val name: String,
    val slug: String,
    @ColumnInfo(name = "icon_url") @SerialName("icon_url") @JsonNames("iconUrl") val iconUrl: String? = null,
    // Team-level helpdesk switch (EXP-180): when on, every member gets the
    // "Support" inbox (standalone tickets with external reporters — not issues).
    @ColumnInfo(name = "helpdesk_enabled") @SerialName("helpdesk_enabled") @JsonNames("helpdeskEnabled") val helpdeskEnabled: PgBool = false,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "boards",
    indices = [Index("team_id")],
)
@Serializable
data class BoardEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    val name: String,
    val slug: String,
    val prefix: String,
    val color: String,
    // Curated display icon (one of contract boardIconValues) or null for
    // pre-collapse rows — the client falls back to a shape-derived glyph then.
    val icon: String? = null,
    // Server-owned protection flag (the dogfood board). A protected
    // board can't be deleted/archived, so clients hide the delete affordance.
    @ColumnInfo(name = "is_protected") @SerialName("is_protected") @JsonNames("isProtected") val isProtected: PgBool = false,
    // Nullable — a repository is optional on every board (EXP-121). Coding/PR
    // affordances gate on its PRESENCE, never on `type`. repository_id rides on
    // the existing boards shape; the repo name is resolved via the
    // `repositories` tRPC router on demand.
    @ColumnInfo(name = "repository_id") @SerialName("repository_id") @JsonNames("repositoryId") val repositoryId: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
    @ColumnInfo(name = "archived_at") @SerialName("archived_at") @JsonNames("archivedAt") val archivedAt: String? = null,
    // Soft-delete (trash) marker — part of the boards shape contract. Always
    // NULL inside the shape (the server where-clause excludes trashed rows; a
    // trash arrives as a delete/move-out message), but queries still filter on
    // it defensively so a stale pre-delete row can never resurface.
    @ColumnInfo(name = "deleted_at") @SerialName("deleted_at") @JsonNames("deletedAt") val deletedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "issues",
    indices = [Index("board_id"), Index("status"), Index("assignee_id"), Index("due_date")],
)
@Serializable
data class IssueEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String,
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
    @ColumnInfo(name = "duplicate_of_id") @SerialName("duplicate_of_id") @JsonNames("duplicateOfId") val duplicateOfId: String? = null,
    // PR fields stay: merge detection (webhook + polling) still populates these.
    @ColumnInfo(name = "pr_url") @SerialName("pr_url") @JsonNames("prUrl") val prUrl: String? = null,
    @ColumnInfo(name = "pr_number") @SerialName("pr_number") @JsonNames("prNumber") val prNumber: Int? = null,
    @ColumnInfo(name = "pr_state") @SerialName("pr_state") @JsonNames("prState") val prState: String? = null,
    val branch: String? = null,
    @ColumnInfo(name = "pr_merged_at") @SerialName("pr_merged_at") @JsonNames("prMergedAt") val prMergedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "labels",
    indices = [Index("team_id")],
)
@Serializable
data class LabelEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    val name: String,
    val color: String,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "issue_labels",
    primaryKeys = ["issue_id", "label_id"],
    indices = [Index("label_id"), Index("team_id")],
)
@Serializable
data class IssueLabelEntity(
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    @ColumnInfo(name = "label_id") @SerialName("label_id") @JsonNames("labelId") val labelId: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // Denormalized issue→board id (v7 server trigger); stored so tolerant-apply
    // stops reporting it dropped. Nullable default for legacy-row decode.
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String? = null,
)

@Entity(tableName = "users")
@Serializable
data class UserEntity(
    @PrimaryKey val id: String,
    val name: String? = null,
    val email: String,
    val image: String? = null,
    @ColumnInfo(name = "is_agent") @SerialName("is_agent") @JsonNames("isAgent") val isAgent: PgBool = false,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "team_members",
    indices = [Index("team_id"), Index("user_id")],
)
@Serializable
data class TeamMemberEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String,
    val role: String,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "team_invites",
    indices = [Index("team_id"), Index("token")],
)
@Serializable
data class TeamInviteEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // Who created the invite (synced with the shape; not rendered yet).
    @ColumnInfo(name = "invited_by_id") @SerialName("invited_by_id") @JsonNames("invitedById") val invitedById: String? = null,
    val role: String,
    // No longer synced (server columns allowlist — the invite token is a
    // bearer secret; owners get it once from the create mutation). Nullable
    // default so token-less shape rows decode.
    val token: String? = null,
    // Optional invited address (EXP-188 invite-by-email) — display metadata
    // for the pending list; the server mails the invite link when it's set.
    val email: String? = null,
    @ColumnInfo(name = "expires_at") @SerialName("expires_at") @JsonNames("expiresAt") val expiresAt: String,
    @ColumnInfo(name = "accepted_at") @SerialName("accepted_at") @JsonNames("acceptedAt") val acceptedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "comments",
    indices = [Index("issue_id"), Index("team_id")],
)
@Serializable
data class CommentEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // Denormalized issue→board id (v7 server trigger).
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String? = null,
    @ColumnInfo(name = "author_id") @SerialName("author_id") @JsonNames("authorId") val authorId: String,
    @Serializable(with = JsonAsStringSerializer::class) val body: String? = null,
    val kind: String = "regular",
    @ColumnInfo(name = "edited_at") @SerialName("edited_at") @JsonNames("editedAt") val editedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

enum class CommentKind { Regular }

// Comment kinds collapsed to regular-only (contract commentKindValues = ["regular"]);
// tolerant decode maps any legacy value to Regular.
fun commentKindOf(raw: String?): CommentKind = CommentKind.Regular

// A coding session against an issue (synced via the coding_sessions shape): a
// real user driving a coding agent from a desktop device. Replaces agent_runs.
@Entity(
    tableName = "coding_sessions",
    indices = [Index("issue_id"), Index("team_id")],
)
@Serializable
data class CodingSessionEntity(
    @PrimaryKey val id: String,
    // Nullable for batch multi-issue runs (a desktop batch spans issues, so the
    // session isn't tied to a single one).
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String? = null,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // Denormalized issue→board id (v7 server trigger); NULL for
    // batch sessions (a batch run spans boards).
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String? = null,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String,
    @ColumnInfo(name = "device_label") @SerialName("device_label") @JsonNames("deviceLabel") val deviceLabel: String? = null,
    val status: String = "running",
    // Desktop-written attention flag (EXP-214): the agent is parked on a
    // plan-approval / AskUserQuestion picker and waits for a human.
    @ColumnInfo(name = "needs_input") @SerialName("needs_input") @JsonNames("needsInput") val needsInput: PgBool = false,
    @ColumnInfo(name = "started_at") @SerialName("started_at") @JsonNames("startedAt") val startedAt: String,
    @ColumnInfo(name = "ended_at") @SerialName("ended_at") @JsonNames("endedAt") val endedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "attachments",
    indices = [Index("issue_id"), Index("team_id")],
)
@Serializable
data class AttachmentEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    // Denormalized issue→board id (v7 server trigger).
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String? = null,
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

@Entity(
    tableName = "notifications",
    indices = [Index("user_id", "read_at")],
)
@Serializable
data class NotificationEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String? = null,
    // Set on issue-less support_reply rows (the helpdesk ticket's team); NULL on issue-anchored rows.
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String? = null,
    val type: String,
    val title: String,
    val body: String? = null,
    @ColumnInfo(name = "read_at") @SerialName("read_at") @JsonNames("readAt") val readAt: String? = null,
    @ColumnInfo(name = "pushed_at") @SerialName("pushed_at") @JsonNames("pushedAt") val pushedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "issue_subscribers",
    indices = [Index("user_id"), Index("team_id")],
)
@Serializable
data class IssueSubscriberEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    // Nullable now: widget-reporter rows carry an email instead of a user_id.
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String? = null,
    val email: String? = null,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    val source: String,
    val unsubscribed: PgBool = false,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "issue_events",
    indices = [Index("issue_id"), Index("team_id")],
)
@Serializable
data class IssueEventEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // Denormalized issue→board id (v7 server trigger).
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String? = null,
    @ColumnInfo(name = "actor_user_id") @SerialName("actor_user_id") @JsonNames("actorUserId") val actorUserId: String? = null,
    val type: String,
    @Serializable(with = JsonAsStringSerializer::class) val payload: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(tableName = "electric_offsets")
data class ElectricOffsetEntity(
    @PrimaryKey @ColumnInfo(name = "shape") val shape: String,
    val handle: String,
    val offset: String,
    // True once an up-to-date control was seen — only then may polls long-poll
    // with live=true; catch-up polls stay non-live per the Electric protocol.
    @ColumnInfo(name = "is_live") val isLive: Boolean = false,
)
