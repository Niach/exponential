package com.exponential.app.ui.issue

import androidx.compose.material3.Button
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.material3.Text
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter

// ISO `yyyy-MM-dd` ⇄ UTC-midnight millis, shared with the glass DueDateSheet
// (EXP-240) — the M3 DatePickerState speaks epoch millis in UTC.
internal fun isoDateToUtcMillis(value: String?): Long? = value?.let {
    runCatching {
        DateTimeFormatter.ISO_LOCAL_DATE.parse(it)
        Instant.parse(it + "T00:00:00Z").toEpochMilli()
    }.getOrNull()
}

internal fun utcMillisToIsoDate(millis: Long): String =
    Instant.ofEpochMilli(millis).atZone(ZoneId.of("UTC")).toLocalDate().toString()

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueDatePickerDialog(
    initialDate: String?,
    onConfirm: (String?) -> Unit,
    onDismiss: () -> Unit,
) {
    val state = rememberDatePickerState(initialSelectedDateMillis = isoDateToUtcMillis(initialDate))

    DatePickerDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            Button(onClick = {
                onConfirm(state.selectedDateMillis?.let(::utcMillisToIsoDate))
            }) { Text("Set") }
        },
        dismissButton = {
            TextButton(onClick = { onConfirm(null) }) { Text("Clear") }
        },
    ) {
        DatePicker(state = state)
    }
}
