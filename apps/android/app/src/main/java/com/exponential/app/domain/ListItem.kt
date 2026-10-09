package com.exponential.app.domain

/**
 * EXP-1248: THE list item's geometry, x4 — fixture
 * `packages/domain-contract/fixtures/list-item.json` (web `@exp/ui`
 * SessionRow + PrRow, desktop `domain::list_item`, iOS `ListItem.swift`).
 * Anatomy: [tree guides][lead glyph at base + indent·depth][mono identifier]
 * [title][caption, big only][trailing meta]. No fold chevron, no trailing
 * chevron, no buttons: the lead sits at the same x on every row of a depth,
 * so marks and titles align and a child's elbow ends at its own lead.
 */
object ListItem {
    const val BASE_DP = 12
    const val INDENT_DP = 14
    const val MARK_DP = 14
    const val GAP_DP = 8
    const val SMALL_DP = 32
    const val BIG_DP = 52
    const val PR_ROW_DP = 36
    const val PR_ROW_PHONE_DP = 40
    const val PR_NODE_DP = 12
    const val RAIL_DP = 1

    /** Where a row's lead box starts at [depth]: `base + indent·depth`. */
    fun leadX(depth: Int): Int = BASE_DP + INDENT_DP * depth
}

/** The ring colour a [PrNodeState] wears. */
enum class PrNodeRing { Emerald, Muted }

/**
 * EXP-1248: the PR row's lead, the StatusGlyph-style ring: an open PR = an
 * emerald ring, the current stack member = the ring with a filled centre, the
 * base branch = a muted ring. x4 (fixture `prNodes`).
 */
enum class PrNodeState(val ring: PrNodeRing, val filled: Boolean) {
    Open(PrNodeRing.Emerald, filled = false),
    Current(PrNodeRing.Emerald, filled = true),
    Base(PrNodeRing.Muted, filled = false),
    ;

    companion object {
        fun fromWire(value: String): PrNodeState? = when (value) {
            "open" -> Open
            "current" -> Current
            "base" -> Base
            else -> null
        }
    }
}
