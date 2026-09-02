package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.WidgetSubmissionResult
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

// Pretty-printer for the free-form customData blob (display only).
private val prettyJson = Json { prettyPrint = true }

/**
 * EXP-496: the widget/agent submission metadata card — mobile sibling of
 * web's `widget-submission-card.tsx` (Reporter · Page · Display · User agent
 * · Custom data). Expandable, DEFAULT COLLAPSED, styled like the PR row
 * (rounded glass row); the caller renders it only when a submission row
 * exists.
 */
@Composable
fun WidgetSubmissionCard(
    submission: WidgetSubmissionResult,
    isAgent: Boolean,
    modifier: Modifier = Modifier,
) {
    var expanded by remember(submission) { mutableStateOf(false) }

    val reporter = when {
        submission.reporterName != null && submission.reporterEmail != null ->
            "${submission.reporterName} <${submission.reporterEmail}>"
        submission.reporterName != null -> submission.reporterName
        submission.reporterEmail != null -> submission.reporterEmail
        else -> "Anonymous"
    }

    val display = buildList {
        val viewportWidth = submission.viewportWidth
        val viewportHeight = submission.viewportHeight
        if (viewportWidth != null && viewportHeight != null) {
            val ratio = submission.devicePixelRatio
            // Locale-independent: "2" for whole ratios, "1.5" otherwise (web
            // parity).
            val dpr = when {
                ratio == null -> ""
                ratio == ratio.toLong().toDouble() -> " @${ratio.toLong()}x"
                else -> " @${ratio}x"
            }
            add("Viewport ${viewportWidth}×${viewportHeight}$dpr")
        }
        val screenWidth = submission.screenWidth
        val screenHeight = submission.screenHeight
        if (screenWidth != null && screenHeight != null) {
            add("Screen ${screenWidth}×${screenHeight}")
        }
    }.joinToString(" · ")

    val customDataJson = submission.customData
        ?.takeIf { it.isNotEmpty() }
        ?.let { prettyJson.encodeToString(JsonObject.serializer(), it) }

    Column(modifier = modifier.fillMaxWidth().glassRow()) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier
                .fillMaxWidth()
                .clickable { expanded = !expanded }
                .padding(horizontal = 12.dp, vertical = 10.dp),
        ) {
            Icon(
                if (isAgent) ExpIcons.uiAgentSource else ExpIcons.uiWidget,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Text(
                if (isAgent) "Reported by agent" else "Reported via widget",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.weight(1f),
            )
            Icon(
                if (expanded) ExpIcons.uiChevronUp else ExpIcons.uiChevronDown,
                contentDescription = if (expanded) "Collapse" else "Expand",
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        if (expanded) {
            Column(
                verticalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier.padding(start = 12.dp, end = 12.dp, bottom = 10.dp),
            ) {
                MetaRow("Reporter", reporter)
                submission.pageUrl?.let { MetaRow("Page", it) }
                if (display.isNotEmpty()) MetaRow("Display", display)
                submission.userAgent?.let { MetaRow("User agent", it) }
                if (customDataJson != null) {
                    Row(verticalAlignment = Alignment.Top) {
                        MetaLabel("Custom data")
                        Spacer(Modifier.width(8.dp))
                        Text(
                            customDataJson,
                            style = MaterialTheme.typography.bodySmall,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            modifier = Modifier
                                .weight(1f)
                                .background(GlassTokens.SectionFill, RoundedCornerShape(DesignTokens.Radius.Sm))
                                .padding(8.dp),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun MetaRow(label: String, value: String) {
    Row(verticalAlignment = Alignment.Top) {
        MetaLabel(label)
        Spacer(Modifier.width(8.dp))
        Text(
            value,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun MetaLabel(label: String) {
    Text(
        label,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.width(80.dp),
    )
}
