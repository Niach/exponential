package com.exponential.app.ui.session

import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.ActionEntity
import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.EntityPreview
import com.exponential.app.domain.EntityRef
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SessionDotTone
import com.exponential.app.domain.pastRunTitle
import com.exponential.app.domain.triggerSummary
import com.exponential.app.navigation.EntityTarget
import com.exponential.app.navigation.LocalEntityNavigator
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.EntityChip
import com.exponential.app.ui.components.EntityChipGlyph
import com.exponential.app.ui.components.EntityPreviewCard
import com.exponential.app.ui.components.EntityPreviewRow
import com.exponential.app.ui.components.EntityPreviewSheet
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.LabelChip
import com.exponential.app.ui.components.LabelDot
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.components.entityConceptIcon
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.work.SessionToneDot
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray

/**
 * EXP-920: what a preview sheet resolves its ref against — the account DB the
 * run's screen already queries, the run's team (a ref names a row of THAT
 * team) and the viewer (notifications are per user). Provided by [RunFace];
 * absent = every sheet is the slim card.
 */
class EntityRefResolver(
    val db: Flow<ExponentialDatabase?>,
    val teamId: String?,
    val userId: String?,
)

val LocalEntityRefResolver = staticCompositionLocalOf<EntityRefResolver?> { null }

/**
 * EXP-920: the wrapping row of entity chips under a settled Exponential tool
 * row — one chip per [EntityPreview.groupRefs] group (a `list` ref and the
 * members behind it are ONE chip), each opening [EntityRefPreviewSheet].
 * An issue chip paints its resolved status glyph (the `#IDENTIFIER` chip
 * in the line above does the same); every other kind its concept glyph.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun EntityRefChips(refs: List<EntityRef>, modifier: Modifier = Modifier) {
    val groups = remember(refs) { EntityPreview.groupRefs(refs) }
    var open by remember { mutableStateOf<EntityPreview.Group?>(null) }
    FlowRow(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        groups.forEach { group ->
            val ref = group.ref
            EntityChip(
                icon = { EntityRefGlyph(ref) },
                label = EntityPreview.chipLabel(ref),
                detail = EntityPreview.chipDetail(ref),
                monoLabel = ref.kind == "issue" && !ref.identifier.isNullOrBlank(),
                onClick = { open = group },
            )
        }
    }
    open?.let { group ->
        EntityRefPreviewSheet(ref = group.ref, members = group.members, onDismiss = { open = null })
    }
}

/** A chip's glyph: an issue's synced status glyph, else the kind's concept. */
@Composable
private fun EntityRefGlyph(ref: EntityRef) {
    if (ref.kind == "issue") {
        val status = ref.identifier?.let { LocalIssueRefs.current?.resolve(it) }?.resolvedStatus
        if (status != null) {
            StatusIcon(status, size = MdStyle.chipIconSize)
            return
        }
    }
    EntityChipGlyph(conceptGlyph(EntityPreview.refIcon(ref)))
}

private fun conceptGlyph(concept: String): ImageVector =
    entityConceptIcon(concept) ?: ExpIcons.uiChecklist

// ── The sheet ──────────────────────────────────────────────────────────────

/**
 * The phone's hover card: [ref] resolved from the synced rows into an
 * [EntityPreviewCard] inside a [GlassSheet]. A ref this client has not synced
 * (another team's, a trashed board, an unsynced kind) still renders what the
 * answer said — the slim card: eyebrow, label, and Open where the target
 * needs no row.
 */
@Composable
fun EntityRefPreviewSheet(
    ref: EntityRef,
    members: List<EntityRef>,
    onDismiss: () -> Unit,
) {
    val navigator = LocalEntityNavigator.current
    val open: (EntityTarget) -> Unit = { target ->
        onDismiss()
        navigator.open(target)
    }
    val title = when {
        ref.kind == "list" -> EntityPreview.chipLabel(ref)
        ref.kind == "issue" && !ref.identifier.isNullOrBlank() -> ref.identifier.orEmpty()
        else -> capitalizedNoun(ref.kind)
    }
    EntityPreviewSheet(title = title, onDismiss = onDismiss) {
        when (ref.kind) {
            "issue" -> IssueCard(ref, open)
            "board" -> BoardCard(ref, open)
            "action" -> ActionCard(ref, open)
            "automation" -> AutomationCard(ref, open)
            "comment" -> CommentCard(ref, open)
            "session" -> SessionCard(ref, open)
            "label" -> LabelCard(ref, open)
            "status" -> StatusCard(ref, open)
            "workflow" -> WorkflowCard(ref, open)
            "device" -> DeviceCard(ref, open)
            "member" -> MemberCard(ref, open)
            "team" -> TeamCard(ref, open)
            "invite" -> InviteCard(ref, open)
            "notification" -> NotificationCard(ref, open)
            "attachment" -> AttachmentCard(ref, open)
            "thread" -> SlimCard(ref, onOpen = { open(EntityTarget.SupportThread(ref.id)) })
            "repository" -> SlimCard(ref, onOpen = { open(EntityTarget.TeamSettings) })
            "list" -> ListCard(ref, members, open)
            else -> SlimCard(ref, onOpen = null)
        }
    }
}

private fun capitalizedNoun(kind: String): String =
    EntityPreview.kindNoun(kind, 1).replaceFirstChar { it.uppercase() }

/** What the ref itself says — the fallback for anything unresolved. */
@Composable
private fun SlimCard(ref: EntityRef, onOpen: (() -> Unit)?, subtitle: String? = null) {
    EntityPreviewCard(
        icon = { EntityChipGlyph(conceptGlyph(EntityPreview.refIcon(ref)), tint = MaterialTheme.colorScheme.onSurface) },
        eyebrow = capitalizedNoun(ref.kind),
        title = ref.title?.takeIf { it.isNotBlank() } ?: EntityPreview.chipLabel(ref),
        subtitle = subtitle,
        onOpen = onOpen,
    )
}

/** A concept glyph at the card header's size, in the body colour. */
@Composable
private fun HeaderGlyph(icon: ImageVector) {
    Icon(
        icon,
        contentDescription = null,
        modifier = Modifier.size(20.dp),
        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
    )
}

/**
 * Observe one account-scoped query, re-subscribing when [key] changes; [empty]
 * while no account is active or before the first row lands.
 */
@Composable
private fun <T> observeScoped(
    key: Any?,
    empty: T,
    query: (ExponentialDatabase) -> Flow<T>,
): State<T> {
    val resolver = LocalEntityRefResolver.current
    val flow = remember(resolver, key) {
        resolver?.db?.scopedQuery(empty, query) ?: flowOf(empty)
    }
    return flow.collectAsState(empty)
}

@Composable
private fun teamStatuses(): List<ResolvedIssueStatus> {
    val teamId = LocalEntityRefResolver.current?.teamId
    val rows by observeScoped(teamId, emptyList<IssueStatusEntity>()) { db ->
        if (teamId == null) flowOf(emptyList()) else db.issueStatusDao().observeByTeam(teamId)
    }
    return remember(rows) {
        if (rows.isEmpty()) IssueStatusResolver.builtinDefaults else IssueStatusResolver.teamStatuses(rows)
    }
}

@Composable
private fun teamLabels(): List<LabelEntity> {
    val teamId = LocalEntityRefResolver.current?.teamId
    val rows by observeScoped(teamId, emptyList<LabelEntity>()) { db ->
        if (teamId == null) flowOf(emptyList()) else db.labelDao().observeByTeam(teamId)
    }
    return rows
}

@Composable
private fun teamBoards(): List<BoardEntity> {
    val teamId = LocalEntityRefResolver.current?.teamId
    val rows by observeScoped(teamId, emptyList<BoardEntity>()) { db ->
        if (teamId == null) flowOf(emptyList()) else db.boardDao().observeByTeam(teamId)
    }
    return rows
}

@Composable
private fun teamActions(): List<ActionEntity> {
    val teamId = LocalEntityRefResolver.current?.teamId
    val rows by observeScoped(teamId, emptyList<ActionEntity>()) { db ->
        if (teamId == null) flowOf(emptyList()) else db.actionDao().observeByTeam(teamId)
    }
    return rows
}

// ── Per kind ───────────────────────────────────────────────────────────────

@Composable
private fun IssueCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val issue by observeScoped(ref.id, null) { it.issueDao().observeById(ref.id) }
    val row = issue ?: run {
        SlimCard(ref, subtitle = ref.title?.takeIf { !ref.identifier.isNullOrBlank() }, onOpen = { open(EntityTarget.Issue(ref.id)) })
        return
    }
    val statuses = teamStatuses()
    val labels = teamLabels()
    val joins by observeScoped(row.id, emptyList()) { it.issueLabelDao().observeByIssue(row.id) }
    val status = remember(row, statuses) { IssueStatusResolver.resolve(row, statuses) }
    val priority = remember(row.priority) { IssuePriority.fromWire(row.priority) }
    val issueLabels = remember(joins, labels) {
        val ids = joins.map { it.labelId }.toSet()
        labels.filter { it.id in ids }
    }
    EntityPreviewCard(
        icon = { StatusIcon(status, size = 20.dp) },
        eyebrow = row.identifier,
        title = row.title.ifBlank { "Untitled issue" },
        body = remember(row.description) { row.description?.let { plainExcerpt(it, EXCERPT_MAX) } },
        facts = {
            GlassPill(status.name, size = PillSize.Sm, mode = PillMode.Readonly, leading = { StatusIcon(status, size = 12.dp) })
            if (priority != IssuePriority.None) {
                GlassPill(priority.label, size = PillSize.Sm, mode = PillMode.Readonly, leading = { PriorityIcon(priority, size = 12.dp) })
            }
            issueLabels.forEach { LabelChip(it) }
        },
        onOpen = { open(EntityTarget.Issue(row.id)) },
    )
}

@Composable
private fun BoardCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val boards = teamBoards()
    val board = boards.firstOrNull { it.id == ref.id } ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.Board(ref.id)) })
        return
    }
    val statuses = teamStatuses()
    val issues by observeScoped(board.id, emptyList<IssueEntity>()) { it.issueDao().observeByBoard(board.id) }
    val openIssues = remember(issues) {
        issues.filter { IssueStatus.fromWire(it.status) !in CLOSED_STATUSES }
    }
    val shown = openIssues.take(BOARD_PREVIEW_ROWS)
    EntityPreviewCard(
        icon = { BoardIcon(board, size = 20.dp) },
        eyebrow = "Board",
        title = board.name,
        subtitle = "${openIssues.size} open " + if (openIssues.size == 1) "issue" else "issues",
        rows = shown.map { issue ->
            val status = IssueStatusResolver.resolve(issue, statuses)
            EntityPreviewRow(
                icon = { StatusIcon(status, size = 14.dp) },
                primary = issue.identifier,
                secondary = issue.title,
                monoPrimary = true,
                onClick = { open(EntityTarget.Issue(issue.id)) },
            )
        },
        more = (openIssues.size - shown.size).takeIf { it > 0 }?.let { "+$it more" },
        onOpen = { open(EntityTarget.Board(board.id)) },
    )
}

@Composable
private fun ActionCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val action = teamActions().firstOrNull { it.id == ref.id } ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.Actions) })
        return
    }
    val inputCount = remember(action.inputs) { jsonArrayLength(action.inputs) }
    EntityPreviewCard(
        icon = { HeaderGlyph(action.icon?.let { ExpIcons.byName(it) } ?: ExpIcons.actionDefault) },
        eyebrow = "Action",
        title = action.name,
        body = action.description?.takeIf { it.isNotBlank() },
        facts = {
            GlassPill("$inputCount " + if (inputCount == 1) "input" else "inputs", size = PillSize.Sm, mode = PillMode.Readonly)
            if (action.repositoryId != null) {
                GlassPill("Repository-bound", size = PillSize.Sm, mode = PillMode.Readonly, icon = ExpIcons.uiRepository)
            }
        },
        onOpen = { open(EntityTarget.Actions) },
    )
}

@Composable
private fun AutomationCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val teamId = LocalEntityRefResolver.current?.teamId
    val automations by observeScoped(teamId, emptyList<AutomationEntity>()) { db ->
        if (teamId == null) flowOf(emptyList()) else db.automationDao().observeByTeam(teamId)
    }
    val automation = automations.firstOrNull { it.id == ref.id } ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.Automations) })
        return
    }
    val action = teamActions().firstOrNull { it.id == automation.actionId }
    val devices by observeScoped(Unit, emptyList()) { it.deviceDao().observeAll() }
    val device = devices.firstOrNull { it.deviceId == automation.deviceId || it.id == automation.deviceId }
    val trigger = remember(automation.trigger) { AutomationTrigger.parse(automation.trigger)?.let(::triggerSummary) }
    EntityPreviewCard(
        icon = { HeaderGlyph(ExpIcons.navAutomations) },
        eyebrow = "Automation",
        title = ref.title?.takeIf { it.isNotBlank() } ?: action?.name ?: "Automation",
        subtitle = action?.let { "Runs ${it.name}" },
        facts = {
            if (trigger != null) GlassPill(trigger, size = PillSize.Sm, mode = PillMode.Readonly, icon = ExpIcons.navAutomations)
            if (device != null) {
                GlassPill(device.label.ifBlank { "Device" }, size = PillSize.Sm, mode = PillMode.Readonly, icon = deviceIcon(device.icon, device.kind == SteerDevice.KIND_SERVER))
            }
            GlassPill(if (automation.enabled) "Enabled" else "Disabled", size = PillSize.Sm, mode = PillMode.Readonly)
        },
        onOpen = { open(EntityTarget.Automations) },
    )
}

@Composable
private fun CommentCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val comment by observeScoped(ref.id, null) { it.commentDao().observeById(ref.id) }
    val row: CommentEntity = comment ?: run {
        SlimCard(ref, onOpen = null)
        return
    }
    val author by observeScoped(row.authorId, null) { it.userDao().observeById(row.authorId) }
    val issue by observeScoped(row.issueId, null) { it.issueDao().observeById(row.issueId) }
    val statuses = teamStatuses()
    val name = userDisplayName(author, row.authorId)
    EntityPreviewCard(
        icon = { UserAvatar(author, name, size = 24.dp, userId = row.authorId) },
        eyebrow = "Comment",
        title = name,
        subtitle = relativeTime(row.createdAt) + if (row.source == DomainContract.commentSourceMcp) " · via MCP" else "",
        body = remember(row.body) { row.body?.let { plainExcerpt(it, EXCERPT_MAX) } },
        rows = listOfNotNull(
            issue?.let { target ->
                val status = IssueStatusResolver.resolve(target, statuses)
                EntityPreviewRow(
                    icon = { StatusIcon(status, size = 14.dp) },
                    primary = target.identifier,
                    secondary = target.title,
                    monoPrimary = true,
                    onClick = { open(EntityTarget.Issue(target.id)) },
                )
            },
        ),
        onOpen = { open(EntityTarget.Issue(row.issueId)) },
    )
}

@Composable
private fun SessionCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val session by observeScoped(ref.id, null) { it.codingSessionDao().observeById(ref.id) }
    val row: CodingSessionEntity = session ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.Session(ref.id)) })
        return
    }
    val issue by observeScoped(row.issueId, null) { db ->
        row.issueId?.let { db.issueDao().observeById(it) } ?: flowOf(null)
    }
    // A batch run names itself from its covered issues (`EXP-874 +2`).
    val issues by observeScoped(row.batchIssueIds, emptyList<IssueEntity>()) { db ->
        if (row.batchIssueIds.isNullOrBlank()) flowOf(emptyList()) else db.issueDao().observeAll()
    }
    val nowMs = System.currentTimeMillis()
    val tone = sessionDotTone(row, issue?.prState, nowMs, awaitingInput = row.needsInput) ?: SessionDotTone.Muted
    val stateLabel = when (row.status) {
        DomainContract.codingSessionStatusEnded -> "Ended"
        DomainContract.codingSessionStatusInReview -> "In review"
        else -> if (row.needsInput) "Needs input" else "Running"
    }
    EntityPreviewCard(
        icon = { SessionToneDot(tone, busy = row.agentBusy) },
        eyebrow = "Run",
        title = pastRunTitle(row, issue, issues),
        subtitle = issue?.identifier,
        facts = {
            GlassPill(stateLabel, size = PillSize.Sm, mode = PillMode.Readonly)
            row.deviceLabel?.takeIf { it.isNotBlank() }?.let {
                GlassPill(it, size = PillSize.Sm, mode = PillMode.Readonly, icon = ExpIcons.uiDevice)
            }
            GlassPill("Started ${relativeTime(row.startedAt, nowMs)}", size = PillSize.Sm, mode = PillMode.Readonly)
        },
        onOpen = { open(EntityTarget.Session(row.id)) },
    )
}

@Composable
private fun LabelCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val label = teamLabels().firstOrNull { it.id == ref.id } ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.TeamSettings) })
        return
    }
    val color = remember(label.color) { parseColor(label.color) }
    EntityPreviewCard(
        icon = { LabelDot(color, size = 12.dp) },
        eyebrow = "Label",
        title = label.name,
        onOpen = { open(EntityTarget.TeamSettings) },
    )
}

@Composable
private fun StatusCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val status = teamStatuses().firstOrNull { it.rowId == ref.id } ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.TeamSettings) })
        return
    }
    EntityPreviewCard(
        icon = { StatusIcon(status, size = 20.dp) },
        eyebrow = "Status",
        title = status.name,
        subtitle = status.category.wire.replaceFirstChar { it.uppercase() },
        onOpen = { open(EntityTarget.TeamSettings) },
    )
}

@Composable
private fun WorkflowCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val workflow by observeScoped(ref.id, null) { it.workflowDao().observeById(ref.id) }
    val row = workflow ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.Workflow(ref.id)) })
        return
    }
    val nodes by observeScoped(row.id, emptyList()) { it.workflowNodeDao().observeByWorkflow(row.id) }
    EntityPreviewCard(
        icon = { HeaderGlyph(ExpIcons.navWorkflows) },
        eyebrow = "Workflow",
        title = row.name.ifBlank { ref.title?.takeIf { it.isNotBlank() } ?: "Workflow" },
        facts = {
            GlassPill(row.status.replaceFirstChar { it.uppercase() }, size = PillSize.Sm, mode = PillMode.Readonly)
            GlassPill("${nodes.size} " + if (nodes.size == 1) "node" else "nodes", size = PillSize.Sm, mode = PillMode.Readonly)
        },
        onOpen = { open(EntityTarget.Workflow(row.id)) },
    )
}

@Composable
private fun DeviceCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val devices by observeScoped(Unit, emptyList()) { it.deviceDao().observeAll() }
    val device = devices.firstOrNull { it.id == ref.id || it.deviceId == ref.id } ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.Devices) })
        return
    }
    val isServer = device.kind == SteerDevice.KIND_SERVER
    val online = DeviceLiveness.isOnline(device.lastSeenAt)
    EntityPreviewCard(
        icon = { HeaderGlyph(deviceIcon(device.icon, isServer)) },
        eyebrow = "Device",
        title = device.label.ifBlank { ref.title?.takeIf { it.isNotBlank() } ?: "Device" },
        facts = {
            GlassPill(
                if (online) "Online" else "Offline",
                size = PillSize.Sm,
                mode = PillMode.Readonly,
                dot = if (online) OnlineGreen else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
            )
            device.platform?.takeIf { it.isNotBlank() }?.let { GlassPill(it, size = PillSize.Sm, mode = PillMode.Readonly) }
        },
        onOpen = { open(EntityTarget.Devices) },
    )
}

@Composable
private fun MemberCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val teamId = LocalEntityRefResolver.current?.teamId
    val user by observeScoped(ref.id, null) { it.userDao().observeById(ref.id) }
    val members by observeScoped(teamId, emptyList()) { db ->
        if (teamId == null) flowOf(emptyList()) else db.teamMemberDao().observeByTeam(teamId)
    }
    val row = user ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.TeamSettings) })
        return
    }
    val role = members.firstOrNull { it.userId == row.id }?.role
    val name = userDisplayName(row, row.id)
    EntityPreviewCard(
        icon = { UserAvatar(row, name, size = 24.dp) },
        eyebrow = "Member",
        title = name,
        subtitle = row.email,
        facts = if (role == null) {
            null
        } else {
            { GlassPill(role.replaceFirstChar { c -> c.uppercase() }, size = PillSize.Sm, mode = PillMode.Readonly) }
        },
        onOpen = { open(EntityTarget.TeamSettings) },
    )
}

@Composable
private fun TeamCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val team by observeScoped(ref.id, null) { it.teamDao().observeById(ref.id) }
    val row = team ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.TeamSettings) })
        return
    }
    EntityPreviewCard(
        icon = { TeamAvatar(row, size = 24.dp) },
        eyebrow = "Team",
        title = row.name,
        onOpen = { open(EntityTarget.TeamSettings) },
    )
}

@Composable
private fun InviteCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val teamId = LocalEntityRefResolver.current?.teamId
    val invites by observeScoped(teamId, emptyList()) { db ->
        if (teamId == null) flowOf(emptyList()) else db.teamInviteDao().observeByTeam(teamId)
    }
    val invite = invites.firstOrNull { it.id == ref.id } ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.TeamSettings) })
        return
    }
    EntityPreviewCard(
        icon = { HeaderGlyph(ExpIcons.uiInvite) },
        eyebrow = "Invite",
        title = invite.email?.takeIf { it.isNotBlank() } ?: "Invite link",
        facts = { GlassPill(invite.role.replaceFirstChar { it.uppercase() }, size = PillSize.Sm, mode = PillMode.Readonly) },
        onOpen = { open(EntityTarget.TeamSettings) },
    )
}

@Composable
private fun NotificationCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val userId = LocalEntityRefResolver.current?.userId
    val notifications by observeScoped(userId, emptyList()) { db ->
        if (userId == null) flowOf(emptyList()) else db.notificationDao().observeByUser(userId)
    }
    val row = notifications.firstOrNull { it.id == ref.id } ?: run {
        SlimCard(ref, onOpen = { open(EntityTarget.Inbox) })
        return
    }
    EntityPreviewCard(
        icon = { HeaderGlyph(ExpIcons.navNotifications) },
        eyebrow = "Notification",
        title = row.title,
        subtitle = relativeTime(row.createdAt),
        body = row.body?.takeIf { it.isNotBlank() }?.let { plainExcerpt(it, EXCERPT_MAX) },
        onOpen = { open(row.issueId?.let { EntityTarget.Issue(it) } ?: EntityTarget.Inbox) },
    )
}

@Composable
private fun AttachmentCard(ref: EntityRef, open: (EntityTarget) -> Unit) {
    val attachment by observeScoped(ref.id, null) { it.attachmentDao().observeById(ref.id) }
    val row = attachment ?: run {
        SlimCard(ref, onOpen = null)
        return
    }
    val context = LocalContext.current
    EntityPreviewCard(
        icon = { HeaderGlyph(ExpIcons.uiAttach) },
        eyebrow = "Attachment",
        title = row.filename,
        subtitle = Formatter.formatShortFileSize(context, row.sizeBytes) + " · " + row.contentType,
        onOpen = { open(EntityTarget.Issue(row.issueId)) },
    )
}

/**
 * A `list` chip's card: the chip label as the header, one row per member
 * ref (an issue row paints its synced status glyph; every row taps through
 * to its own target), and "+N more" when the answer counted more than it
 * named. A list has no Open of its own.
 */
@Composable
private fun ListCard(ref: EntityRef, members: List<EntityRef>, open: (EntityTarget) -> Unit) {
    val issueRefs = LocalIssueRefs.current
    val count = maxOf(0, ref.count ?: 0)
    EntityPreviewCard(
        icon = { HeaderGlyph(conceptGlyph(EntityPreview.refIcon(ref))) },
        eyebrow = "List",
        title = EntityPreview.chipLabel(ref),
        rows = members.map { member ->
            val isIssue = member.kind == "issue" && !member.identifier.isNullOrBlank()
            val target = if (isIssue) member.identifier?.let { issueRefs?.resolve(it) } else null
            EntityPreviewRow(
                icon = {
                    val status = target?.resolvedStatus
                    if (status != null) {
                        StatusIcon(status, size = 14.dp)
                    } else {
                        EntityChipGlyph(conceptGlyph(EntityPreview.refIcon(member)))
                    }
                },
                primary = EntityPreview.chipLabel(member),
                secondary = EntityPreview.chipDetail(member) ?: target?.title?.takeIf { !isIssue },
                monoPrimary = isIssue,
                onClick = memberTarget(member)?.let { t -> { open(t) } },
            )
        },
        more = (count - members.size).takeIf { it > 0 && members.isNotEmpty() }?.let { "+$it more" },
        onOpen = null,
    )
}

/** Where a list member's row lands — the same mapping the Open rows use,
 *  minus the kinds that need a synced row to know their issue. */
private fun memberTarget(ref: EntityRef): EntityTarget? = when (ref.kind) {
    "issue" -> EntityTarget.Issue(ref.id)
    "board" -> EntityTarget.Board(ref.id)
    "session" -> EntityTarget.Session(ref.id)
    "workflow" -> EntityTarget.Workflow(ref.id)
    "thread" -> EntityTarget.SupportThread(ref.id)
    "action" -> EntityTarget.Actions
    "automation" -> EntityTarget.Automations
    "device" -> EntityTarget.Devices
    "label", "status", "member", "invite", "team", "repository" -> EntityTarget.TeamSettings
    "notification" -> EntityTarget.Inbox
    else -> null
}

// ── Helpers ────────────────────────────────────────────────────────────────

private const val EXCERPT_MAX = 240
private const val BOARD_PREVIEW_ROWS = 6
private val CLOSED_STATUSES = setOf(IssueStatus.Done, IssueStatus.Cancelled, IssueStatus.Duplicate)
private val OnlineGreen = androidx.compose.ui.graphics.Color(0xFF34D399)

/** How many entries a JSON array string holds; 0 for anything else. */
private fun jsonArrayLength(raw: String?): Int {
    val text = raw?.trim().orEmpty()
    if (!text.startsWith("[")) return 0
    return runCatching { (Json.parseToJsonElement(text) as? JsonArray)?.size ?: 0 }.getOrDefault(0)
}

private val MD_IMAGE = Regex("""!\[[^\]]*]\([^)]*\)""")
private val MD_LINK = Regex("""\[([^\]]*)]\([^)]*\)""")
private val MD_CODE_FENCE = Regex("""```[\s\S]*?```""")
private val MD_MARKS = Regex("""(^|\n)\s{0,3}(#{1,6}\s+|>\s?|[-*+]\s+|\d+\.\s+|\|)""")
private val MD_INLINE = Regex("""[*_`~]+""")
private val WHITESPACE = Regex("""\s+""")

/**
 * A GFM body as ONE line of plain text for a card excerpt: images dropped,
 * links reduced to their text, fences dropped, block and inline marks
 * stripped, whitespace collapsed, cut at [max] code points with an ellipsis.
 */
internal fun plainExcerpt(markdown: String, max: Int): String {
    val text = markdown
        .replace(MD_CODE_FENCE, " ")
        .replace(MD_IMAGE, " ")
        .replace(MD_LINK) { it.groupValues[1] }
        .replace(MD_MARKS) { it.groupValues[1] }
        .replace(MD_INLINE, "")
        .replace(WHITESPACE, " ")
        .trim()
    val points = text.codePointCount(0, text.length)
    if (points <= max) return text
    return text.substring(0, text.offsetByCodePoints(0, max - 1)).trimEnd() + "…"
}
