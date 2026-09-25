package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.NO_ESTIMATE
import com.exponential.app.domain.estimateLabel
import com.exponential.app.domain.estimatePickerValues
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow

/**
 * Estimate sheet (EXP-630, web `EstimateControl` parity): a pinned
 * "No estimate" row, then the team scale's ladder ([estimatePickerValues] —
 * an off-ladder current value joins it in order) labelled by [estimateLabel]
 * ("M" / "5 points"), the current one checked. Selecting dismisses.
 *
 * EXP-1021's ONE recorded exception, pinned by `PickerContractTest`: an
 * estimate is not one of the ten typed picker subjects — it has no id, no
 * glyph and no colour, only a number the caller renders as a word — and iOS's
 * `EstimateSheet` keeps the same [GlassSheetRow] idiom. Sweeping this one onto
 * the primitive alone would put the two phones back out of step, which is the
 * thing EXP-1021 exists to stop. The idiom already agrees where it matters:
 * a [GlassSheetRow] marks a single pick with the SAME trailing check the
 * picker's single arm draws.
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
            // Plain text rows on every client (web `EstimateControl`, iOS
            // `EstimateSheet`, desktop menu): the value IS the label.
            GlassSheetRow(
                label = NO_ESTIMATE,
                selected = current == null,
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
