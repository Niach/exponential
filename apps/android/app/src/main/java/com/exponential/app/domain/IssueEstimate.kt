package com.exponential.app.domain

import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// EXP-630: story points, rendered on the TEAM's scale (`teams.estimation_type`,
// contract `issueEstimation`). The stored value is always a point number, so
// a team can switch scales without touching a single issue; `none` hides the
// chip everywhere. T-shirt sizes are the fibonacci points worn as XS…XL,
// exactly how Linear stores them. Mirrors web `lib/issue-estimate.ts`; locked
// ×4 by `domain-contract/fixtures/issue-estimate.json` (IssueEstimateTest).

/** The ladders per scale, in the contract's order (`none` has none). */
val ISSUE_ESTIMATION_SCALES: Map<String, List<Int>> = linkedMapOf(
    DomainContract.issueEstimationExponential to listOf(1, 2, 4, 8, 16),
    DomainContract.issueEstimationFibonacci to listOf(1, 2, 3, 5, 8),
    DomainContract.issueEstimationLinear to listOf(1, 2, 3, 4, 5),
    DomainContract.issueEstimationTshirt to listOf(1, 2, 3, 5, 8),
)

/** The t-shirt ladder's sizes, index-aligned with its points. */
val ISSUE_ESTIMATE_TSHIRT_LABELS: List<String> = listOf("XS", "S", "M", "L", "XL")

const val NO_ESTIMATE = "No estimate"

/** The ladder of one scale; an unknown scale (or `none`) offers nothing. */
fun estimationScale(type: String): List<Int> = ISSUE_ESTIMATION_SCALES[type] ?: emptyList()

private fun tshirtLabel(value: Int): String? {
    val index = ISSUE_ESTIMATION_SCALES.getValue(DomainContract.issueEstimationTshirt).indexOf(value)
    return if (index == -1) null else ISSUE_ESTIMATE_TSHIRT_LABELS[index]
}

/** "No estimate", "M", "1 point", "5 points". */
fun estimateLabel(value: Int?, scale: String = DomainContract.issueEstimationFibonacci): String {
    if (value == null) return NO_ESTIMATE
    if (scale == DomainContract.issueEstimationTshirt) {
        tshirtLabel(value)?.let { return it }
    }
    return if (value == 1) "1 point" else "$value points"
}

/** The chip form: "M" on the t-shirt scale, "5 pt" elsewhere. */
fun estimateShortLabel(value: Int, scale: String = DomainContract.issueEstimationFibonacci): String {
    if (scale == DomainContract.issueEstimationTshirt) {
        tshirtLabel(value)?.let { return it }
    }
    return "$value pt"
}

/**
 * The values a picker offers: the scale's ladder plus the current value when
 * it sits off the ladder (an import from another scale, a value set before
 * the scale was switched), ascending — so the trigger always names a listed
 * option. The "No estimate" row is the caller's.
 */
fun estimatePickerValues(current: Int?, scale: String): List<Int> {
    val values = estimationScale(scale).toMutableSet()
    if (current != null && current >= 0) values.add(current)
    return values.sorted()
}

/** The `estimate_changed` timeline phrase, on the team's scale. */
fun estimateEventPhrase(
    payload: JsonObject?,
    scale: String = DomainContract.issueEstimationFibonacci,
): String {
    val to = estimatePayloadValue(payload?.get("to")) ?: return "removed the estimate"
    return "set the estimate to ${estimateLabel(to, scale)}"
}

// A payload side as a point value: a JSON number, or a numeric string (an
// older writer's stringified payload); null/absent/blank/NaN = no estimate.
private fun estimatePayloadValue(raw: kotlinx.serialization.json.JsonElement?): Int? {
    if (raw == null || raw is JsonNull) return null
    val content = (raw as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() } ?: return null
    content.toIntOrNull()?.let { return it }
    return content.toDoubleOrNull()?.takeIf { it.isFinite() }?.toInt()
}
