package com.exponential.app.ui

import androidx.compose.ui.graphics.Color

/**
 * Parse a hex color string (with or without a leading '#', 6- or 8-digit),
 * falling back to the brand indigo on malformed input. Shared across the
 * issue, home, settings and nav screens.
 */
fun parseColor(hex: String): Color {
    val cleaned = hex.removePrefix("#")
    return runCatching {
        Color(
            android.graphics.Color.parseColor(
                if (cleaned.length == 6) "#$cleaned" else "#FF$cleaned"
            )
        )
    }.getOrElse { Color(0xFF6366F1) }
}
