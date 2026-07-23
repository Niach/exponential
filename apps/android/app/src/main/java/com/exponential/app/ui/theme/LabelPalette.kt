package com.exponential.app.ui.theme

/**
 * The 20-swatch label color palette, identical to the iOS `LabelPalette` so a
 * label created or recolored on either client offers the same choices.
 */
object LabelPalette {
    val colors: List<String> = listOf(
        "#ef4444", "#f97316", "#f59e0b", "#eab308",
        "#84cc16", "#22c55e", "#10b981", "#14b8a6",
        "#06b6d4", "#3b82f6", "#6366f1", "#8b5cf6",
        "#a855f7", "#d946ef", "#ec4899", "#f43f5e",
        "#64748b", "#6b7280", "#71717a", "#737373",
    )

    /**
     * Deterministic quick-create color (EXP-240) — the shared cross-platform
     * rule `palette[abs(hash(lowercased name)) % palette.count]`, with hash =
     * the Java-style 31-based 32-bit wrapping string hash (spelled out so the
     * iOS twin can mirror it byte-for-byte; Swift's Hashable is seeded per
     * launch and unusable here). abs via Long so Int.MIN_VALUE can't overflow.
     */
    fun autoColor(name: String): String {
        val hash = name.lowercase().fold(0) { acc, c -> acc * 31 + c.code }
        val index = (kotlin.math.abs(hash.toLong()) % colors.size).toInt()
        return colors[index]
    }
}
