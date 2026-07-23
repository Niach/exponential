package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Due-date sheet (EXP-240): an embedded graphical M3 [DatePicker] whose taps
 * commit immediately (iOS parity — the sheet stays open for follow-up tweaks,
 * and ✕/swipe dismiss can never lose a pick), Start/End time rows (enabled the
 * moment a date is selected; they open the existing [IssueTimePickerDialog]s),
 * and a destructive "Clear due date" row. The date/time mutations stay the
 * separate tRPC fields they always were.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DueDateSheet(
    dueDate: String?,
    dueTime: String?,
    endTime: String?,
    onSetDate: (String?) -> Unit,
    onSetDueTime: (String?) -> Unit,
    onSetEndTime: (String?) -> Unit,
    onDismiss: () -> Unit,
) {
    val dateState = rememberDatePickerState(initialSelectedDateMillis = isoDateToUtcMillis(dueDate))
    var dueTimeOpen by remember { mutableStateOf(false) }
    var endTimeOpen by remember { mutableStateOf(false) }

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
    // Time rows unlock as soon as a date is picked (selection), not only once
    // the mutation round-trips into the synced issue (persisted).
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
            GlassSheetRow(
                label = "Start time",
                enabled = hasDate,
                leading = {
                    Icon(
                        Icons.Filled.Schedule,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                        tint = Color.White.copy(alpha = if (hasDate) TextEmphasis.Secondary else TextEmphasis.Quaternary),
                    )
                },
                trailing = { TimeValueText(dueTime, enabled = hasDate) },
                onClick = { dueTimeOpen = true },
            )
            GlassSheetRow(
                label = "End time",
                enabled = hasDate,
                leading = {
                    Icon(
                        Icons.Filled.Schedule,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                        tint = Color.White.copy(alpha = if (hasDate) TextEmphasis.Secondary else TextEmphasis.Quaternary),
                    )
                },
                trailing = { TimeValueText(endTime, enabled = hasDate) },
                onClick = { endTimeOpen = true },
            )
            if (hasDate) {
                GlassSheetRow(
                    label = "Clear due date",
                    labelColor = MaterialTheme.colorScheme.error,
                    leading = {
                        Icon(
                            Icons.Filled.Close,
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

    if (dueTimeOpen) {
        IssueTimePickerDialog(
            initialTime = dueTime,
            title = "Start time",
            onConfirm = { onSetDueTime(it); dueTimeOpen = false },
            onClear = { onSetDueTime(null); dueTimeOpen = false },
            onDismiss = { dueTimeOpen = false },
        )
    }

    if (endTimeOpen) {
        IssueTimePickerDialog(
            initialTime = endTime,
            title = "End time",
            onConfirm = { onSetEndTime(it); endTimeOpen = false },
            onClear = { onSetEndTime(null); endTimeOpen = false },
            onDismiss = { endTimeOpen = false },
        )
    }
}

@Composable
private fun TimeValueText(time: String?, enabled: Boolean) {
    Text(
        time ?: "—",
        style = MaterialTheme.typography.bodyMedium,
        fontFamily = FontFamily.Monospace,
        color = Color.White.copy(
            alpha = when {
                !enabled -> TextEmphasis.Quaternary
                time != null -> TextEmphasis.Primary
                else -> TextEmphasis.Tertiary
            },
        ),
    )
}
