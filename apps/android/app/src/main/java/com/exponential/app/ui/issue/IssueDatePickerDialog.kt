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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueDatePickerDialog(
    initialDate: String?,
    onConfirm: (String?) -> Unit,
    onDismiss: () -> Unit,
) {
    val initialMillis = initialDate?.let {
        runCatching {
            DateTimeFormatter.ISO_LOCAL_DATE.parse(it)
            Instant.parse(it + "T00:00:00Z").toEpochMilli()
        }.getOrNull()
    }
    val state = rememberDatePickerState(initialSelectedDateMillis = initialMillis)

    DatePickerDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            Button(onClick = {
                val millis = state.selectedDateMillis
                val iso = millis?.let {
                    Instant.ofEpochMilli(it).atZone(ZoneId.of("UTC")).toLocalDate().toString()
                }
                onConfirm(iso)
            }) { Text("Set") }
        },
        dismissButton = {
            TextButton(onClick = { onConfirm(null) }) { Text("Clear") }
        },
    ) {
        DatePicker(state = state)
    }
}
