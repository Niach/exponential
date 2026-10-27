package com.exponential.app.domain

import kotlin.math.floor
import kotlin.math.pow
import kotlin.math.roundToLong

// EXP-1275 / EXP-1271 (EXP-1259 D8): THE feedback-states system, the pure
// half. Hand-copied from `packages/domain-contract/fixtures/feedback-states.json`
// and locked to it by FeedbackMarkTest; the Compose half (spinner, pending
// button, skeleton rows, the mark, the blocking overlay) lives in
// `ui/components/Feedback.kt` and only draws what this says. Web reference:
// `packages/domain-contract/src/feedback-mark.ts`.

/** A visible part of one stripe's cut, in the cubic's parameter t (0..1). */
data class MarkSpan(val from: Double, val to: Double)

/** One of the logo's three cut stripes: a cubic in its 100×100 grid. */
data class MarkStripe(
    /** p0, p1, p2, p3 as (x, y) pairs. */
    val cubic: List<Pair<Double, Double>>,
    /** The loop phase window the cut draws in over. */
    val draw: Pair<Double, Double>,
    /** The loop phase window the cut erases over. */
    val erase: Pair<Double, Double>,
)

object FeedbackStates {
    const val SPINNER_TEST_ID = "feedback-spinner"
    const val PENDING_TEST_ID = "feedback-pending"
    const val SKELETON_TEST_ID = "feedback-skeleton"
    const val MARK_TEST_ID = "feedback-mark"
    const val BLOCKING_TEST_ID = "feedback-blocking"
    const val STARTING_TEST_ID = "composer-starting"

    /** (1) INLINE: the spinner's px and its gap to the progressive word. */
    const val INLINE_SPINNER = 10
    const val INLINE_GAP = 6
    const val WORD_SAVING = "Saving"
    const val WORD_SAVED = "Saved"
    const val WORD_LOADING = "Loading"

    /** (2) PENDING: nothing changes before the delay, so a fast action never flashes. */
    const val PENDING_DELAY_MS = 150L
    const val PENDING_SPINNER = 12
    val PENDING_WORDS: Map<String, String> = mapOf(
        "start" to "Starting",
        "connect" to "Connecting",
        "save" to "Saving",
        "delete" to "Deleting",
        "send" to "Sending",
        "merge" to "Merging",
        "invite" to "Inviting",
    )

    /** (3) PLACEHOLDER: skeleton rows in the real row geometry. */
    const val PLACEHOLDER_ROWS = 3
    const val PLACEHOLDER_BAR_HEIGHT = 8
    const val PLACEHOLDER_RADIUS = 4
    const val PLACEHOLDER_DOT = 12
    const val PLACEHOLDER_PULSE_MS = 1600
    const val PLACEHOLDER_OPACITY_LOW = 0.45
    const val PLACEHOLDER_OPACITY_HIGH = 0.9
    const val PLACEHOLDER_REDUCED_OPACITY = 0.6

    /** (4) THE MARK. */
    const val MARK_LOOP_MS = 1200
    const val MARK_SAMPLES = 24
    const val MARK_STRIPE_WIDTH = 6.0
    const val MARK_SIZE_INLINE = 16
    const val MARK_SIZE_STATUS = 16
    const val MARK_SIZE_STYLEGUIDE = 40

    /**
     * The logo's three stripes, lower-left first. ONE copy of the geometry on
     * Android: the Compose mark ([com.exponential.app.ui.components.ExponentialMark]
     * and the animated FeedbackMark) paints from these; only the splash
     * drawable (an OS resource) keeps its own copy.
     */
    val STRIPES: List<MarkStripe> = listOf(
        MarkStripe(
            cubic = listOf(-5.87 to 62.01, 39.09 to 65.44, 48.72 to 28.71, 49.03 to -6.21),
            draw = 0.0 to 0.3,
            erase = 0.55 to 0.8,
        ),
        MarkStripe(
            cubic = listOf(-5.07 to 86.0, 53.78 to 84.42, 71.13 to 37.29, 73.0 to -5.09),
            draw = 0.12 to 0.42,
            erase = 0.65 to 0.9,
        ),
        MarkStripe(
            cubic = listOf(-4.27 to 109.99, 68.46 to 103.4, 93.55 to 45.86, 96.98 to -3.98),
            draw = 0.24 to 0.54,
            erase = 0.75 to 1.0,
        ),
    )

    /** (5) BLOCKING: switching team or account, signing out. */
    const val BLOCKING_BACKDROP_OPACITY = 0.85
    const val BLOCKING_MARK_SIZE = 40
    const val BLOCKING_GAP = 16
    const val BLOCKING_SWITCH_TEAM = "Switching to {team}"
    const val BLOCKING_SWITCH_ACCOUNT = "Switching to {account}"
    const val BLOCKING_SIGN_OUT = "Signing out"

    fun switchingTeam(team: String): String = BLOCKING_SWITCH_TEAM.replace("{team}", team)
    fun switchingAccount(account: String): String = BLOCKING_SWITCH_ACCOUNT.replace("{account}", account)

    /** The composer after Start, until the run's row lands. */
    const val STARTING_LABEL = "Starting on {device}"
    const val STARTING_TIMEOUT_MS = 30_000L
    /** How long the watch keeps looking for a late row after the send. */
    const val STARTING_WATCH_MS = 1_800_000L
    /** The server and device dedupe a (deviceId, startId) this long. */
    const val STARTING_DEDUPE_MS = 1_800_000L
    const val STARTING_TIMEOUT_TITLE = "The session did not start"
    const val STARTING_TIMEOUT_DESCRIPTION = "{device} did not answer."
    const val RETRY = "Retry"

    fun startingLabel(device: String): String = STARTING_LABEL.replace("{device}", device)
    fun startingTimeoutDescription(device: String): String =
        STARTING_TIMEOUT_DESCRIPTION.replace("{device}", device)

    /** (6) ERRORS: `Couldn't {verb} {thing}`. */
    const val ERROR_TITLE_PATTERN = "Couldn't {verb} {thing}"

    fun errorTitle(verb: String, thing: String): String =
        ERROR_TITLE_PATTERN.replace("{verb}", verb).replace("{thing}", thing)

    /**
     * EXP-1391 + EXP-1378 (`errors.mergeRefusal`): the check gate's refusal
     * code (HTTP 428) and the override its toast offers instead of Retry
     * (`pullRequests.merge({force: true})`).
     */
    const val MERGE_CHECKS_CODE = "PRECONDITION_REQUIRED"
    const val MERGE_ANYWAY = "Merge anyway"

    /** `errors.mergeRefusal.handoff`: ends the server's message once the refusal went into the thread. */
    const val MERGE_HANDOFF = "Sent to the thread."

    /**
     * Where a failure renders, by who started the work (the fixture's `kind`
     * -> `render`): `action` = toast, `input` = inline, `background` (sync,
     * polling, a load on open or focus; anything unknown) = quiet, never a toast.
     */
    fun errorSurface(kind: String): String = when (kind) {
        "action" -> "toast"
        "input" -> "inline"
        else -> "quiet"
    }

    private fun easeInOutCubic(x: Double): Double =
        if (x < 0.5) 4 * x * x * x else 1 - (-2 * x + 2).pow(3) / 2

    private fun round4(value: Double): Double = (value * 10_000).roundToLong() / 10_000.0

    /**
     * The visible span of stripe [index] (0 = lower-left) at loop [phase]
     * (0..1, wrapping): the cut DRAWS IN from t=0 during its draw window, stays
     * whole, then ERASES from t=0 during its erase window, and is absent
     * outside them, so the loop starts and ends on the plain disc. Rounded to 4
     * decimals (TS `markStripeSpan`).
     */
    fun markStripeSpan(index: Int, phase: Double): MarkSpan? {
        val stripe = STRIPES.getOrNull(index) ?: return null
        val p = phase - floor(phase)
        val (drawFrom, drawTo) = stripe.draw
        val (eraseFrom, eraseTo) = stripe.erase
        if (p < drawFrom || p >= eraseTo) return null
        if (p < drawTo) return MarkSpan(0.0, round4(easeInOutCubic((p - drawFrom) / (drawTo - drawFrom))))
        if (p < eraseFrom) return MarkSpan(0.0, 1.0)
        return MarkSpan(round4(easeInOutCubic((p - eraseFrom) / (eraseTo - eraseFrom))), 1.0)
    }

    /** The static logo: every stripe whole (reduce motion, the plain mark). */
    val STATIC_SPANS: List<MarkSpan?> = STRIPES.map { MarkSpan(0.0, 1.0) }

    /** All three spans at [phase]. */
    fun markSpans(phase: Double): List<MarkSpan?> = STRIPES.indices.map { markStripeSpan(it, phase) }

    /** A point on stripe [index]'s cubic at parameter [t], in the 100×100 grid. */
    fun markStripePoint(index: Int, t: Double): Pair<Double, Double> {
        val (p0, p1, p2, p3) = STRIPES[index].cubic
        val u = 1 - t
        val a = u * u * u
        val b = 3 * u * u * t
        val c = 3 * u * t * t
        val d = t * t * t
        return Pair(
            a * p0.first + b * p1.first + c * p2.first + d * p3.first,
            a * p0.second + b * p1.second + c * p2.second + d * p3.second,
        )
    }
}
