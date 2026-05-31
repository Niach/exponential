package com.exponential.app.ui.issue

/**
 * Human-readable recurrence label, e.g. "Weekly" / "Every 3 days". Shared by
 * the issue detail and create screens.
 */
internal fun formatRecurrence(interval: Int?, unit: String?): String {
    if (interval == null || unit == null) return "Doesn't repeat"
    return when (unit) {
        "day" -> if (interval == 1) "Daily" else "Every $interval days"
        "week" -> if (interval == 1) "Weekly" else "Every $interval weeks"
        "month" -> if (interval == 1) "Monthly" else "Every $interval months"
        else -> "Every $interval $unit"
    }
}
