package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.NO_ESTIMATE
import com.exponential.app.domain.estimateLabel
import com.exponential.app.domain.estimatePickerValues
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Estimate sheet (EXP-630, web `EstimateControl` parity): a pinned
 * "No estimate" row, then the team scale's ladder ([estimatePickerValues] —
 * an off-ladder current value joins it in order) labelled by [estimateLabel]
 * ("M" / "5 points"), the current one checked. Selecting dismisses.
 */
@Composable
fun EstimatePickerSheet(
    current: Int?,
    estimationType: String,
    onSelect: (Int?) -> Unit,
    onDismiss: () -> Unit,
) {
    val values = remember(current, estimationType) { estimatePickerValues(current, estimationType) }

    GlassSheet(title = "Estimate", onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
        ) {
            GlassSheetRow(
                label = NO_ESTIMATE,
                selected = current == null,
                leading = {
                    Icon(
                        ExpIcons.uiEstimate,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                        tint = Color.White.copy(alpha = TextEmphasis.Tertiary),
                    )
                },
                onClick = {
                    onSelect(null)
                    onDismiss()
                },
            )
            values.forEach { value ->
                GlassSheetRow(
                    label = estimateLabel(value, estimationType),
                    selected = value == current,
                    onClick = {
                        onSelect(value)
                        onDismiss()
                    },
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}
