package com.exponential.app.ui.gettingstarted

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.net.toUri
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.components.AddDeviceSheet
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetDefaults
import com.exponential.app.ui.components.InviteLinkCard
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionBand
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.onboarding.CreateBoardSheet
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/** A later row's chevron emphasis (the fixture's "muted at 45%"). */
private const val LaterChevronAlpha = 0.45f

/**
 * EXP-1270 (EXP-1259 D6): THE Getting started block ×4 — a vertical path in
 * the list language: the group band (title, "N of M", the meter, Docs as a
 * ghost icon button) over flat hairline-divided rows in the fixture's order.
 * A done row is muted with a green check (still tappable: do it again); the
 * ONE next row is expanded (the active fill, its `why` and ONE primary
 * button labelled `cta`); later rows are plain with a quiet chevron. Rows are
 * at least `row.heightPhone` tall; the block is at most [maxWidth] wide (null
 * = the phone sheet's full width) and never scrolls sideways. Pure UI:
 * [onTarget] runs a row's target ([GettingStartedSection] binds them).
 */
@Composable
fun GettingStartedList(
    model: GettingStartedModel,
    onTarget: (GettingStartedTarget) -> Unit,
    onDocs: () -> Unit,
    modifier: Modifier = Modifier,
    /** The block's side inset, so the band lines up with the page gutter. */
    contentPadding: PaddingValues = PaddingValues(0.dp),
    maxWidth: Dp? = GettingStartedCopy.MAX_WIDTH.dp,
) {
    Column(
        modifier = modifier
            .padding(contentPadding)
            .then(if (maxWidth != null) Modifier.widthIn(max = maxWidth) else Modifier)
            .fillMaxWidth()
            .testTag(GettingStartedCopy.BLOCK_TEST_ID),
    ) {
        SectionBand(
            contentPadding = PaddingValues(start = GettingStartedCopy.ROW_PADDING.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
        ) {
            Text(
                GettingStartedCopy.TITLE,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
            )
            Spacer(Modifier.width(10.dp))
            Text(
                model.progress,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
            )
            Spacer(Modifier.width(10.dp))
            ProgressMeter(
                fraction = if (model.total == 0) 0f else model.done.toFloat() / model.total,
                modifier = Modifier.weight(1f, fill = false).width(GettingStartedCopy.METER.dp),
            )
            Spacer(Modifier.weight(1f))
            CircleIconButton(
                ExpIcons.uiDocs,
                contentDescription = GettingStartedCopy.DOCS_LABEL,
                onClick = onDocs,
                modifier = Modifier.testTag(GettingStartedCopy.DOCS_TEST_ID),
                borderless = true,
                glyphSize = 16.dp,
            )
        }
        model.entries.forEachIndexed { index, entry ->
            HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
            val onClick = { onTarget(entry.target) }
            when {
                entry.next -> NextRow(entry, onClick)
                else -> StepRow(entry, onClick)
            }
            if (entry.next && index == model.entries.lastIndex) {
                HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
            }
        }
    }
}

@Composable
private fun ProgressMeter(fraction: Float, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .height(4.dp)
            .clip(RoundedCornerShape(2.dp))
            .background(GlassTokens.RowFillActive),
    ) {
        Box(
            Modifier
                .fillMaxHeight()
                .fillMaxWidth(fraction.coerceIn(0f, 1f))
                .background(MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)),
        )
    }
}

/** A done row (muted, a green check) or a later one (plain, a quiet chevron). */
@Composable
private fun StepRow(entry: GettingStartedEntry, onClick: () -> Unit) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = GettingStartedCopy.ROW_HEIGHT_PHONE.dp)
            .clickable(onClick = onClick)
            .padding(horizontal = GettingStartedCopy.ROW_PADDING.dp)
            .testTag(GettingStartedCopy.rowTestId(entry.key)),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            stepIcon(entry.key),
            contentDescription = null,
            modifier = Modifier.size(GettingStartedCopy.ROW_ICON.dp),
            tint = if (entry.done) muted else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Text(
            GettingStarted.step(entry.key).title,
            style = MaterialTheme.typography.bodyLarge,
            color = if (entry.done) muted else MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(12.dp))
        if (entry.done) {
            Icon(
                ExpIcons.uiCheck,
                contentDescription = "Done",
                modifier = Modifier.size(16.dp),
                tint = DesignTokens.Semantic.Green,
            )
        } else {
            Icon(
                ExpIcons.uiChevronRight,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = LaterChevronAlpha),
            )
        }
    }
}

/** THE expanded next row: the active fill, title + `Next`, the `why` and the `cta` button. */
@Composable
private fun NextRow(entry: GettingStartedEntry, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = GettingStartedCopy.ROW_HEIGHT_PHONE.dp)
            .background(GlassTokens.RowFillActive)
            .clickable(onClick = onClick)
            .padding(
                horizontal = GettingStartedCopy.ROW_PADDING.dp,
                vertical = GettingStartedCopy.ROW_EXPANDED_PADDING.dp,
            )
            .testTag(GettingStartedCopy.rowTestId(entry.key)),
        verticalAlignment = Alignment.Top,
    ) {
        Icon(
            stepIcon(entry.key),
            contentDescription = null,
            // Centred on the title's first line.
            modifier = Modifier.padding(top = 4.dp).size(GettingStartedCopy.ROW_ICON.dp),
            tint = MaterialTheme.colorScheme.onSurface,
        )
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    GettingStarted.step(entry.key).title,
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(12.dp))
                GlassPill(GettingStartedCopy.NEXT_LABEL, size = PillSize.Sm)
            }
            Spacer(Modifier.height(2.dp))
            Text(
                GettingStartedCopy.why(entry.key),
                fontSize = 13.sp,
                lineHeight = 18.sp,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Spacer(Modifier.height(10.dp))
            GlassPill(
                GettingStartedCopy.cta(entry.key),
                onClick = onClick,
                primary = true,
                modifier = Modifier.testTag(GettingStartedCopy.CTA_TEST_ID),
            )
        }
    }
}

/** The step's concept glyph (`steps[].icon`). */
internal fun stepIcon(key: GettingStartedStepKey): ImageVector = when (key) {
    GettingStartedStepKey.Board -> ExpIcons.uiFolder
    GettingStartedStepKey.Github -> ExpIcons.uiGithub
    GettingStartedStepKey.Device -> ExpIcons.uiDevice
    GettingStartedStepKey.Run -> ExpIcons.actionRun
    GettingStartedStepKey.Invite -> ExpIcons.uiInvite
    GettingStartedStepKey.Server -> ExpIcons.uiServer
    GettingStartedStepKey.Mcp -> ExpIcons.uiMcp
    GettingStartedStepKey.Widget -> ExpIcons.uiWidget
}

/**
 * The block WITH its targets bound, for every host (an empty board, the
 * Agent home with no board, the More row's sheet): create-board = the
 * create-board sheet, github = the web connect page, add-device / add-server
 * = the ONE device-setup sheet, invite = the invite link, agent = [onOpenAgent]
 * (the Agent page with its composer focused). Renders nothing while loading
 * or once complete.
 */
@Composable
fun GettingStartedSection(
    onOpenAgent: () -> Unit,
    modifier: Modifier = Modifier,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    maxWidth: Dp? = GettingStartedCopy.MAX_WIDTH.dp,
    viewModel: GettingStartedViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val teamId by viewModel.selectedTeamId.collectAsStateWithLifecycle()
    val origin by viewModel.instanceOrigin.collectAsStateWithLifecycle()
    val context = LocalContext.current
    var sheet by remember { mutableStateOf<GettingStartedTarget?>(null) }
    val model = state.model
    if (state.visible && model != null) {
        GettingStartedList(
            model = model,
            onTarget = { target ->
                when (target) {
                    GettingStartedTarget.Agent -> onOpenAgent()
                    // Phones open the web page for the GitHub step (the
                    // contract's phone target).
                    GettingStartedTarget.Github -> origin?.let {
                        context.startActivity(Intent(Intent.ACTION_VIEW, "$it/integrations/github?return=app".toUri()))
                    }
                    else -> sheet = target
                }
            },
            onDocs = {
                context.startActivity(Intent(Intent.ACTION_VIEW, GettingStartedCopy.DOCS_URL.toUri()))
            },
            modifier = modifier,
            contentPadding = contentPadding,
            maxWidth = maxWidth,
        )
    }
    when (sheet) {
        GettingStartedTarget.CreateBoard -> CreateBoardSheet(
            teamId = teamId,
            onCreated = { sheet = null },
            onDismiss = { sheet = null },
        )
        GettingStartedTarget.AddDevice, GettingStartedTarget.AddServer ->
            AddDeviceSheet(onDismiss = { sheet = null })
        GettingStartedTarget.Invite -> teamId?.let { id ->
            GlassSheet(title = GettingStarted.step(GettingStartedStepKey.Invite).title, onDismiss = { sheet = null }) {
                InviteLinkCard(
                    teamId = id,
                    modifier = Modifier.padding(horizontal = GlassSheetDefaults.HorizontalPadding),
                )
            }
        }
        else -> Unit
    }
}

/**
 * The phones' Getting started ENTRY's sheet (the More row): the same block,
 * full width, scrolling vertically in a bottom sheet.
 */
@Composable
fun GettingStartedSheet(onOpenAgent: () -> Unit, onDismiss: () -> Unit) {
    GlassSheet(title = null, onDismiss = onDismiss, modifier = Modifier.testTag("getting-started-sheet")) {
        Column(Modifier.verticalScroll(rememberScrollState())) {
            GettingStartedSection(
                onOpenAgent = {
                    onDismiss()
                    onOpenAgent()
                },
                maxWidth = null,
            )
            Spacer(Modifier.height(8.dp))
        }
    }
}
