package com.exponential.app.ui.gettingstarted

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.net.toUri
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.components.AddDeviceSheet
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetDefaults
import com.exponential.app.ui.components.InviteLinkCard
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.onboarding.CreateBoardSheet
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-1270 (EXP-1259 D6): THE Getting started block ×4 — a header (title,
 * "N of M", the meter, the Docs button) over ONE horizontal strip of square
 * cards in the fixture's order. The next step is highlighted, done steps keep
 * their green check and stay tappable (do it again), nothing is locked and
 * nothing carries a description. Pure UI: [onTarget] runs a card's target
 * ([GettingStartedSection] binds them).
 */
@Composable
fun GettingStartedBlock(
    model: GettingStartedModel,
    onTarget: (GettingStartedTarget) -> Unit,
    onDocs: () -> Unit,
    modifier: Modifier = Modifier,
    /** The strip's side inset, so the first card lines up with the page gutter. */
    contentPadding: PaddingValues = PaddingValues(0.dp),
) {
    Column(
        modifier = modifier.fillMaxWidth().testTag(GettingStartedCopy.BLOCK_TEST_ID),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(contentPadding),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                GettingStartedCopy.TITLE,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Spacer(Modifier.width(10.dp))
            Text(
                model.progress,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Spacer(Modifier.width(10.dp))
            ProgressMeter(
                fraction = if (model.total == 0) 0f else model.done.toFloat() / model.total,
                modifier = Modifier.weight(1f, fill = false).width(GettingStartedCopy.METER.dp),
            )
            Spacer(Modifier.weight(1f))
            GlassPill(
                GettingStartedCopy.DOCS_LABEL,
                icon = ExpIcons.uiDocs,
                size = PillSize.Sm,
                onClick = onDocs,
                modifier = Modifier.testTag(GettingStartedCopy.DOCS_TEST_ID),
            )
        }
        LazyRow(
            modifier = Modifier.fillMaxWidth(),
            contentPadding = contentPadding,
            horizontalArrangement = Arrangement.spacedBy(GettingStartedCopy.CARD_GAP.dp),
        ) {
            items(model.entries, key = { it.key.wire }) { entry ->
                StepCard(
                    entry = entry,
                    number = model.entries.indexOf(entry) + 1,
                    onClick = { onTarget(entry.target) },
                )
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

/** One 104×112 card: the step glyph top-left, check or number top-right, the title bottom-left. */
@Composable
private fun StepCard(entry: GettingStartedEntry, number: Int, onClick: () -> Unit) {
    val step = GettingStarted.step(entry.key)
    val shape = RoundedCornerShape(DesignTokens.Radius.Lg)
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Column(
        modifier = Modifier
            .size(GettingStartedCopy.CARD_WIDTH.dp, GettingStartedCopy.CARD_HEIGHT.dp)
            .clip(shape)
            .background(if (entry.next) GlassTokens.RowFillActive else GlassTokens.CardFill, shape)
            .border(
                GlassTokens.Hairline,
                if (entry.next) MaterialTheme.colorScheme.onSurface.copy(alpha = 0.3f) else GlassTokens.StrokeCard,
                shape,
            )
            .clickable(onClick = onClick)
            .padding(GettingStartedCopy.CARD_PADDING.dp)
            .testTag(GettingStartedCopy.cardTestId(entry.key)),
    ) {
        Row(verticalAlignment = Alignment.Top) {
            Icon(
                stepIcon(entry.key),
                contentDescription = null,
                modifier = Modifier.size(GettingStartedCopy.CARD_ICON.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (entry.done) TextEmphasis.Tertiary else TextEmphasis.Secondary,
                ),
            )
            Spacer(Modifier.weight(1f))
            if (entry.done) {
                Icon(
                    ExpIcons.uiCheck,
                    contentDescription = "Done",
                    modifier = Modifier.size(14.dp),
                    tint = DesignTokens.Semantic.Green,
                )
            } else {
                Text(number.toString(), fontSize = 11.sp, color = muted)
            }
        }
        Spacer(Modifier.weight(1f))
        Text(
            step.title,
            fontSize = 13.sp,
            fontWeight = FontWeight.Medium,
            lineHeight = 17.sp,
            color = if (entry.done) muted else MaterialTheme.colorScheme.onSurface,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
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
 * Agent home with no board, the Settings entry's sheet): create-board = the
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
    viewModel: GettingStartedViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val teamId by viewModel.selectedTeamId.collectAsStateWithLifecycle()
    val origin by viewModel.instanceOrigin.collectAsStateWithLifecycle()
    val context = LocalContext.current
    var sheet by remember { mutableStateOf<GettingStartedTarget?>(null) }
    val model = state.model
    if (state.visible && model != null) {
        GettingStartedBlock(
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
 * The phones' Getting started ENTRY's sheet (the Settings row): the same
 * block in a bottom sheet.
 */
@Composable
fun GettingStartedSheet(onOpenAgent: () -> Unit, onDismiss: () -> Unit) {
    GlassSheet(title = null, onDismiss = onDismiss, modifier = Modifier.testTag("getting-started-sheet")) {
        GettingStartedSection(
            onOpenAgent = {
                onDismiss()
                onOpenAgent()
            },
            contentPadding = PaddingValues(horizontal = GlassSheetDefaults.HorizontalPadding),
        )
        Spacer(Modifier.height(8.dp))
    }
}
