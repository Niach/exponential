package com.exponential.app.domain

import java.util.Locale

// EXP-1051: where a run's context window actually GOES — the stacked bar and
// its legend, folded from the `usage` meter (how full the window is) plus the
// device's `context_layout` state (what the launcher put in it before the
// first turn).
//
// Hand-mirrored ×4 and byte-locked by the ONE contract fixture every client's
// test iterates (`packages/domain-contract/fixtures/context-layout.json`):
//   web      apps/web/src/lib/context-layout.ts
//   desktop  apps/desktop/crates/ui/src/context_layout.rs
//   iOS      apps/ios/ExpCore/Sources/Domain/ContextLayoutPresentation.swift
// Changing a rule or a string here means changing it in all four and in the
// fixture — the fixture IS the spec.
//
// Two rules the renderers depend on:
//  • `conversation` and `free` are DERIVED here, never on the wire: the device
//    publishes only what it can attribute, and everything else the window
//    holds is the conversation.
//  • The segments are NEVER scaled to fit. When the device's estimates
//    overshoot the measured `contextUsed` the percents still add past 100 and
//    the renderer CLIPS — a silently rescaled bar would lie about the layer
//    sizes to hide a rounding error.

/** One slice of the stacked bar. [percent] is of the WHOLE window (size), to
 *  two decimals — small layers (a 600-token task prompt in a 200k window) are
 *  a hairline rather than nothing. `free` is not a slice: it is the track. */
data class ContextBarSlice(
    val key: String,
    val tone: String,
    val percent: Double,
)

/** One legend row. Both numbers are already FORMATTED — the ×4 lock is on the
 *  strings, not on the arithmetic behind them. */
data class ContextLegendRow(
    val key: String,
    val label: String,
    val tone: String,
    /** `21k`, `37.4k`, `600` — see [ContextLayoutPresentation.tokensCompact]. */
    val tokens: String,
    /** `10.5%`, `0.3%` — one decimal, always. */
    val percent: String,
    /** The device guessed this layer rather than measuring it; the UI prefixes
     *  `≈`. Always false for the derived rows. */
    val estimated: Boolean,
    /** What the layer is made of, when the device named it (`CLAUDE.md,
     *  ~/.claude/CLAUDE.md`). */
    val detail: String? = null,
)

/** The whole block: the headline the "Context" line used to carry alone, the
 *  stacked bar, and the legend under it. */
data class ContextWindowView(
    /** `65k / 200k (32%)` — [AgentUsagePresentation.formatContextUsage]. */
    val headline: String,
    /** 0-100, floored ([AgentUsagePresentation.contextPercent]). */
    val percent: Int,
    val severity: AgentUsageSeverity,
    /** Where the bar draws its marks: the compaction floor, then the two usage
     *  thresholds. */
    val ticks: List<Int>,
    val bar: List<ContextBarSlice>,
    val legend: List<ContextLegendRow>,
)

object ContextLayoutPresentation {

    /** EXP-1051 section title. Byte-identical ×4. */
    val TITLE: String = DomainContract.contextLayoutTitle

    /** One contract layer: its wire key, its label and its tone. */
    private data class Spec(val key: String, val label: String, val tone: String)

    private val SEGMENT_SPECS: List<Spec> =
        DomainContract.contextLayoutSegmentKeys.mapIndexed { index, key ->
            Spec(
                key = key,
                label = DomainContract.contextLayoutSegmentLabels.getOrElse(index) { key },
                tone = DomainContract.contextLayoutSegmentTones.getOrElse(index) { "neutral" },
            )
        }

    private val DERIVED_SPECS: List<Spec> =
        DomainContract.contextLayoutDerivedKeys.mapIndexed { index, key ->
            Spec(
                key = key,
                label = DomainContract.contextLayoutDerivedLabels.getOrElse(index) { key },
                tone = DomainContract.contextLayoutDerivedTones.getOrElse(index) { "neutral" },
            )
        }

    private val CONVERSATION: Spec? = DERIVED_SPECS.firstOrNull { it.key == "conversation" }
    private val FREE: Spec? = DERIVED_SPECS.firstOrNull { it.key == "free" }

    /** EXP-484's thresholds, the bar's second and third marks. */
    private const val WARNING_PERCENT = 75
    private const val DANGER_PERCENT = 95

    /**
     * `21k`, `37.4k`, `1.5k`, `134.7k`, and the raw number under 1000 (`600`).
     * One decimal, with a trailing `.0` dropped — `21.0k` reads as false
     * precision on a number the device estimated.
     */
    fun tokensCompact(tokens: Int): String {
        val value = tokens.coerceAtLeast(0)
        if (value < 1000) return value.toString()
        val text = String.format(Locale.US, "%.1f", value / 1000.0)
        return "${if (text.endsWith(".0")) text.dropLast(2) else text}k"
    }

    /** Percent of the whole window, to TWO decimals — the bar's geometry.
     *  Rounded HALF-UP off the same quotient the other three clients take
     *  (`Math.round` in JS, never a banker's-rounding `BigDecimal`), so an
     *  exact half lands on the same side ×4. */
    private fun barPercent(tokens: Int, size: Int): Double =
        Math.round(tokens.toDouble() * 10_000.0 / size) / 100.0

    /** Percent of the whole window, to ONE decimal plus the sign — the
     *  legend's string. Rounded from the same arithmetic as [barPercent], so a
     *  row reading `0.0%` is a row whose slice is a hairline, never a rounding
     *  disagreement between the two. */
    private fun legendPercent(tokens: Int, size: Int): String =
        String.format(Locale.US, "%.1f%%", Math.round(tokens.toDouble() * 1_000.0 / size) / 10.0)

    /** The wire segments this client KNOWS, in the contract's render order: an
     *  unknown key is dropped (an older client never draws a layer it cannot
     *  label), the FIRST of a duplicate key wins, a negative count reads as
     *  zero and a zero-token layer never draws at all. */
    private fun knownSegments(segments: List<ContextSegment>?): List<Pair<Spec, ContextSegment>> {
        if (segments == null) return emptyList()
        return SEGMENT_SPECS.mapNotNull { spec ->
            val segment = segments.firstOrNull { it.key == spec.key } ?: return@mapNotNull null
            val tokens = segment.tokens.coerceAtLeast(0)
            if (tokens == 0) return@mapNotNull null
            spec to segment.copy(tokens = tokens)
        }
    }

    /**
     * The whole context-window view, or null when there is no window to draw:
     * the engine has published no `usage` yet, or it reported a zero size
     * ("unknown"). A layout WITHOUT a usage is nothing — the bar has no scale.
     *
     * `conversation` = what the window holds that the device could not
     * attribute (clamped at 0 when its estimates overshoot), `free` = the rest
     * of the window (clamped at 0 once a run runs past its own size).
     */
    fun contextWindowView(
        usage: SessionUsageState?,
        segments: List<ContextSegment>?,
    ): ContextWindowView? {
        val percent = AgentUsagePresentation.contextPercent(usage) ?: return null
        if (usage == null) return null
        val size = usage.contextSize
        val used = usage.contextUsed.coerceAtLeast(0)

        val known = knownSegments(segments)
        val attributed = known.sumOf { it.second.tokens }
        val conversation = (used - attributed).coerceAtLeast(0)
        val free = (size - used).coerceAtLeast(0)

        val bar = known.map { (spec, segment) ->
            ContextBarSlice(
                key = spec.key,
                tone = spec.tone,
                percent = barPercent(segment.tokens, size),
            )
        }.toMutableList()
        // Always drawn, even at zero: the conversation is the slice that GROWS,
        // and a bar that gains a segment mid-run would re-key its slices.
        bar += ContextBarSlice(
            key = CONVERSATION?.key ?: "conversation",
            tone = CONVERSATION?.tone ?: "blue",
            percent = barPercent(conversation, size),
        )

        val legend = known.map { (spec, segment) ->
            ContextLegendRow(
                key = spec.key,
                label = spec.label,
                tone = spec.tone,
                tokens = tokensCompact(segment.tokens),
                percent = legendPercent(segment.tokens, size),
                estimated = segment.source == CONTEXT_SOURCE_ESTIMATED,
                detail = segment.detail?.takeIf { it.isNotEmpty() },
            )
        }.toMutableList()
        for ((spec, tokens) in listOfNotNull(
            CONVERSATION?.let { it to conversation },
            FREE?.let { it to free },
        )) {
            legend += ContextLegendRow(
                key = spec.key,
                label = spec.label,
                tone = spec.tone,
                tokens = tokensCompact(tokens),
                percent = legendPercent(tokens, size),
                // Derived from the engine's own measurement — never a guess.
                estimated = false,
            )
        }

        return ContextWindowView(
            headline = AgentUsagePresentation.formatContextUsage(usage),
            percent = percent,
            severity = AgentUsagePresentation.severity(percent.toDouble()),
            // The compaction floor first: below it `exponential_sessions_compact`
            // refuses, so the tick is what makes "not yet" legible.
            ticks = listOf(
                DomainContract.contextLayoutCompactMinPercent,
                WARNING_PERCENT,
                DANGER_PERCENT,
            ),
            bar = bar,
            legend = legend,
        )
    }
}
