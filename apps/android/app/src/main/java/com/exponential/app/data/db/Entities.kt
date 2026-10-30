package com.exponential.app.data.db

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueSearch
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
    // (SLOP-4: the team-level support switch is gone — a widget submission
    // IS an issue, and the reporter conversation is its comments.)
    // EXP-1105: yolo mode — the team's agents merge their own PRs, so the
    // Reviews tab hides unless an open PR (= a failed auto-merge) is waiting.
    // Toggled on web + desktop only; an absent column = off.
    @ColumnInfo(name = "yolo_mode") @SerialName("yolo_mode") @JsonNames("yoloMode") val yoloMode: PgBool = false,
    // EXP-630: the team's estimate scale (contract `issueEstimation`:
    // none | exponential | fibonacci | linear | tshirt). `none` (and a row
    // without the column) = estimates OFF, so the
    // chip and the properties row hide; issues keep their point values.
    @ColumnInfo(name = "estimation_type") @SerialName("estimation_type") @JsonNames("estimationType") val estimationType: String? = null,
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
    // EXP-712: the board's OWN branch — the base its coding-session worktrees
    // branch from and the base its PRs target. NULL = follow the backing
    // repo's (team-pin aware) default branch, so two boards on one repo can
    // develop on different branches. Only meaningful with a repository.
    @ColumnInfo(name = "default_branch") @SerialName("default_branch") @JsonNames("defaultBranch") val defaultBranch: String? = null,
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
    @PrimaryKey override val id: String,
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String,
    val number: Int,
    override val identifier: String,
    override val title: String,
    @Serializable(with = JsonAsStringSerializer::class) override val description: String? = null,
    // The dual-written builtin ANCHOR (EXP-314): still one of the 7 enum wire
    // values on every row, so enum-only writers and old clients keep working.
    // EXP-922: it is also `IssueSearch.Row.status` — undone issues rank above
    // done ones in every search.
    override val status: String,
    // The issue's team status ROW (EXP-314). Nullable: pre-backfill rows and
    // enum-only writes rely on the server trigger deriving it, and clients
    // resolve status_id → anchor → constructed default (IssueStatusResolver).
    @ColumnInfo(name = "status_id") @SerialName("status_id") @JsonNames("statusId") val statusId: String? = null,
    val priority: String,
    @ColumnInfo(name = "assignee_id") @SerialName("assignee_id") @JsonNames("assigneeId") val assigneeId: String? = null,
    @ColumnInfo(name = "creator_id") @SerialName("creator_id") @JsonNames("creatorId") val creatorId: String? = null,
    @ColumnInfo(name = "source") @SerialName("source") @JsonNames("source") val source: String? = null,
    @ColumnInfo(name = "due_date") @SerialName("due_date") @JsonNames("dueDate") val dueDate: String? = null,
    // EXP-630: story points — always a point NUMBER, rendered on the team's
    // `estimation_type` scale (domain/IssueEstimate.kt). NULL = not estimated.
    val estimate: Int? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
    @ColumnInfo(name = "completed_at") @SerialName("completed_at") @JsonNames("completedAt") val completedAt: String? = null,
    @ColumnInfo(name = "duplicate_of_id") @SerialName("duplicate_of_id") @JsonNames("duplicateOfId") val duplicateOfId: String? = null,
    // EXP-1231: the draft this issue was created from (`issues.create({draftId})`
    // stamps it). An open draft page that sees an issue carrying its draft id
    // replaces itself with that issue ([com.exponential.app.domain.IssueDraftPage.fate]).
    @ColumnInfo(name = "draft_id") @SerialName("draft_id") @JsonNames("draftId") val draftId: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") override val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") override val updatedAt: String,
    // #EXP-1383: an AGENT assignee (exclusive with `assignee_id`; the server
    // clears one when it sets the other) and the agent whose session filed it.
    @ColumnInfo(name = "assignee_agent_id") @SerialName("assignee_agent_id") @JsonNames("assigneeAgentId") val assigneeAgentId: String? = null,
    @ColumnInfo(name = "creator_agent_id") @SerialName("creator_agent_id") @JsonNames("creatorAgentId") val creatorAgentId: String? = null,
    // EXP-892: the ONE issue-search engine ranks these rows directly (Search
    // tab, the pickers) — the interface is pure projection, nothing is stored.
) : IssueSearch.Row

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

// One issue relation (EXP-736, the 20th Electric shape). The row is a DIRECTED
// edge in the canonical direction — `issue_id` blocks / parents / duplicates
// `related_issue_id`; `related` is symmetric and normalized server-side — so
// each side renders its own label off IssueRelationType. Board-scoped by the
// SOURCE issue, which is why `board_id` is nullable here like every other
// child mirror.
@Entity(
    tableName = "issue_relations",
    indices = [Index("issue_id"), Index("related_issue_id"), Index("team_id")],
)
@Serializable
data class IssueRelationEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    @ColumnInfo(name = "related_issue_id") @SerialName("related_issue_id") @JsonNames("relatedIssueId") val relatedIssueId: String,
    // One of DomainContract.issueRelationTypeValues; an unknown value from a
    // newer server renders as a plain link instead of failing.
    val type: String,
    // DomainContract.issueRelationSourceValues — `user` (picked) or
    // `reference` (auto-linked from a #IDENT mention).
    val source: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
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
    // EXP-630: the placeholder member this email invite created — a roster row
    // (users + team_members) that exists before the person signs in. Member
    // lists badge it "Invited" / "Invite expired" while `accepted_at` is null.
    // Null for link invites and for invites to an existing account.
    @ColumnInfo(name = "placeholder_user_id") @SerialName("placeholder_user_id") @JsonNames("placeholderUserId") val placeholderUserId: String? = null,
    // EXP-1076: when the invite LINK was issued. Null = a roster row nobody
    // was ever invited (the Linear import seats its placeholders without a
    // mail) — member lists read "Not invited", never "Invite expired" (such
    // rows are minted with `expires_at = created_at`).
    @ColumnInfo(name = "sent_at") @SerialName("sent_at") @JsonNames("sentAt") val sentAt: String? = null,
    @ColumnInfo(name = "expires_at") @SerialName("expires_at") @JsonNames("expiresAt") val expiresAt: String,
    @ColumnInfo(name = "accepted_at") @SerialName("accepted_at") @JsonNames("acceptedAt") val acceptedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

@Entity(
    tableName = "comments",
    indices = [Index("issue_id"), Index("team_id"), Index("parent_id")],
)
@Serializable
data class CommentEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // Denormalized issue→board id (v7 server trigger).
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String? = null,
    // SLOP-4: NULL for a widget reporter's comment (source `reporter`) — the
    // card names the submission's reporter instead of a users row.
    @ColumnInfo(name = "author_id") @SerialName("author_id") @JsonNames("authorId") val authorId: String? = null,
    // EXP-741: the top-level comment this one replies to (one level deep);
    // null = a top-level card.
    @ColumnInfo(name = "parent_id") @SerialName("parent_id") @JsonNames("parentId") val parentId: String? = null,
    // EXP-741: `user` | `mcp` — an agent posted it over MCP ("via MCP");
    // SLOP-4: `reporter` — the widget reporter wrote it from their page.
    val source: String? = null,
    // SLOP-4: `team` | `reporter` — a member reply the server emailed to the
    // reporter carries `reporter` (the "to reporter" caption). Older servers
    // omit the column: every row then reads as a team comment.
    val audience: String = com.exponential.app.domain.DomainContract.commentAudienceTeam,
    @Serializable(with = JsonAsStringSerializer::class) val body: String? = null,
    val kind: String = "regular",
    @ColumnInfo(name = "edited_at") @SerialName("edited_at") @JsonNames("editedAt") val editedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
    // #EXP-1383: the agent whose session wrote it (the card names the agent
    // and wears its mark; `author_id` keeps the member for audit).
    @ColumnInfo(name = "author_agent_id") @SerialName("author_agent_id") @JsonNames("authorAgentId") val authorAgentId: String? = null,
)

enum class CommentKind { Regular }

/** EXP-741: an agent posted this comment over MCP (the "via MCP" caption). */
val CommentEntity.isViaMcp: Boolean
    get() = source == com.exponential.app.domain.DomainContract.commentSourceMcp

/** SLOP-4: the widget reporter wrote it (the "reporter" caption, no author row). */
val CommentEntity.isFromReporter: Boolean
    get() = source == com.exponential.app.domain.DomainContract.commentSourceReporter

/** SLOP-4: a member's reply the server emailed to the reporter ("to reporter"). */
val CommentEntity.isToReporter: Boolean
    get() = !isFromReporter &&
        audience == com.exponential.app.domain.DomainContract.commentAudienceReporter

// Comment kinds collapsed to regular-only (contract commentKindValues = ["regular"]);
// tolerant decode maps any legacy value to Regular.
fun commentKindOf(raw: String?): CommentKind = CommentKind.Regular

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
    // EXP-824 inline media: probed playback length of a `video/*` / `audio/*`
    // row and the storage key of its poster frame (non-null = the poster route
    // `/api/attachments/{id}?poster=1` serves one). Both nullable with
    // defaults: a required field would stall the shape on a partial update.
    @ColumnInfo(name = "duration_ms") @SerialName("duration_ms") @JsonNames("durationMs") val durationMs: Long? = null,
    @ColumnInfo(name = "poster_storage_key") @SerialName("poster_storage_key") @JsonNames("posterStorageKey") val posterStorageKey: String? = null,
    // EXP-1247: uploaded through a FILE/paperclip path — never inlined, always
    // a Files row, whatever its type (`AttachmentFiles.isFileAttachment`).
    @ColumnInfo(name = "as_file", defaultValue = "0") @SerialName("as_file") @JsonNames("asFile") val asFile: PgBool = false,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
) {
    /** True when the server holds a poster frame for this media row. */
    val hasPoster: Boolean get() = posterStorageKey != null
}

@Entity(
    tableName = "notifications",
    indices = [Index("user_id", "read_at")],
)
@Serializable
data class NotificationEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String? = null,
    // Set on issue-less agent_message / session_blocked rows (the team they
    // belong to); NULL on issue-anchored rows (a reporter_reply too, SLOP-4).
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String? = null,
    // EXP-980: the coding run a `session_blocked` row is about — the inbox row
    // and its push both route to it. NULL on every other type, and on a row
    // whose run has since been pruned.
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String? = null,
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
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,    // #EXP-1383: the agent whose session made the change (the row names it).
    @ColumnInfo(name = "actor_agent_id") @SerialName("actor_agent_id") @JsonNames("actorAgentId") val actorAgentId: String? = null,
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
    // EXP-924: the owner-picked display icon (contract `deviceIcon`, the
    // registry's `devicePickable` set). NULL — which is what every machine
    // registers with — is
    // the KIND default the clients derive (`deviceIconName`), never a hole.
    val icon: String? = null,
    val version: String? = null,
    // jsonb string arrays, kept as raw JSON text and parsed at the consumer
    // (DeviceRows) — the ActionEntity.inputs idiom.
    @Serializable(with = JsonAsStringSerializer::class) val agents: String? = null,
    @Serializable(with = JsonAsStringSerializer::class) val caps: String? = null,
    @ColumnInfo(name = "unauthed_agents") @SerialName("unauthed_agents") @JsonNames("unauthedAgents")
    @Serializable(with = JsonAsStringSerializer::class) val unauthedAgents: String? = null,
    // EXP-749: the agents this machine runs through the in-process ACP engine
    // (the session screen). The column is nullable and NULL still arrives for
    // a registry row whose machine never reported one, but it no longer MEANS
    // anything: the consumer reads it as "none", and a runnable agent missing
    // from the list cannot start there at all (EXP-773).
    @ColumnInfo(name = "acp_agents") @SerialName("acp_agents") @JsonNames("acpAgents")
    @Serializable(with = JsonAsStringSerializer::class) val acpAgents: String? = null,
    // The server-authoritative per-agent launch defaults (EXP-481) — a jsonb
    // object stored as its raw JSON text; NULL = never set, clients seed
    // static contract defaults.
    @ColumnInfo(name = "launch_defaults") @SerialName("launch_defaults") @JsonNames("launchDefaults")
    @Serializable(with = JsonAsStringSerializer::class) val launchDefaults: String? = null,
    @ColumnInfo(name = "launch_defaults_updated_at") @SerialName("launch_defaults_updated_at") @JsonNames("launchDefaultsUpdatedAt")
    val launchDefaultsUpdatedAt: String? = null,
    // EXP-484: the machine's READ-ONLY per-agent auth + usage status, shipped
    // on register/heartbeat. Two jsonb objects kept as their raw JSON text and
    // parsed at the consumer (AgentUsagePresentation) — the launch_defaults
    // idiom. `agent_accounts` is { [agent]: { signedIn, email?, plan?,
    // checkedAt } }, `agent_usage` { [agent]: { fetchedAt, stale, windows[] } }.
    // No credential ever rides here.
    @ColumnInfo(name = "agent_accounts") @SerialName("agent_accounts") @JsonNames("agentAccounts")
    @Serializable(with = JsonAsStringSerializer::class) val agentAccounts: String? = null,
    @ColumnInfo(name = "agent_usage") @SerialName("agent_usage") @JsonNames("agentUsage")
    @Serializable(with = JsonAsStringSerializer::class) val agentUsage: String? = null,
    // Server stamp of the last usage write. Moves every 3-10 minutes, so it is
    // NEVER a sync-nudge trigger — only a display fallback ("as of ...").
    @ColumnInfo(name = "agent_usage_at") @SerialName("agent_usage_at") @JsonNames("agentUsageAt")
    val agentUsageAt: String? = null,
    // EXP-1196/1218/1219: the machine's readiness report (`{ checkedAt,
    // items[] }`), shipped on register/heartbeat — raw JSON text, parsed at
    // the consumer (`parseDeviceDoctor`). NULL = an older build: no block.
    @Serializable(with = JsonAsStringSerializer::class) val doctor: String? = null,
    @ColumnInfo(name = "active_sessions") @SerialName("active_sessions") @JsonNames("activeSessions")
    val activeSessions: Int = 0,
    @ColumnInfo(name = "last_seen_at") @SerialName("last_seen_at") @JsonNames("lastSeenAt")
    val lastSeenAt: String? = null,
    // FEED-33: every team this (server) machine is shared with; empty =
    // private. A Postgres uuid[] — Electric ships it as the text literal
    // `{a,b}` inside a JSON string, hence the tolerant serializer.
    @ColumnInfo(name = "shared_team_ids") @SerialName("shared_team_ids") @JsonNames("sharedTeamIds")
    @Serializable(with = PgUuidArraySerializer::class) val sharedTeamIds: List<String> = emptyList(),
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

// One personal pin (EXP-778, the 21st Electric shape): the caller's own
// "Pinned" sidebar rows — an issue or a session, exactly one of the two ids set
// (`kind` names which). The shape is static per user (`user_id = me`), NOT
// team- or trash-scoped: the client renders only the pins of the active team
// whose TARGET resolves from the other synced tables, so a pin whose
// issue/session is not synced is simply hidden. Every field
// that can be absent is defaulted (the attachments.uploader_id lesson).
@Entity(
    tableName = "pins",
    indices = [Index("team_id"), Index("issue_id"), Index("session_id")],
)
@Serializable
data class PinEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String = "",
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // Contract `pinKind`: issue | session; any other kind is never stored
    // ([isKept]).
    val kind: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String? = null,
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String? = null,
    // Ascending = display order (a Postgres double, appended one past the tail).
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double = 0.0,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
) {
    /** A kind outside the contract (the dropped `action`) is ignored. */
    val isKept: Boolean get() = kind in com.exponential.app.domain.DomainContract.pinKindValues
}

// One unsent issue draft (EXP-878, the 22nd Electric shape): the create
// screen's form, saved when the screen closes with content in it. The shape is
// static per user (`user_id = me`), NOT team- or trash-scoped — exactly like
// pins — so a draft renders only once its BOARD resolves from the synced
// boards table. Every field but the PK defaults: a column absent on the wire
// must never drop the row (the attachments.uploader_id lesson).
@Entity(
    tableName = "issue_drafts",
    indices = [Index("user_id"), Index("team_id"), Index("board_id")],
)
@Serializable
data class IssueDraftEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String = "",
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    @ColumnInfo(name = "board_id") @SerialName("board_id") @JsonNames("boardId") val boardId: String = "",
    val title: String = "",
    // Plain GFM, like issues.description. Inline images are already FINAL
    // `/api/attachments/{id}` URLs — the draft path uploads eagerly, so a
    // `draft://` placeholder never reaches the server.
    val description: String = "",
    // The precise per-team status row; null = the team's Backlog builtin.
    @ColumnInfo(name = "status_id") @SerialName("status_id") @JsonNames("statusId")
    val statusId: String? = null,
    val priority: String = "none",
    @ColumnInfo(name = "assignee_id") @SerialName("assignee_id") @JsonNames("assigneeId")
    val assigneeId: String? = null,
    // A Postgres uuid[] — Electric ships it as the text literal `{a,b}` inside
    // a JSON string, hence the tolerant serializer (devices.shared_team_ids).
    @ColumnInfo(name = "label_ids") @SerialName("label_ids") @JsonNames("labelIds")
    @Serializable(with = PgUuidArraySerializer::class) val labelIds: List<String> = emptyList(),
    // `YYYY-MM-DD` — the date-only due date (no time of day, REV2-49).
    @ColumnInfo(name = "due_date") @SerialName("due_date") @JsonNames("dueDate")
    val dueDate: String? = null,
    // EXP-1291: "New issue" from a thread — the thread ROOT session the issue
    // files into (create links it), so the draft keeps it across leave and
    // reopen. Synced as `session_id` (the local column keeps its v86 name).
    // SET NULL server-side: a deleted thread leaves a plain draft.
    @ColumnInfo(name = "thread_id") @SerialName("session_id") @JsonNames("sessionId", "thread_id", "threadId")
    val threadId: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",    // #EXP-1383: an AGENT assignee (exclusive with `assignee_id`).
    @ColumnInfo(name = "assignee_agent_id") @SerialName("assignee_agent_id") @JsonNames("assigneeAgentId")
    val assigneeAgentId: String? = null,
)

// EXP-1291 Threads v2: ONE session tree (the `sessions` shape). A thread IS a
// root session (`kind = thread`, `parent_id` NULL); a team agent is a root of
// `kind = agent`; children are sub-sessions (kind `thread`, `parent_id` /
// `root_id` / `depth`). Visible = `shared OR user_id = me` inside the member
// teams. jsonb columns stay their raw TEXT off the wire (parsed at the
// consumer, tolerant). Every field but the PK defaults: a column absent on
// the wire must never drop the row. NEW-2: an agent is a definition
// (`agents`, its `instructions` server-only via `agents.get`); a root with
// `agent_id` is one member's conversation with it.
@Entity(
    tableName = "sessions",
    indices = [Index("team_id"), Index("root_id"), Index("parent_id")],
)
@Serializable
data class SessionEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    @ColumnInfo(name = "parent_id") @SerialName("parent_id") @JsonNames("parentId") val parentId: String? = null,
    // The root's id (its own on a root).
    @ColumnInfo(name = "root_id") @SerialName("root_id") @JsonNames("rootId") val rootId: String = "",
    @Serializable(with = PgIntSerializer::class) val depth: Int? = 0,
    // The STARTER (visibility keys on it).
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String = "",
    // The steer deviceId of the machine that runs it.
    @ColumnInfo(name = "device_id") @SerialName("device_id") @JsonNames("deviceId") val deviceId: String = "",
    // The agent CLI (contract `codingAgent`), its account profile, model, effort.
    val cli: String? = null,
    val account: String? = null,
    val model: String? = null,
    val effort: String? = null,
    @ColumnInfo(name = "plan_mode") @SerialName("plan_mode") @JsonNames("planMode") val planMode: PgBool = false,
    @ColumnInfo(name = "computer_use") @SerialName("computer_use") @JsonNames("computerUse") val computerUse: PgBool = false,
    val ultracode: PgBool = false,
    @ColumnInfo(name = "repository_id") @SerialName("repository_id") @JsonNames("repositoryId") val repositoryId: String? = null,
    // The session's working branch (device-written at claim).
    val branch: String? = null,
    // Contract `sessionStatus`: queued → running ⇄ idle → ended.
    val status: String = com.exponential.app.domain.DomainContract.sessionStatusQueued,
    // Device-written turn state: the ONLY list-spinner input ×4.
    val busy: PgBool = false,
    // EXP-1374: busy ONLY because background tasks (shell, workflow,
    // subagent) still run after the turn (device-written).
    @ColumnInfo(name = "background", defaultValue = "0") val background: PgBool = false,
    // A question/plan card waits for a person (the "Needs you" group).
    @ColumnInfo(name = "needs_input") @SerialName("needs_input") @JsonNames("needsInput") val needsInput: PgBool = false,
    // EXP-804: the usage wall (`{kind, agent, window, resetsAt, since}`), raw
    // jsonb TEXT; NULL = not blocked. A blocked session stays live.
    @ColumnInfo(name = "blocked") @SerialName("blocked")
    @Serializable(with = JsonAsStringSerializer::class) val blocked: String? = null,
    @ColumnInfo(name = "started_at") @SerialName("started_at") @JsonNames("startedAt") val startedAt: String? = null,
    @ColumnInfo(name = "last_turn_at") @SerialName("last_turn_at") @JsonNames("lastTurnAt") val lastTurnAt: String? = null,
    @ColumnInfo(name = "ended_at") @SerialName("ended_at") @JsonNames("endedAt") val endedAt: String? = null,
    // Stamped at creation: the device is shared with the team.
    val shared: PgBool = false,
    // A root's title (NULL = derived for display, `SessionTree.threadTitle`);
    // on a child the CLI's auto-name.
    val title: String? = null,
    // The thread's ONE Guide (the PR body), raw jsonb TEXT of
    // `CodingSessionResult[]`; NULL or `[]` = nothing published.
    @ColumnInfo(name = "guide") @SerialName("guide")
    @Serializable(with = JsonAsStringSerializer::class) val guide: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
    // NEW-2: the agent whose conversation this root is (one per agent and
    // member); NULL = a thread or a child. The kind derives from it.
    @ColumnInfo(name = "agent_id") @SerialName("agent_id") @JsonNames("agentId") val agentId: String? = null,
    // VAPP-110: the app this root is one member's EDITOR SESSION of (one per
    // app and member); NULL = anything else. Never a thread.
    @ColumnInfo(name = "app_id") @SerialName("app_id") @JsonNames("appId") val appId: String? = null,
    // #EXP-1383: when this session last fetched its agent's brief (the
    // "Instructions changed" banner compares it with the agent's stamp).
    @ColumnInfo(name = "agent_instructions_at") @SerialName("agent_instructions_at") @JsonNames("agentInstructionsAt") val agentInstructionsAt: String? = null,
    // #EXP-1383: the agent whose session started this root (an agent tree).
    @ColumnInfo(name = "started_by_agent_id") @SerialName("started_by_agent_id") @JsonNames("startedByAgentId") val startedByAgentId: String? = null,
) {
    val isRoot: Boolean get() = parentId == null
    /** NEW-2: an agent's conversation session ([SessionKind.sessionKindOf]). */
    val isAgent: Boolean get() = !agentId.isNullOrEmpty()
    val isEnded: Boolean get() = status == com.exponential.app.domain.DomainContract.sessionStatusEnded
}

// NEW-2: a team agent = a DEFINITION, never a session. It runs as
// conversation sessions (`sessions.agent_id`, one per agent and member).
// `instructions` + `trigger_state` are server-only (tRPC `agents.get`);
// `triggers` = raw jsonb TEXT of `[{id, enabled, kind: schedule, cron, tz?} |
// {id, enabled, kind: event, name}]` (contract `agentTrigger`).
@Entity(
    tableName = "agents",
    indices = [Index("team_id")],
)
@Serializable
data class AgentDefinitionEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    val name: String = "",
    val icon: String? = null,
    val color: String? = null,
    @ColumnInfo(name = "triggers") @SerialName("triggers")
    @Serializable(with = JsonAsStringSerializer::class) val triggers: String? = null,
    // The steer deviceId its conversations run on, the CLI, account and model.
    @ColumnInfo(name = "device_id") @SerialName("device_id") @JsonNames("deviceId") val deviceId: String = "",
    val cli: String? = null,
    val account: String? = null,
    val model: String? = null,
    @ColumnInfo(name = "owner_id") @SerialName("owner_id") @JsonNames("ownerId") val ownerId: String = "",
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double = 0.0,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
    // #EXP-1383: stamped only when the instructions TEXT changes.
    @ColumnInfo(name = "instructions_updated_at") @SerialName("instructions_updated_at") @JsonNames("instructionsUpdatedAt") val instructionsUpdatedAt: String? = null,
    // S-3: contract `agentRole` (assistant | agent); tolerant: missing = agent.
    @ColumnInfo(name = "role") val role: String? = null,
) {
    /** S-3: the member's own Assistant (brand mark, orchestrator + voice). */
    val isAssistant: Boolean get() = role == com.exponential.app.domain.DomainContract.agentRoleAssistant
}

// NEW-2: an event (free-text `name`, raw jsonb `payload`), team-scoped;
// `emitted_by` = contract `eventEmitter` (session | user | server).
@Entity(
    tableName = "events",
    indices = [Index("team_id")],
)
@Serializable
data class EventEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    val name: String = "",
    @ColumnInfo(name = "payload") @SerialName("payload")
    @Serializable(with = JsonAsStringSerializer::class) val payload: String? = null,
    @ColumnInfo(name = "emitted_by") @SerialName("emitted_by") @JsonNames("emittedBy") val emittedBy: String = "",
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String? = null,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
)

// VAPP-110: a team's APP (fixtures/apps.json): a declarative Exponential UI
// package, the HEAD version's `package` riding along as raw jsonb TEXT (the
// editor's preview repaints from it); `head_version` 0 = no version yet;
// `deleted_at` = the trash mirror (hidden).
@Entity(
    tableName = "apps",
    indices = [Index("team_id")],
)
@Serializable
data class AppEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    val slug: String = "",
    val name: String = "",
    val icon: String? = null,
    val color: String? = null,
    val description: String = "",
    @ColumnInfo(name = "package") @SerialName("package")
    @Serializable(with = JsonAsStringSerializer::class) val packageJson: String? = null,
    @ColumnInfo(name = "head_version") @SerialName("head_version") @JsonNames("headVersion")
    @Serializable(with = PgIntSerializer::class) val headVersion: Int? = 0,
    // Release train 2026-10-23: apps.created_by is nullable server-side (migration 0186, ON DELETE SET NULL); Room keeps NOT NULL, "" means no creator.
    @ColumnInfo(name = "created_by") @SerialName("created_by") @JsonNames("createdBy")
    @Serializable(with = NullAsEmptyStringSerializer::class) val createdBy: String = "",
    // VAPP-120: the typed actions the app may run as the viewer without asking
    // (a JSON string[] of exact `exponential.<noun>.<op>` names; "[]" = none).
    @ColumnInfo(name = "granted_tools") @SerialName("granted_tools") @JsonNames("grantedTools")
    @Serializable(with = JsonAsStringSerializer::class) val grantedTools: String? = "[]",
    @ColumnInfo(name = "deleted_at") @SerialName("deleted_at") @JsonNames("deletedAt") val deletedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

// VAPP-110: one version of an app (the Versions list); its `package` stays
// behind the shape (tRPC `apps.get {version}` on demand).
@Entity(
    tableName = "app_versions",
    indices = [Index("app_id"), Index("team_id")],
)
@Serializable
data class AppVersionEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "app_id") @SerialName("app_id") @JsonNames("appId") val appId: String = "",
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    @Serializable(with = PgIntSerializer::class) val number: Int? = 0,
    val note: String? = null,
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String? = null,
    @ColumnInfo(name = "created_by") @SerialName("created_by") @JsonNames("createdBy") val createdBy: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
)

// VAPP-122: one RECORD of an app (its own data): a JSON object `data` of a
// `kind` the app's manifest declares. Trash never reaches a client (the
// shape's static predicates), so a delete is the row leaving the shape.
@Entity(
    tableName = "app_records",
    indices = [Index(value = ["app_id", "kind"]), Index("team_id")],
)
@Serializable
data class AppRecordEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "app_id") @SerialName("app_id") @JsonNames("appId") val appId: String = "",
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    val kind: String = "",
    @ColumnInfo(name = "data") @SerialName("data")
    @Serializable(with = JsonAsStringSerializer::class) val data: String? = null,
    @ColumnInfo(name = "created_by") @SerialName("created_by") @JsonNames("createdBy") val createdBy: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

// NEW-2: a saved prompt (replaces actions): name + curated icon; the body is
// server-only (tRPC `prompts.get`).
@Entity(
    tableName = "prompts",
    indices = [Index("team_id")],
)
@Serializable
data class PromptEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    val name: String = "",
    val icon: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double = 0.0,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

// EXP-1291: a thread's issues (n ↔ n), always on the ROOT session. Hidden
// with the issue's board server-side (static trash/archive predicates).
@Entity(
    tableName = "session_issues",
    indices = [Index("session_id"), Index("issue_id"), Index("team_id")],
)
@Serializable
data class SessionIssueEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String = "",
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String = "",
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    // Contract `sessionIssueSource`: user | agent.
    val source: String = com.exponential.app.domain.DomainContract.sessionIssueSourceUser,
    @ColumnInfo(name = "added_by") @SerialName("added_by") @JsonNames("addedBy") val addedBy: String? = null,
    @ColumnInfo(name = "added_by_session_id") @SerialName("added_by_session_id") @JsonNames("addedBySessionId")
    val addedBySessionId: String? = null,
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double = 0.0,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

// EXP-1291: the Conversation — one row per person message, agent turn reply
// and card. `consumed_at` NULL on a person's message = still queued (the
// queue bar ×4). `card` = raw jsonb TEXT (`{kind, …}`, contract
// `messageCardKind`); unknown kinds render nothing.
@Entity(
    tableName = "messages",
    indices = [Index("conversation_id"), Index("root_id"), Index("team_id")],
)
@Serializable
data class MessageEntity(
    @PrimaryKey val id: String,
    // NEW-8: the conversation the row lives in; its session (if any) comes
    // from the `conversations` row ([com.exponential.app.domain.MessageConversation]),
    // never from a legacy `messages.session_id` (not stored; the shape
    // decoder drops it).
    @ColumnInfo(name = "conversation_id") @SerialName("conversation_id") @JsonNames("conversationId")
    val conversationId: String = "",
    @ColumnInfo(name = "root_id") @SerialName("root_id") @JsonNames("rootId") val rootId: String = "",
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    // Contract `messageAuthorKind`: user | agent | system.
    @ColumnInfo(name = "author_kind") @SerialName("author_kind") @JsonNames("authorKind")
    val authorKind: String = com.exponential.app.domain.DomainContract.messageAuthorKindSystem,
    @ColumnInfo(name = "author_id") @SerialName("author_id") @JsonNames("authorId") val authorId: String? = null,
    // EXP-1323: the SESSION that posted it (a child asking its parent, a
    // child's report); NULL = a person or the server.
    @ColumnInfo(name = "author_session_id") @SerialName("author_session_id") @JsonNames("authorSessionId")
    val authorSessionId: String? = null,
    val body: String = "",
    @ColumnInfo(name = "card") @SerialName("card")
    @Serializable(with = JsonAsStringSerializer::class) val card: String? = null,
    @ColumnInfo(name = "answered_at") @SerialName("answered_at") @JsonNames("answeredAt") val answeredAt: String? = null,
    @ColumnInfo(name = "consumed_at") @SerialName("consumed_at") @JsonNames("consumedAt") val consumedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

// NEW-3: the chat primitive. Step 1: every row is a session's own (kind
// `session`), visible exactly when its session is; dm/channel rows arrive
// with NEW-8.
@Entity(
    tableName = "conversations",
    indices = [Index("team_id"), Index("session_id")],
)
@Serializable
data class ConversationEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    // Contract `conversationKind`: dm | channel | session.
    val kind: String = com.exponential.app.domain.DomainContract.conversationKindSession,
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String? = null,
    val title: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
    // Thread e4a04d22: the Chats list's per-conversation digest (jsonb kept
    // by the server's `conversation_digest` trigger), stored as JSON text
    // like messages.card; read through [com.exponential.app.domain.Chats.digestMessages].
    @Serializable(with = JsonAsStringSerializer::class) val digest: String? = null,
)

// EXP-1323: the viewer's read mark of one session (a thread = its root's
// row), upserted by `sessions.markRead` when a thread or a session view opens.
// Static per user like pins (`user_id = me`), never trash-scoped; a row only
// matters beside a synced session.
@Entity(
    tableName = "session_reads",
    indices = [Index("session_id")],
)
@Serializable
data class SessionReadEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String = "",
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String = "",
    @ColumnInfo(name = "read_at") @SerialName("read_at") @JsonNames("readAt") val readAt: String = "",
    // #EXP-1388: the viewer archived the chat (per viewer, on the root's row).
    @ColumnInfo(name = "archived_at") @SerialName("archived_at") @JsonNames("archivedAt") val archivedAt: String? = null,
    // S-3 (stories.json): the greatest story createdAt the viewer was shown
    // in this root's tree (`sessions.markStoriesSeen`; null = none).
    @ColumnInfo(name = "story_seen_at") @SerialName("story_seen_at") @JsonNames("storySeenAt") val storySeenAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

// EXP-1291: a pull request (all PR + branch state lives here, never on
// issues). Team-visible. `base_pull_request_id` = the open PR this one is
// stacked on (native GitHub stacks).
@Entity(
    tableName = "pull_requests",
    indices = [Index("team_id"), Index("root_session_id"), Index("session_id")],
)
@Serializable
data class PullRequestEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    @ColumnInfo(name = "root_session_id") @SerialName("root_session_id") @JsonNames("rootSessionId") val rootSessionId: String? = null,
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String? = null,
    @ColumnInfo(name = "repository_id") @SerialName("repository_id") @JsonNames("repositoryId") val repositoryId: String = "",
    @Serializable(with = PgIntSerializer::class) val number: Int? = null,
    val url: String = "",
    val title: String? = null,
    val head: String = "",
    val base: String = "",
    @ColumnInfo(name = "base_pull_request_id") @SerialName("base_pull_request_id") @JsonNames("basePullRequestId")
    val basePullRequestId: String? = null,
    // Contract `prState`: open | merged | closed.
    val state: String = com.exponential.app.domain.DomainContract.prStateOpen,
    @ColumnInfo(name = "merged_at") @SerialName("merged_at") @JsonNames("mergedAt") val mergedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

// EXP-1291: which issues a PR closes (n ↔ n).
@Entity(
    tableName = "pull_request_issues",
    indices = [Index("pull_request_id"), Index("issue_id"), Index("team_id")],
)
@Serializable
data class PullRequestIssueEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "pull_request_id") @SerialName("pull_request_id") @JsonNames("pullRequestId") val pullRequestId: String = "",
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String = "",
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
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
