package com.exponential.app.ui.session

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.ScriptCard
import com.exponential.app.domain.ScriptCardSpec
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

// EXP-1279 (EXP-1259 D13): a code-mode call (`mcp__codemode__exec`, contract
// toolKind `script`) as ONE card: "Ran a script · 59 calls · 3.0 s", the calls
// it made grouped by tool (our brand mark on an Exponential tool, the generic
// tool glyph otherwise; first subject + "+n", ×count, failures in rose) and
// the script itself behind the "Script" disclosure. The model is
// `domain/ScriptCard.kt` (fixture `script-card.json` ×4); this only paints it.
// Web `@exp/ui` `ScriptCard`.

@Composable
fun ScriptCardView(
    card: ScriptCard,
    modifier: Modifier = Modifier,
    /** The call is still going: the title alone with the run's spinner. */
    running: Boolean = false,
    /** The exec call itself failed (not one of its calls). */
    failed: Boolean = false,
    /** The exec input's `script`; null = no disclosure. */
    source: String? = null,
) {
    var open by rememberSaveable { mutableStateOf(false) }
    val script = source?.takeIf { it.isNotBlank() }
    val style = threadCardTextStyle()
    val muted = threadCardMuted()
    val turn by animateFloatAsState(if (open) 90f else 0f, Motion.fast(), label = "script-card-disclosure")
    Column(
        modifier = modifier
            .fillMaxWidth()
            .threadCard()
            .testTag(ScriptCardSpec.TEST_ID_CARD),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(40.dp)
                .padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                ExpIcons.toolScript,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = if (failed) ThreadCardRose else muted,
            )
            // The title ellipsizes before the meta ever does: "· 59 calls ·
            // 3.0 s" always reads whole.
            LeadText(
                card.title,
                style = style.copy(fontWeight = FontWeight.Medium),
                color = if (failed) ThreadCardRose else MaterialTheme.colorScheme.onSurface,
            ) {
                if (running) InlineSpinner(size = 12.dp)
                card.meta?.let { meta ->
                    Text("· $meta", style = style.copy(fontFeatureSettings = "tnum"), color = muted, maxLines = 1)
                }
            }
            if (script != null) {
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .clickable(role = Role.Button) { open = !open }
                        .padding(horizontal = 4.dp, vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Text(ScriptCardSpec.SCRIPT, style = style, color = muted, maxLines = 1)
                    Icon(
                        ExpIcons.uiChevronRight,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp).rotate(turn),
                        tint = muted,
                    )
                }
            }
        }
        if (open && script != null) {
            Text(
                script,
                style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier
                    .padding(start = 12.dp, end = 12.dp, bottom = 8.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(8.dp))
                    .border(GlassTokens.Hairline, GlassTokens.StrokeRow, RoundedCornerShape(8.dp))
                    .background(MaterialTheme.colorScheme.onSurface.copy(alpha = 0.03f))
                    .heightIn(max = DomainContract.diffUiInlineDiffMaxHeight.dp)
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 8.dp, vertical = 6.dp)
                    .testTag(ScriptCardSpec.TEST_ID_SOURCE),
            )
        }
        if (card.groups.isNotEmpty() || card.overflow != null) {
            Column(modifier = Modifier.fillMaxWidth().padding(bottom = 6.dp)) {
                card.groups.forEach { group ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(28.dp)
                            .padding(horizontal = 12.dp)
                            .testTag(ScriptCardSpec.TEST_ID_GROUP),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        if (group.exponential) {
                            ExponentialMark(size = 12.dp)
                        } else {
                            Icon(ExpIcons.codingTool, null, Modifier.size(12.dp), tint = muted)
                        }
                        Text(group.label, style = style, color = MaterialTheme.colorScheme.onSurface, maxLines = 1)
                        // Fills the row so the count / failure sits flush right.
                        Text(
                            group.subject?.let { subject -> group.more?.let { "$subject $it" } ?: subject }.orEmpty(),
                            style = style,
                            color = muted,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                        val tail = group.failed ?: group.count
                        if (tail != null) {
                            Text(
                                tail,
                                style = style.copy(fontFeatureSettings = "tnum"),
                                color = if (group.failed != null) ThreadCardRose else muted,
                                maxLines = 1,
                            )
                        }
                    }
                }
                card.overflow?.let { overflow ->
                    Row(
                        modifier = Modifier.fillMaxWidth().height(28.dp).padding(horizontal = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(overflow, style = style, color = muted, maxLines = 1)
                    }
                }
            }
        }
    }
}
