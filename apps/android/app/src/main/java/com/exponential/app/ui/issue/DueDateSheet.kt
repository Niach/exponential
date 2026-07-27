package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.icons.ExpIcons

/**
 * Due-date sheet (EXP-240): an embedded graphical M3 [DatePicker] whose taps
 * commit immediately (iOS parity — the sheet stays open for follow-up tweaks,
 * and ✕/swipe dismiss can never lose a pick), plus a destructive "Clear due
 * date" row. The due date is a date only — there is no time of day.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DueDateSheet(
    dueDate: String?,
    onSetDate: (String?) -> Unit,
    onDismiss: () -> Unit,
) {
    val dateState = rememberDatePickerState(initialSelectedDateMillis = isoDateToUtcMillis(dueDate))

    // Commit every calendar tap immediately. A null selection never commits:
    // the M3 calendar can't unselect, so the only null path is the Clear row
    // below (which resets this state itself before dismissing). The no-change
    // guard swallows the initial emission and re-taps of the current date.
    val currentDueDate by rememberUpdatedState(dueDate)
    val currentOnSetDate by rememberUpdatedState(onSetDate)
    LaunchedEffect(dateState) {
        snapshotFlow { dateState.selectedDateMillis }.collect { millis ->
            val iso = millis?.let(::utcMillisToIsoDate) ?: return@collect
            if (iso != currentDueDate) currentOnSetDate(iso)
        }
    }
    // The Clear row appears as soon as a date is picked (selection), not only
    // once the mutation round-trips into the synced issue (persisted).
    val hasDate = dueDate != null || dateState.selectedDateMillis != null

    GlassSheet(title = "Due date", onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
        ) {
            DatePicker(
                state = dateState,
                title = null,
                headline = null,
                showModeToggle = false,
                colors = DatePickerDefaults.colors(containerColor = Color.Transparent),
                modifier = Modifier.padding(horizontal = 4.dp),
            )
            if (hasDate) {
                GlassSheetRow(
                    label = "Clear due date",
                    labelColor = MaterialTheme.colorScheme.error,
                    leading = {
                        Icon(
                            ExpIcons.uiClose,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                            tint = MaterialTheme.colorScheme.error,
                        )
                    },
                    onClick = {
                        dateState.selectedDateMillis = null
                        onSetDate(null)
                        onDismiss()
                    },
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}
