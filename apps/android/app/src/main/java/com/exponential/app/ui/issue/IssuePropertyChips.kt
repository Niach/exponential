package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.Alignment
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.estimateShortLabel
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillMode
import androidx.compose.material3.Icon
import androidx.compose.foundation.layout.size
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.domain.AgentSettings
import com.exponential.app.ui.components.AgentAvatar
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassCard

/**
 * What the chip box shows besides status + labels — an issue's fields, or the
 * New issue page's unsent ones (EXP-1170).
 */
data class IssuePropertySubject(
    val priority: IssuePriority,
    val assigneeId: String?,
    val dueDate: String?,
    val estimate: Int?,
    /** #EXP-1383: a team AGENT as the assignee (exclusive with [assigneeId]). */
    val assigneeAgentId: String? = null,
) {
    companion object {
        fun fromIssue(issue: IssueEntity) = IssuePropertySubject(
            priority = IssuePriority.fromWire(issue.priority),
            assigneeId = issue.assigneeId,
            dueDate = issue.dueDate,
            estimate = issue.estimate,
            assigneeAgentId = issue.assigneeAgentId,
        )
    }
}

/**
 * The top property chip box (EXP-240) — one glass box of wrapping capsule
 * chips replacing the stacked property/times cards + labels section: Status,
 * Priority, Assignee (hidden on solo teams, EXP-50), Due date (only when set),
 * Estimate (EXP-630: only while the team's scale is not `none`; reads
 * "Estimate" until set) and ONE labels chip (EXP-1647: web `LabelPicker`'s
 * trigger — tag glyph, up to three colour dots, the names; "Label" when
 * none). Chip taps open the per-property sheets; the box background (FlowRow
 * gaps included) opens the combined Properties sheet. No "+" chip: web draws
 * none. Non-moderators see it dimmed and inert.
 *
 * EXP-1170: with no Properties sheet ([onOpenProperties] null, the New issue
 * page) there is no box tap; the empty Due date chip shows too, in the ×4
 * page order status · priority · assignee · labels · due date · board.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun IssuePropertyChips(
    subject: IssuePropertySubject,
    status: ResolvedIssueStatus,
    assignee: UserEntity?,
    issueLabels: List<LabelEntity>,
    isModerator: Boolean,
    hideAssignee: Boolean,
    /** The team's `estimation_type` (contract issueEstimation); `none`/null
     *  hides the estimate chip entirely (EXP-630). */
    estimationType: String?,
    onOpenStatus: () -> Unit,
    onOpenPriority: () -> Unit,
    onOpenAssignee: () -> Unit,
    onOpenDueDate: () -> Unit,
    onOpenEstimate: () -> Unit,
    onOpenLabels: () -> Unit,
    onOpenProperties: (() -> Unit)?,
    /** EXP-1170: the board chip, LAST in the box: its NAME (P46: on the face too). */
    board: String? = null,
    onOpenBoard: (() -> Unit)? = null,
    /** P44: the board itself, so the chip draws ITS glyph in ITS colour. */
    boardEntity: BoardEntity? = null,
    /** #EXP-1383: the resolved agent assignee: THE AGENT MARK + its name. */
    assigneeAgent: AgentSettings.AgentRowLike? = null,
) {
    val priority = subject.priority
    val pageMode = onOpenProperties == null
    val estimatesOn = estimationType != null && estimationType != DomainContract.issueEstimationNone
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier
            .fillMaxWidth()
            .glassCard()
            // Box-level clickable first, so the chips' own clickables win on
            // the chips and the gaps fall through to Properties.
            .then(
                if (isModerator && onOpenProperties != null) {
                    Modifier.clickable(onClick = onOpenProperties)
                } else {
                    Modifier
                },
            )
            // EXP-698: no outer .alpha() — every pill in here already dims
            // itself to quaternary when it is not enabled, and the two dims
            // stacked into an unreadable box for non-moderators.
            .padding(10.dp),
    ) {
        GlassPill(
            status.name,
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onOpenStatus,
            leading = { StatusIcon(status, size = GlassPillDefaults.SmGlyphSize) },
        )
        GlassPill(
            priority.label,
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onOpenPriority,
            leading = { PriorityIcon(priority, size = GlassPillDefaults.SmGlyphSize) },
        )
        if (!hideAssignee && assigneeAgent != null && subject.assigneeAgentId == assigneeAgent.id) {
            // #EXP-1383: an agent assignee reads like a member: mark + name.
            GlassPill(
                assigneeAgent.name,
                size = PillSize.Sm,
                enabled = isModerator,
                onClick = onOpenAssignee,
                leading = {
                    AgentAvatar(
                        icon = assigneeAgent.icon,
                        color = assigneeAgent.color,
                        size = GlassPillDefaults.AvatarSize,
                    )
                },
            )
        } else if (!hideAssignee) {
            val assigneeName = subject.assigneeId?.let { userDisplayName(assignee, it) }
            GlassPill(
                assigneeName ?: IssuePropertyChipCopy.EMPTY_ASSIGNEE,
                size = PillSize.Sm,
                enabled = isModerator,
                onClick = onOpenAssignee,
                leading = if (assigneeName != null) {
                    {
                        // An avatar is a face, not a glyph: at the 12dp glyph
                        // rung its initials fell to ~5sp (EXP-698).
                        UserAvatar(
                            user = assignee,
                            nameOrEmail = assigneeName,
                            size = GlassPillDefaults.AvatarSize,
                            userId = subject.assigneeId,
                        )
                    }
                } else {
                    null
                },
                icon = if (assigneeName == null) ExpIcons.uiUnassigned else null,
            )
        }
        // P46: ONE order ×4 — status · priority · assignee · labels · due
        // date · board (the estimate, where the team keeps one, after the
        // due date). The face shows no empty Due date chip; the labels chip
        // is always there (web's is).
        LabelsChip(issueLabels, isModerator, onOpenLabels)
        DueDateChip(subject.dueDate, isModerator, onOpenDueDate, showEmpty = pageMode)
        if (!pageMode && estimatesOn) {
            GlassPill(
                subject.estimate?.let { estimateShortLabel(it, estimationType!!) } ?: "Estimate",
                size = PillSize.Sm,
                enabled = isModerator,
                onClick = onOpenEstimate,
                icon = ExpIcons.uiEstimate,
                maxLines = 1,
            )
        }
        if (board != null) {
            // P44: the board's own glyph in its colour + its NAME, no chevron.
            GlassPill(
                board,
                size = PillSize.Sm,
                enabled = onOpenBoard != null,
                onClick = onOpenBoard,
                mode = if (onOpenBoard != null) PillMode.Action else PillMode.Readonly,
                leading = boardEntity?.let { b -> { BoardIcon(b, size = GlassPillDefaults.SmGlyphSize) } },
                icon = if (boardEntity == null) ExpIcons.navBoards else null,
                maxLines = 1,
                modifier = Modifier.testTag("issue-board-chip"),
            )
        }
    }
}

@Composable
private fun DueDateChip(dueDate: String?, isModerator: Boolean, onClick: () -> Unit, showEmpty: Boolean) {
    if (dueDate != null) {
        // P48: the urgency tints the GLYPH only ×4; the label keeps the
        // pill's own colour, so a not-yet-due date never reads as disabled.
        val tint = dueDateColor(dueDate)
        GlassPill(
            formatDueDate(dueDate),
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onClick,
            leading = {
                Icon(
                    ExpIcons.uiDueDate,
                    contentDescription = null,
                    tint = tint,
                    modifier = Modifier.size(GlassPillDefaults.SmGlyphSize),
                )
            },
            maxLines = 1,
        )
    } else if (showEmpty) {
        GlassPill(
            "Due date",
            size = PillSize.Sm,
            enabled = isModerator,
            onClick = onClick,
            icon = ExpIcons.uiDueDate,
            maxLines = 1,
        )
    }
}

/**
 * EXP-1647: THE labels chip — web `LabelPicker`'s trigger ×4: the tag glyph,
 * then (when labelled) up to three overlapping 8dp colour dots ringed in the
 * page background and the names joined ", " (one line, ellipsized); an
 * unlabelled issue reads tag + "Label". One chip, one tap: the label sheet.
 */
@Composable
private fun LabelsChip(labels: List<LabelEntity>, isModerator: Boolean, onClick: () -> Unit) {
    val fg = MaterialTheme.colorScheme.onSurface.copy(
        alpha = if (isModerator) TextEmphasis.Secondary else TextEmphasis.Quaternary,
    )
    GlassPill(
        if (labels.isEmpty()) IssuePropertyChipCopy.EMPTY_LABEL else labels.joinToString(", ") { it.name },
        size = PillSize.Sm,
        enabled = isModerator,
        onClick = onClick,
        maxLines = 1,
        shrinkLabel = labels.isNotEmpty(),
        modifier = Modifier.testTag("issue-labels-chip"),
        leading = {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(GlassPillDefaults.SmSpacing),
            ) {
                Icon(
                    ExpIcons.settingsLabels,
                    contentDescription = null,
                    tint = fg,
                    modifier = Modifier.size(GlassPillDefaults.SmGlyphSize),
                )
                if (labels.isNotEmpty()) {
                    // web `-space-x-0.5`: each dot overlaps the previous by 2dp.
                    Row(horizontalArrangement = Arrangement.spacedBy((-2).dp)) {
                        labels.take(3).forEach { label ->
                            Box(
                                Modifier
                                    .size(LabelDotSize)
                                    .background(DesignTokens.Palette.Background, CircleShape)
                                    .padding(1.dp)
                                    .background(parseColor(label.color), CircleShape),
                            )
                        }
                    }
                }
            }
        },
    )
}

/** web `h-2 w-2` + its `ring-1`: the 1dp ring is drawn inside these 10dp. */
private val LabelDotSize = 10.dp

/** The empty chips' copy ×4 (P45, web `assignee-picker` / `label-picker`). */
object IssuePropertyChipCopy {
    const val EMPTY_ASSIGNEE = "Assignee"
    const val EMPTY_LABEL = "Label"
}
