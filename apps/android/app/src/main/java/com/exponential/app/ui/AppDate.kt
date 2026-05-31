package com.exponential.app.ui

import java.time.LocalDate
import java.time.format.DateTimeFormatter

private val isoDate: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd")
private val monthDay: DateTimeFormatter = DateTimeFormatter.ofPattern("MMM d")

/** Parse a `yyyy-MM-dd` due-date string, returning null on anything malformed. */
fun parseIsoDateOrNull(value: String?): LocalDate? =
    value?.takeIf { it.isNotBlank() }?.let { runCatching { LocalDate.parse(it, isoDate) }.getOrNull() }

/**
 * Today / Tomorrow / "MMM d" relative formatting for a due date, mirroring the
 * iOS `formatDueDate`. Falls back to the raw string if it can't be parsed.
 */
fun formatDueDate(value: String?): String {
    val date = parseIsoDateOrNull(value) ?: return value.orEmpty()
    val today = LocalDate.now()
    return when (date) {
        today -> "Today"
        today.plusDays(1) -> "Tomorrow"
        else -> date.format(monthDay)
    }
}
