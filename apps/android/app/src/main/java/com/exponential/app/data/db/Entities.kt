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
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double,
    @ColumnInfo(name = "completed_at") @SerialName("completed_at") @JsonNames("completedAt") val completedAt: String? = null,
    @ColumnInfo(name = "duplicate_of_id") @SerialName("duplicate_of_id") @JsonNames("duplicateOfId") val duplicateOfId: String? = null,
    // PR fields stay: merge detection (webhook + polling) still populates these.
    @ColumnInfo(name = "pr_url") @SerialName("pr_url") @JsonNames("prUrl") val prUrl: String? = null,
    @ColumnInfo(name = "pr_number") @SerialName("pr_number") @JsonNames("prNumber") val prNumber: Int? = null,
    @ColumnInfo(name = "pr_state") @SerialName("pr_state") @JsonNames("prState") val prState: String? = null,
    val branch: String? = null,
    // EXP-897: the branch this issue's pull request is BASED on — the stack
    // edge (`child.pr_base_branch == lower.branch`). NULL on an ordinary PR
    // cut from the board's default branch; the server's own `pr_stack_number`
    // is not synced.
    @ColumnInfo(name = "pr_base_branch") @SerialName("pr_base_branch") @JsonNames("prBaseBranch") val prBaseBranch: String? = null,
    @ColumnInfo(name = "pr_merged_at") @SerialName("pr_merged_at") @JsonNames("prMergedAt") val prMergedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") override val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") override val updatedAt: String,
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
    @ColumnInfo(name = "author_id") @SerialName("author_id") @JsonNames("authorId") val authorId: String,
    // EXP-741: the top-level comment this one replies to (one level deep);
    // null = a top-level card.
    @ColumnInfo(name = "parent_id") @SerialName("parent_id") @JsonNames("parentId") val parentId: String? = null,
    // EXP-741: `user` | `mcp` — an agent posted it over MCP ("via MCP").
    val source: String? = null,
    @Serializable(with = JsonAsStringSerializer::class) val body: String? = null,
    val kind: String = "regular",
    @ColumnInfo(name = "edited_at") @SerialName("edited_at") @JsonNames("editedAt") val editedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String,
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String,
)

enum class CommentKind { Regular }

/** EXP-741: an agent posted this comment over MCP (the "via MCP" caption). */
val CommentEntity.isViaMcp: Boolean
    get() = source == com.exponential.app.domain.DomainContract.commentSourceMcp

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
    // EXP-484: the coding agent this run launched with (contract `codingAgent`
    // values, a documented varchar server-side). NULL on every pre-EXP-484 row
    // and on any starter that omits it — the usage bar simply doesn't render.
    val agent: String? = null,
    // EXP-909: the agent ACCOUNT PROFILE this run spends — `system` = the
    // machine's ambient login, else a device-local profile id matching
    // `agentAccounts[agent].profiles[].id` on the devices row. NULL/absent =
    // UNKNOWN, which is NOT the ambient login: the usage overlay then falls
    // back to the machine's reported email, then its active profile.
    @ColumnInfo(name = "agent_account") @SerialName("agent_account") @JsonNames("agentAccount") val agentAccount: String? = null,
    // EXP-637: who ended the run — `agent` (sessions_end), `user`
    // (killSession), `client` (exit/tab close/quit), `merge` (a PR merge) or
    // `system` (the sweep). NULL on rows ended before the stamp existed.
    @ColumnInfo(name = "ended_by") @SerialName("ended_by") @JsonNames("endedBy") val endedBy: String? = null,
    // EXP-637: the ended run this one continues (Resume) — FK SET NULL. The
    // post-send start watch matches a resumed run by it (StartedRunMatch).
    @ColumnInfo(name = "resumed_from_id") @SerialName("resumed_from_id") @JsonNames("resumedFromId") val resumedFromId: String? = null,
    // EXP-818: the run that spawned this one through `exponential_sessions_start`
    // (FK SET NULL); null on a top-level run. The session lists nest a child
    // under its parent (SessionTree).
    @ColumnInfo(name = "parent_session_id") @SerialName("parent_session_id") @JsonNames("parentSessionId") val parentSessionId: String? = null,
    // Desktop-written attention flag (EXP-214): the agent is parked on a
    // plan-approval / AskUserQuestion picker and waits for a human.
    @ColumnInfo(name = "needs_input") @SerialName("needs_input") @JsonNames("needsInput") val needsInput: PgBool = false,
    // EXP-848: the agent is MID-TURN right now, written by the device on every
    // turn edge exactly like needsInput (and cleared by every server end path).
    // Orthogonal to `status`: a live run sits at false between turns, which is
    // why the session lists pulse on THIS and never on status = running.
    @ColumnInfo(name = "agent_busy") @SerialName("agent_busy") @JsonNames("agentBusy") val agentBusy: PgBool = false,
    // EXP-850 (S8): what the run is DOING right now in one line — today the
    // caption of the newest running workflow, written by the device (throttled,
    // on change) and cleared by every server end path. NULL = nothing to say,
    // which is every run that is not inside a workflow. Session list rows
    // render it as their SECOND line, before the device byline.
    @ColumnInfo(name = "agent_caption") @SerialName("agent_caption") @JsonNames("agentCaption") val agentCaption: String? = null,
    // EXP-905: the title the agent CLI (Claude Code / codex) auto-named the
    // run with, device-written like agentCaption. A CHAT run's subject
    // (`chatRunSubject`); NULL = the agent has not named it (yet).
    @ColumnInfo(name = "agent_title") @SerialName("agent_title") @JsonNames("agentTitle") val agentTitle: String? = null,
    // EXP-804: the agent's usage wall as row state, kept as the raw jsonb
    // TEXT off the wire (`{kind, agent, window, resetsAt, since}`) exactly
    // like DeviceEntity.agentUsage; NULL = not blocked. Orthogonal to
    // `status` the way needsInput is — a blocked run still reads `running`
    // and stays live and killable, so a client ignoring this shows a silently
    // walled run as healthy. Parsed for display by AgentUsagePresentation.
    @ColumnInfo(name = "blocked") @SerialName("blocked")
    @Serializable(with = JsonAsStringSerializer::class) val blocked: String? = null,
    // EXP-879: the run's published RESULTS — the screenshots the agent filed
    // with `exponential_sessions_results` while it worked, kept as the raw
    // jsonb TEXT off the wire exactly like `blocked`. A FLAT, ORDERED array of
    // `{topic, label, attachmentId, width, height}` capped at 60 server-side;
    // NULL or `[]` = nothing published, which is why the Results face is a
    // sub-face of Run rather than a permanent tab. Parsed for display by
    // SessionResults.
    @ColumnInfo(name = "results") @SerialName("results")
    @Serializable(with = JsonAsStringSerializer::class) val results: String? = null,
    // EXP-876: the issues a BATCH run covers, in the order the composer listed
    // them — the raw jsonb TEXT off the wire like `results`, and the only
    // thing that can NAME such a row (every batch used to read "Batch run").
    // NULL / `[]` on every other subject and on batch rows started by a client
    // too old to send it. Parsed for display by `batchRunIssueIds`.
    @ColumnInfo(name = "batch_issue_ids") @SerialName("batch_issue_ids")
    @Serializable(with = JsonAsStringSerializer::class) val batchIssueIds: String? = null,
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
    // EXP-734: the run's OWN pull request. Populated only when the PR links no
    // issue — an action or chat run (issue_id NULL) that opened one via MCP
    // `exponential_pr_open({repositoryId, head})`. Issue and batch runs keep
    // their PR on the issue row(s), so these stay NULL there and the merge
    // shortcut goes on resolving through the issue (MergeTarget). The server
    // flips pr_state to `merged` after a merge, so a client settles one by
    // watching this row rather than writing anything locally.
    @ColumnInfo(name = "pr_url") @SerialName("pr_url") @JsonNames("prUrl") val prUrl: String? = null,
    @ColumnInfo(name = "pr_number")
    @SerialName("pr_number")
    @JsonNames("prNumber")
    @Serializable(with = PgIntSerializer::class)
    val prNumber: Int? = null,
    @ColumnInfo(name = "pr_state") @SerialName("pr_state") @JsonNames("prState") val prState: String? = null,
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
    // EXP-825: the composer's field hint while this action is picked (≤200
    // chars, server-trimmed); null = the generic "Additional instructions
    // (optional)…" prompt. Absent on rows synced before the column existed.
    @ColumnInfo(name = "prompt_placeholder") @SerialName("prompt_placeholder") @JsonNames("promptPlaceholder") val promptPlaceholder: String? = null,
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

// One workflow (EXP-981, the 23rd Electric shape): a picked set of backlog
// issues of ONE repository, planned as a DAG and — from the engine on — run by
// the bound device. Team-scoped like `automations` (a workflow spans boards,
// so the board trash rules do not apply). The `blocks` relations among the
// covered issues are the EDGES and are never copied here.
//
// `launch` and `metrics` are jsonb kept as their raw JSON text and parsed
// tolerantly at the consumer ([WorkflowRows] — unknown keys ignored, missing
// keys default), the way every other jsonb column on this shape family is.
@Entity(
    tableName = "workflows",
    indices = [Index("team_id")],
)
@Serializable
data class WorkflowEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // SET NULL server-side: the row stays readable when the repo is unlinked.
    @ColumnInfo(name = "repository_id") @SerialName("repository_id") @JsonNames("repositoryId") val repositoryId: String? = null,
    val name: String = "",
    // contract `wfStatus` (documented varchar). An unknown value reads as Done
    // rather than vanishing (`WorkflowView.band`).
    val status: String = DomainContract.wfStatusDraft,
    // devices.device_id of the runner: the engine's SINGLE writer. NULL on a
    // draft nobody bound yet.
    @ColumnInfo(name = "device_id") @SerialName("device_id") @JsonNames("deviceId") val deviceId: String? = null,
    @Serializable(with = JsonAsStringSerializer::class) val launch: String? = null,
    // EXP-1010: a relic. The review gate setting is gone (every node gets an
    // agent review); the server still syncs the column for older engines and
    // it stays here only so the Room schema does not move. Nothing reads it.
    val gate: String = "agent",
    // contract `wfStartOn`.
    @ColumnInfo(name = "start_on") @SerialName("start_on") @JsonNames("startOn") val startOn: String = DomainContract.wfStartOnContract,
    @ColumnInfo(name = "integration_branch") @SerialName("integration_branch") @JsonNames("integrationBranch") val integrationBranch: String = "",
    @ColumnInfo(name = "final_pr_url") @SerialName("final_pr_url") @JsonNames("finalPrUrl") val finalPrUrl: String? = null,
    @ColumnInfo(name = "final_pr_number")
    @SerialName("final_pr_number")
    @JsonNames("finalPrNumber")
    @Serializable(with = PgIntSerializer::class)
    val finalPrNumber: Int? = null,
    @ColumnInfo(name = "final_pr_state") @SerialName("final_pr_state") @JsonNames("finalPrState") val finalPrState: String? = null,
    // Dated answers, appended; part of every node prompt (≤64KB).
    val decisions: String = "",
    @Serializable(with = JsonAsStringSerializer::class) val metrics: String? = null,
    @ColumnInfo(name = "started_at") @SerialName("started_at") @JsonNames("startedAt") val startedAt: String? = null,
    @ColumnInfo(name = "ended_at") @SerialName("ended_at") @JsonNames("endedAt") val endedAt: String? = null,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

// One NODE of a workflow (EXP-981, the 24th Electric shape): an issue, or a
// parent issue with its sub-issues (a compound node run as ONE batch on one
// branch with one PR). `team_id` is denormalized server-side for the shape's
// team scoping, exactly like the issue-child shapes.
//
// `wave` / `lane` / `on_cycle` ARE the server-computed layout — no client ever
// lays a workflow out; they draw column = wave, row = lane.
@Entity(
    tableName = "workflow_nodes",
    indices = [Index("workflow_id"), Index("team_id"), Index("issue_id")],
)
@Serializable
data class WorkflowNodeEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "workflow_id") @SerialName("workflow_id") @JsonNames("workflowId") val workflowId: String,
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String = "",
    // The node's representative issue (a compound node's PARENT).
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String,
    // A compound node's sub-issues (`EXP-14 +3`); empty for a plain node.
    @ColumnInfo(name = "member_issue_ids")
    @SerialName("member_issue_ids")
    @JsonNames("memberIssueIds")
    @Serializable(with = PgUuidArraySerializer::class)
    val memberIssueIds: List<String> = emptyList(),
    // contract `wfNodeKind` / `wfNodeState` / `wfRisk`.
    val kind: String = DomainContract.wfNodeKindLeaf,
    val state: String = DomainContract.wfNodeStateBlocked,
    val risk: String = DomainContract.wfRiskMedium,
    @Serializable(with = PgIntSerializer::class) val wave: Int? = 0,
    @Serializable(with = PgIntSerializer::class) val lane: Int? = 0,
    // On a blocking cycle (server layout): drawn red, and nothing can start.
    @ColumnInfo(name = "on_cycle") @SerialName("on_cycle") @JsonNames("onCycle") val onCycle: PgBool = false,
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String? = null,
    @Serializable(with = PgIntSerializer::class) val attempt: Int? = 0,
    @ColumnInfo(name = "base_branch") @SerialName("base_branch") @JsonNames("baseBranch") val baseBranch: String? = null,
    // EXP-982: the human (or agent) gate's stamp — a node with an open PR only
    // joins the merge train once this is set. NULL = still waiting on a person.
    @ColumnInfo(name = "approved_at") @SerialName("approved_at") @JsonNames("approvedAt") val approvedAt: String? = null,
    // EXP-983: when the node announced its contract. A dependent whose
    // workflow starts `on contract` waits for exactly this stamp; NULL = the
    // node has published nothing yet.
    @ColumnInfo(name = "checkpoint_at") @SerialName("checkpoint_at") @JsonNames("checkpointAt") val checkpointAt: String? = null,
    // EXP-983: the engine's SERIALIZATION edges — the nodes whose work this one
    // merges in first after two siblings collided. A jsonb string[] read as
    // tolerantly as every other id list (a native array, the Postgres literal
    // or the JSON text); anything else is EMPTY rather than a dropped row.
    @ColumnInfo(name = "after_node_ids")
    @SerialName("after_node_ids")
    @JsonNames("afterNodeIds")
    @Serializable(with = PgUuidArraySerializer::class)
    val afterNodeIds: List<String> = emptyList(),
    // EXP-984: the agent review gate. `review_round` counts the rounds this
    // node bounced back to its author (at most
    // `DomainContract.workflowMaxReviewRounds`, then it waits for a person);
    // `review` is the latest submitted verdict, a jsonb cell kept as its raw
    // text and read tolerantly (`workflowNodeReview`) like every other one.
    @ColumnInfo(name = "review_round")
    @SerialName("review_round")
    @JsonNames("reviewRound")
    @Serializable(with = PgIntSerializer::class)
    val reviewRound: Int? = 0,
    @Serializable(with = JsonAsStringSerializer::class) val review: String? = null,
    // EXP-982: why the node is `failed` / `waiting`, in the engine's own words.
    val note: String? = null,
    // A Postgres `text[]` of globs: what this node expects to change.
    @Serializable(with = PgUuidArraySerializer::class) val touches: List<String> = emptyList(),
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
    // EXP-824 inline media: probed playback length of a `video/*` / `audio/*`
    // row and the storage key of its poster frame (non-null = the poster route
    // `/api/attachments/{id}?poster=1` serves one). Both nullable with
    // defaults: a required field would stall the shape on a partial update.
    @ColumnInfo(name = "duration_ms") @SerialName("duration_ms") @JsonNames("durationMs") val durationMs: Long? = null,
    @ColumnInfo(name = "poster_storage_key") @SerialName("poster_storage_key") @JsonNames("posterStorageKey") val posterStorageKey: String? = null,
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
    // Set on issue-less support_reply / agent_message / session_blocked rows (the
    // team they belong to); NULL on issue-anchored rows.
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
    // EXP-924: the owner-picked display icon (contract `deviceIcon`, the
    // registry's `devicePickable` set). NULL — which is what every machine
    // registers with, and what a row synced from an older server carries — is
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
// "Pinned" sidebar rows — an issue, a coding session or an action, exactly one
// of the three ids set (`kind` names which). The shape is static per user
// (`user_id = me`), NOT team- or trash-scoped: the client renders only the pins
// of the active team whose TARGET resolves from the other synced tables, so a
// pin whose issue/session/action is not synced is simply hidden. Every field
// that can be absent is defaulted (the attachments.uploader_id lesson).
@Entity(
    tableName = "pins",
    indices = [Index("team_id"), Index("issue_id"), Index("session_id"), Index("action_id")],
)
@Serializable
data class PinEntity(
    @PrimaryKey val id: String,
    @ColumnInfo(name = "user_id") @SerialName("user_id") @JsonNames("userId") val userId: String = "",
    @ColumnInfo(name = "team_id") @SerialName("team_id") @JsonNames("teamId") val teamId: String,
    // Contract `pinKind`: issue | session | action.
    val kind: String,
    @ColumnInfo(name = "issue_id") @SerialName("issue_id") @JsonNames("issueId") val issueId: String? = null,
    @ColumnInfo(name = "session_id") @SerialName("session_id") @JsonNames("sessionId") val sessionId: String? = null,
    @ColumnInfo(name = "action_id") @SerialName("action_id") @JsonNames("actionId") val actionId: String? = null,
    // Ascending = display order (a Postgres double, appended one past the tail).
    @ColumnInfo(name = "sort_order") @SerialName("sort_order") @JsonNames("sortOrder") val sortOrder: Double = 0.0,
    @ColumnInfo(name = "created_at") @SerialName("created_at") @JsonNames("createdAt") val createdAt: String = "",
    @ColumnInfo(name = "updated_at") @SerialName("updated_at") @JsonNames("updatedAt") val updatedAt: String = "",
)

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
