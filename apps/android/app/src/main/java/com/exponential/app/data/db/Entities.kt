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
    // Nullable — a repository is optional on every board (EXP-121). Coding/PR
    // affordances gate on its PRESENCE, never on `type`. repository_id rides on
    // the existing boards shape; the repo name is resolved via the
    // `repositories` tRPC router on demand.
    @ColumnInfo(name = "repository_id") @SerialName("repository_id") @JsonNames("repositoryId") val repositoryId: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
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
    // The dual-written builtin ANCHOR (EXP-314): still one of the 7 enum wire
    // values on every row, so enum-only writers and old clients keep working.
    val status: String,
    // The issue's team status ROW (EXP-314). Nullable: pre-backfill rows and
    // enum-only writes rely on the server trigger deriving it, and clients
    // resolve status_id → anchor → constructed default (IssueStatusResolver).
    @ColumnInfo(name = "status_id") @SerialName("status_id") @JsonNames("statusId") val statusId: String? = null,
    val priority: String,
    @ColumnInfo(name = "assignee_id") @SerialName("assignee_id") @JsonNames("assigneeId") val assigneeId: String? = null,
    @ColumnInfo(name = "creator_id") @SerialName("creator_id") @JsonNames("creatorId") val creatorId: String? = null,
    @ColumnInfo(name = "source") @SerialName("source") @JsonNames("source") val source: String? = null,
    @ColumnInfo(name = "due_date") @SerialName("due_date") @JsonNames("dueDate") val dueDate: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
    @ColumnInfo(name = "completed_at") @SerialName("completed_at") @JsonNames("completedAt") val completedAt: String? = null,
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

// A team's issue statuses (EXP-314, the 16th Electric shape). Every team owns
// 7 LOCKED builtin rows (builtin_key = the anchor enum value) plus any number
// of custom rows; `category` drives glyph/sort/duplicate semantics. Builtin
// rows (and the constructed fallbacks) render today's design-token colors —
// the synced `color` hex is only used for CUSTOM rows (IssueStatusResolver +
// resolvedStatusColor).
@Entity(
    tableName = "issue_statuses",
    indices = [Index("team_id")],
)
@Serializable
data class IssueStatusEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // One of DomainContract.issueStatusCategoryValues; an unknown value from a
    // newer server degrades to the backlog treatment instead of failing.
    val category: String,
    val name: String,
    val color: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double = 0.0,
    // Non-null on the 7 locked builtin rows (the anchor enum wire value);
    // null on custom rows.
    @ColumnInfo(name = "builtin_key") @SerialName("builtin_key") @JsonNames("builtinKey") val builtinKey: String? = null,
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
    // EXP-549/550: the host machine's steer deviceId (= devices.device_id),
    // stamped at start. Joins the row to its LIVE devices row, so the list
    // shows the machine's CURRENT label (device_label is only the start-time
    // snapshot) and a session whose machine went offline renders as paused
    // instead of forever "starting". NULL on rows started before the stamp.
    @ColumnInfo(name = "device_id") @SerialName("device_id") @JsonNames("deviceId") val deviceId: String? = null,
    val status: String = "running",
    // EXP-545: the batch↔PR linkage — the PR's head branch (`exp/batch-<id8>`),
    // stamped by the server's pr_open batch flip alongside the in_review
    // status. Ties a batch row's Merge shortcut to its OWN PR; null on
    // issue-scoped sessions, on action rows, and on batch rows whose PR
    // isn't open yet (or that were flipped before the stamp existed).
    val branch: String? = null,
    // Desktop-written attention flag (EXP-214): the agent is parked on a
    // plan-approval / AskUserQuestion picker and waits for a human.
    @ColumnInfo(name = "needs_input") @SerialName("needs_input") @JsonNames("needsInput") val needsInput: PgBool = false,
    // Action run linkage (EXP-253): set on a session started from a team
    // action. action_id nulls if the action is later deleted (server FK SET
    // NULL) while action_name — a display snapshot — keeps labeling the run.
    // Both null on ordinary issue/batch sessions.
    @ColumnInfo(name = "action_id") @SerialName("action_id") @JsonNames("actionId") val actionId: String? = null,
    @ColumnInfo(name = "action_name") @SerialName("action_name") @JsonNames("actionName") val actionName: String? = null,
    // EXP-530: why an automation started this run (`schedule` | `event`);
    // NULL on every user-started session. Powers the automated-run list and
    // keeps automation rows out of the post-send start watch (StartedRunMatch).
    @ColumnInfo(name = "started_reason") @SerialName("started_reason") @JsonNames("startedReason") val startedReason: String? = null,
    // EXP-583: the automations row that fired this run (FK SET NULL), NULL on
    // every user-started session. The Automations tab's "last run" column
    // joins on it — an action can carry several automations now, so the
    // action_id link no longer identifies which one ran.
    @ColumnInfo(name = "automation_id") @SerialName("automation_id") @JsonNames("automationId") val automationId: String? = null,
    @ColumnInfo(name = "started_at") @SerialName("started_at") @JsonNames("startedAt") val startedAt: String,
    @ColumnInfo(name = "ended_at") @SerialName("ended_at") @JsonNames("endedAt") val endedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

// A team action prompt (EXP-253, synced via the actions shape since EXP-268).
// The shape deliberately EXCLUDES the ≤64KB markdown `body` — mobile is
// view + run only, and the desktop fetches the body via tRPC `actions.get`
// right before a run.
@Entity(
    tableName = "actions",
    indices = [Index("team_id")],
)
@Serializable
data class ActionEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // Null for repo-less actions (the desktop runs those in a scratch dir).
    @ColumnInfo(name = "repository_id") @SerialName("repository_id") @JsonNames("repositoryId") val repositoryId: String? = null,
    val name: String,
    val description: String? = null,
    // EXP-273: curated registry icon name (same set as boards.icon); null =
    // the generic action glyph.
    val icon: String? = null,
    // jsonb array of typed run-input defs ({key,label,type,required,placeholder}
    // — EXP-257), kept as its raw JSON string and parsed at the consumer.
    @Serializable(with = JsonAsStringSerializer::class) val inputs: String? = null,
    // DEAD since EXP-583: automations became their own table + shape, and the
    // server dropped this column. The local column stays (nullable, always
    // NULL now) because removing it would need a Room migration for no gain —
    // nothing reads it. Do not resurrect it.
    @Serializable(with = JsonAsStringSerializer::class) val trigger: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

// One automation (EXP-583, the 19th Electric shape): an action + a bound
// device + the WHEN-part trigger, team-scoped. Split out of `actions.trigger`
// so an action can carry several automations (and none by default). The bound
// device selects its own enabled rows off this shape and fires locally —
// there is no server scheduler. `agent`/`model`/`effort` NULL = the device's
// own launch defaults.
@Entity(
    tableName = "automations",
    indices = [Index("team_id"), Index("action_id")],
)
@Serializable
data class AutomationEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    @ColumnInfo(name = "action_id") @SerialName("action_id") @JsonNames("actionId") val actionId: String,
    // The steer deviceId (= devices.device_id) of the machine that runs it.
    @ColumnInfo(name = "device_id") @SerialName("device_id") @JsonNames("deviceId") val deviceId: String = "",
    val enabled: PgBool = true,
    // The when-part jsonb as its raw JSON string, parsed tolerantly at the
    // consumer (AutomationTrigger.parse — unknown kinds read as null).
    @Serializable(with = JsonAsStringSerializer::class) val trigger: String? = null,
    val agent: String? = null,
    val model: String? = null,
    val effort: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double = 0.0,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
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
    // NULLABLE (REV-7), mirroring the server column: a widget screenshot
    // attachment has no human uploader, and the FK is ON DELETE SET NULL, so a
    // deleted account nulls the uploader on attachments it left behind in a
    // surviving team. Required here, a null insert failed to decode (dropped
    // forever) and a SET NULL partial update threw NOT NULL inside the batch,
    // stalling the attachments shape on every poll.
    @ColumnInfo(name = "uploader_id") @SerialName("uploader_id") @JsonNames("uploaderId") val uploaderId: String? = null,
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

// A registered machine (EXP-481, the 17th Electric shape): the caller's own
// devices plus SERVER machines teammates share with a common team. Rows are
// SERVER-AUTHORITATIVE device state — `launch_defaults` is the canonical copy
// of the machine's per-agent coding defaults (its local settings.json
// converges), and online-ness derives CLIENT-side from `last_seen_at`
// freshness (DeviceLiveness — devices heartbeat ~30s; no relay presence in
// the sync path). Every field that can be absent is defaulted: a required
// field missing on the wire silently drops the row forever (the
// attachments.uploader_id lesson).
@Entity(
    tableName = "devices",
    indices = [Index("user_id")],
)
@Serializable
data class DeviceEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String,
    // The steer deviceId (start target) — NOT the row id.
    @ColumnInfo(name = "device_id") @SerialName("device_id") @JsonNames("deviceId") val deviceId: String,
    val label: String = "",
    /** `desktop` (the IDE) or `server` (a headless `exponential` daemon). */
    val kind: String = "desktop",
    val platform: String? = null,
    val version: String? = null,
    // jsonb string arrays, kept as raw JSON text and parsed at the consumer
    // (DeviceRows) — the ActionEntity.inputs idiom.
    @Serializable(with = JsonAsStringSerializer::class) val agents: String? = null,
    @Serializable(with = JsonAsStringSerializer::class) val caps: String? = null,
    @ColumnInfo(name = "unauthed_agents") @SerialName("unauthed_agents") @JsonNames("unauthedAgents")
    @Serializable(with = JsonAsStringSerializer::class) val unauthedAgents: String? = null,
    // The server-authoritative per-agent launch defaults (EXP-481) — a jsonb
    // object stored as its raw JSON text; NULL = never set, clients seed
    // static contract defaults.
    @ColumnInfo(name = "launch_defaults") @SerialName("launch_defaults") @JsonNames("launchDefaults")
    @Serializable(with = JsonAsStringSerializer::class) val launchDefaults: String? = null,
    @ColumnInfo(name = "launch_defaults_updated_at") @SerialName("launch_defaults_updated_at") @JsonNames("launchDefaultsUpdatedAt")
    val launchDefaultsUpdatedAt: String? = null,
    @ColumnInfo(name = "active_sessions") @SerialName("active_sessions") @JsonNames("activeSessions")
    val activeSessions: Int = 0,
    @ColumnInfo(name = "last_seen_at") @SerialName("last_seen_at") @JsonNames("lastSeenAt")
    val lastSeenAt: String? = null,
    // EXP-432: the ONE team this (server) machine is shared with; null = private.
    @ColumnInfo(name = "shared_team_id") @SerialName("shared_team_id") @JsonNames("sharedTeamId")
    val sharedTeamId: String? = null,
    // EXP-622: the ROW OWNER's default machine — the one every device picker
    // prefills. Honoured only when `user_id` is the signed-in user: a
    // teammate's shared server carries THEIR preference, not ours.
    @ColumnInfo(name = "is_default") @SerialName("is_default") @JsonNames("isDefault")
    val isDefault: PgBool = false,
    // Web "Update" click pending on the daemon (cleared by its next register).
    @ColumnInfo(name = "update_requested_at") @SerialName("update_requested_at") @JsonNames("updateRequestedAt")
    val updateRequestedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

// One worktree a device reported (EXP-481, the 18th Electric shape) — powers
// the remote resume offer and the device-settings worktree list, from
// persisted data even while the machine is offline. `device_row_id` is the
// devices ROW id (uuid), never the steer device-id string.
@Entity(
    tableName = "device_worktrees",
    indices = [Index("device_row_id")],
)
@Serializable
data class DeviceWorktreeEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "device_row_id") @SerialName("device_row_id") @JsonNames("deviceRowId")
    val deviceRowId: String,
    @ColumnInfo(name = "repo_full_name") @SerialName("repo_full_name") @JsonNames("repoFullName")
    val repoFullName: String = "",
    val branch: String = "",
    // `exp/<IDENTIFIER>` linkage as the DEVICE parsed it; null on foreign
    // branches. Clients join against their own synced issues.
    @ColumnInfo(name = "issue_identifier") @SerialName("issue_identifier") @JsonNames("issueIdentifier")
    val issueIdentifier: String? = null,
    // Agents recorded in the worktree's .exp-agents resume marker (jsonb
    // string array as raw JSON text); NULL = pre-marker worktree, any agent
    // may resume.
    @Serializable(with = JsonAsStringSerializer::class) val agents: String? = null,
    // Documented varchar: clean | untracked | tracked | unknown.
    val dirty: String = "unknown",
    // A live local session currently holds this worktree's branch.
    val busy: PgBool = false,
    @ColumnInfo(name = "reported_at") @SerialName("reported_at") @JsonNames("reportedAt")
    val reportedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

@Entity(tableName = "electric_offsets")
data class ElectricOffsetEntity(
    @PrimaryKey @ColumnInfo(name = "shape") val shape: String,
    val handle: String,
    val offset: String,
    // True once an up-to-date control was seen — only then may polls long-poll
    // with live=true; catch-up polls stay non-live per the Electric protocol.
    @ColumnInfo(name = "is_live") val isLive: Boolean = false,
    // Set when Electric told us to re-snapshot (409/400 or an inline
    // must-refetch) and cleared once the snapshot lands. The rows are NOT wiped
    // when this is set: the next poll requests offset=-1 and prepends the wipe
    // to its own batch, so the swap is one transaction and the UI never blanks.
    @ColumnInfo(name = "needs_refetch") val needsRefetch: Boolean = false,
)
