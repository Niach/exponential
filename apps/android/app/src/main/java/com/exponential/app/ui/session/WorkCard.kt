package com.exponential.app.ui.session

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.WorkCard
import com.exponential.app.domain.WorkCardFoldedPhase
import com.exponential.app.domain.WorkCardKind
import com.exponential.app.domain.WorkCardLane
import com.exponential.app.domain.WorkCardLaneTarget
import com.exponential.app.domain.WorkCardRow
import com.exponential.app.domain.WorkCardSpec
import com.exponential.app.domain.WorkState
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

// EXP-1240 (EXP-1259 D12): THE Work card, ONE card for a claude workflow, a
// task list and a subagent, replacing the workflow card, the task list block
// and the subagent group rows. The model is `domain/WorkCard.kt` (fixture
// `work-card.json` ×4); this only paints it: a header (kind icon, name, kind
// word, progress · elapsed), a segment bar (phases or tasks), then
// hairline-divided rows. No report text, no token counts, no JSON. A lane row
// (and a subagent card) opens that lane's transcript through the host's
// callbacks; a task row does nothing. Web `@exp/ui` `WorkCard`.

/** The run thread's card shell (web `rounded-lg border-glass-stroke-card
 *  bg-glass-card-opaque`), shared by the Work, Script, Comment, session and
 *  suggestions cards. N-12: ONE gray ×4, the opaque card composite. */
internal fun Modifier.threadCard(): Modifier {
    val shape = RoundedCornerShape(DesignTokens.Radius.Lg)
    return this
        .clip(shape)
        .background(GlassTokens.OpaqueCardFill, shape)
        .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
}

/** The transcript's tool rung (the cards read one step below prose). */
@Composable
internal fun threadCardTextStyle(): TextStyle = MaterialTheme.typography.labelMedium.copy(
    fontSize = DesignTokens.Transcript.ToolSize.sp,
    lineHeight = DesignTokens.Transcript.ToolLineHeight.sp,
)

@Composable
internal fun threadCardMuted(): Color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

/** The rose a failure reads in (the semantic red the failed tool rows use). */
internal val ThreadCardRose: Color = DesignTokens.Semantic.Red

/** A running lane's dot (web `emerald-400`). */
private val LaneGreen = Color(0xFF34D399)

@Composable
fun WorkCardView(
    card: WorkCard,
    modifier: Modifier = Modifier,
    /** The phases behind a folded row, for its in-place expansion; empty =
     *  the folded row carries no chevron. */
    foldedPhases: List<WorkCardFoldedPhase> = emptyList(),
    /** A workflow lane row was picked: open that lane's transcript. */
    onOpenLane: ((WorkCardLaneTarget) -> Unit)? = null,
    /** A subagent card was picked: open its transcript. */
    onOpenSubagent: (() -> Unit)? = null,
    /** Rows the host appends under the card's own (an amber duplicate
     *  warning); never report text. */
    footer: @Composable ColumnScope.() -> Unit = {},
) {
    val style = threadCardTextStyle()
    val subagentOpens = card.kind == WorkCardKind.Subagent && onOpenSubagent != null
    val phases = remember(card) { if (card.kind == WorkCardKind.Workflow) rowPhases(card) else emptyList() }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .threadCard()
            .testTag(WorkCardSpec.TEST_ID_CARD),
    ) {
        if (subagentOpens) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(role = Role.Button, onClick = onOpenSubagent ?: {}),
            ) {
                WorkCardHeader(card, style)
                card.rows.forEach { row ->
                    if (row is WorkCardRow.Status) StatusRow(row.label, style)
                }
            }
        } else {
            WorkCardHeader(card, style)
        }
        if (card.segments.isNotEmpty()) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 12.dp, end = 12.dp, bottom = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(WorkCardSpec.SEGMENT_GAP.dp),
            ) {
                val track = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.12f)
                card.segments.forEach { segment ->
                    Box(
                        modifier = Modifier
                            .weight(1f)
                            .height(WorkCardSpec.SEGMENT_HEIGHT.dp)
                            .clip(CircleShape)
                            .background(track),
                    ) {
                        if (segment.fill > 0.0) {
                            Box(
                                Modifier
                                    .fillMaxHeight()
                                    .fillMaxWidth(segment.fill.toFloat().coerceIn(0f, 1f))
                                    .clip(CircleShape)
                                    .background(
                                        if (segment.state == WorkState.Failed) {
                                            ThreadCardRose
                                        } else {
                                            MaterialTheme.colorScheme.onSurface
                                        },
                                    ),
                            )
                        }
                    }
                }
            }
        }
        if (!subagentOpens && card.rows.isNotEmpty()) {
            card.rows.forEachIndexed { index, row ->
                Hairline()
                when (row) {
                    is WorkCardRow.Folded -> FoldedRow(row, foldedPhases, onOpenLane, style)
                    is WorkCardRow.Phase -> PhaseRow(row, phases.getOrElse(index) { index }, onOpenLane, style)
                    is WorkCardRow.Task -> CardRow {
                        StateGlyph(row.state)
                        Text(
                            row.label,
                            style = style,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    is WorkCardRow.Status -> StatusRow(row.label, style)
                }
            }
        }
        footer()
    }
}

@Composable
private fun WorkCardHeader(card: WorkCard, style: TextStyle) {
    val muted = threadCardMuted()
    val trailing = listOfNotNull(card.progress, card.elapsed).joinToString(WorkCardSpec.SEPARATOR)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(WorkCardSpec.HEADER.dp)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            workCardIcon(card.icon),
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = muted,
        )
        LeadText(
            card.name,
            style = style.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface,
        ) {
            val kindWord = card.kindWord
            if (kindWord != null && kindWord != card.name) {
                Text(kindWord, style = style, color = muted, maxLines = 1)
            }
        }
        if (card.running) InlineSpinner(size = 12.dp)
        if (trailing.isNotEmpty()) {
            Text(trailing, style = style.copy(fontFeatureSettings = "tnum"), color = muted, maxLines = 1)
        }
    }
}

/** The card's concept icon (fixture `icons`), the checklist when unknown. */
private fun workCardIcon(concept: String): ImageVector = when (concept) {
    WorkCardSpec.ICON_WORKFLOW -> ExpIcons.sessionTree
    WorkCardSpec.ICON_SUBAGENT -> ExpIcons.codingSubagent
    else -> ExpIcons.uiChecklist
}

@Composable
private fun Hairline() {
    HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
}

@Composable
private fun CardRow(
    modifier: Modifier = Modifier,
    content: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(WorkCardSpec.ROW.dp)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        content = content,
    )
}

@Composable
private fun StatusRow(label: String, style: TextStyle) {
    CardRow {
        Text(label, style = style, color = threadCardMuted(), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun StateGlyph(state: WorkState) {
    when (state) {
        WorkState.Running -> InlineSpinner(size = 12.dp)
        WorkState.Done -> Icon(ExpIcons.uiCheck, null, Modifier.size(12.dp), tint = threadCardMuted())
        WorkState.Failed -> Icon(ExpIcons.uiClose, null, Modifier.size(12.dp), tint = ThreadCardRose)
        WorkState.Pending -> Icon(
            ExpIcons.statusBacklog,
            null,
            Modifier.size(12.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
        )
    }
}

@Composable
private fun LaneDot(state: String) {
    val muted = threadCardMuted()
    val modifier = Modifier.size(6.dp).clip(CircleShape)
    when (state) {
        "running" -> Box(modifier.background(LaneGreen))
        "error" -> Box(modifier.background(ThreadCardRose))
        "queued" -> Box(modifier.border(1.dp, muted, CircleShape))
        else -> Box(modifier.background(muted))
    }
}

@Composable
private fun LaneRow(lane: WorkCardLane, style: TextStyle, onOpen: (() -> Unit)?) {
    val muted = threadCardMuted()
    val failed = lane.state == "error"
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (onOpen != null) Modifier.clickable(role = Role.Button, onClick = onOpen) else Modifier)
            .height(WorkCardSpec.LANE_ROW.dp)
            .padding(start = WorkCardSpec.LANE_INDENT.dp, end = 12.dp)
            .testTag(WorkCardSpec.TEST_ID_LANE),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        LaneDot(lane.state)
        LeadText(lane.label, style = style, color = if (failed) ThreadCardRose else MaterialTheme.colorScheme.onSurface) {
            lane.model?.let { Text(it, style = style, color = muted, maxLines = 1) }
        }
        if (lane.state == "running") {
            InlineSpinner(size = 12.dp)
        } else {
            lane.duration?.let {
                Text(it, style = style.copy(fontFeatureSettings = "tnum"), color = muted, maxLines = 1)
            }
        }
    }
}

@Composable
private fun FoldedRow(
    row: WorkCardRow.Folded,
    phases: List<WorkCardFoldedPhase>,
    onOpenLane: ((WorkCardLaneTarget) -> Unit)?,
    style: TextStyle,
) {
    var open by rememberSaveable { mutableStateOf(false) }
    val expandable = phases.isNotEmpty()
    val muted = threadCardMuted()
    val turn by animateFloatAsState(if (open) 90f else 0f, Motion.fast(), label = "work-card-fold")
    Column(modifier = Modifier.fillMaxWidth().testTag(WorkCardSpec.TEST_ID_FOLDED)) {
        CardRow(
            modifier = if (expandable) Modifier.clickable(role = Role.Button) { open = !open } else Modifier,
        ) {
            Icon(ExpIcons.uiCheck, null, Modifier.size(12.dp), tint = muted)
            LeadText(row.label, style = style, color = MaterialTheme.colorScheme.onSurface) {
                Text(row.lanes, style = style, color = muted, maxLines = 1)
            }
            row.duration?.let {
                Text(it, style = style.copy(fontFeatureSettings = "tnum"), color = muted, maxLines = 1)
            }
            if (expandable) {
                Icon(
                    ExpIcons.uiChevronRight,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp).rotate(turn),
                    tint = muted,
                )
            }
        }
        if (open) {
            phases.forEachIndexed { phaseIndex, phase ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(WorkCardSpec.LANE_ROW.dp)
                        .padding(start = WorkCardSpec.LANE_INDENT.dp, end = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        phase.title,
                        style = style,
                        color = muted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    phase.duration?.let {
                        Text(it, style = style.copy(fontFeatureSettings = "tnum"), color = muted, maxLines = 1)
                    }
                }
                phase.lanes.forEachIndexed { laneIndex, lane ->
                    LaneRow(
                        lane = lane,
                        style = style,
                        onOpen = onOpenLane?.let { open -> { open(WorkCardLaneTarget(phaseIndex, laneIndex)) } },
                    )
                }
            }
        }
    }
}

@Composable
private fun PhaseRow(
    row: WorkCardRow.Phase,
    phase: Int,
    onOpenLane: ((WorkCardLaneTarget) -> Unit)?,
    style: TextStyle,
) {
    val muted = threadCardMuted()
    Column(modifier = Modifier.fillMaxWidth()) {
        CardRow {
            StateGlyph(row.state)
            Text(
                row.title,
                style = style,
                color = if (row.state == WorkState.Pending) muted else MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                row.detail.orEmpty(),
                style = style,
                color = muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            row.duration?.let {
                Text(it, style = style.copy(fontFeatureSettings = "tnum"), color = muted, maxLines = 1)
            }
        }
        row.lanes.forEachIndexed { laneIndex, lane ->
            LaneRow(
                lane = lane,
                style = style,
                onOpen = onOpenLane?.let { open -> { open(WorkCardLaneTarget(phase, laneIndex)) } },
            )
        }
    }
}

/** The phase POSITION each row stands for: a leading folded row covers the
 *  first `segments - (rows - 1)` phases, every other row is one phase. */
private fun rowPhases(card: WorkCard): List<Int> {
    val folded = card.rows.firstOrNull() is WorkCardRow.Folded
    val foldedCount = if (folded) card.segments.size - (card.rows.size - 1) else 0
    return card.rows.indices.map { index ->
        if (folded) (if (index == 0) 0 else foldedCount + index - 1) else index
    }
}

/**
 * A row's leading cluster: [text] ellipsized (only as much as it must) with
 * its muted [trailing] word right after it, the cluster taking the space the
 * row's end items leave, so those end items sit flush right and never
 * truncate. (A `weight(fill = false)` text beside a weighted spacer gets only
 * HALF the free space, which cut names and drifted the end column.)
 */
@Composable
internal fun RowScope.LeadText(
    text: String,
    style: TextStyle,
    color: Color,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    Row(
        modifier = Modifier.weight(1f),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text,
            style = style,
            color = color,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        trailing()
    }
}
