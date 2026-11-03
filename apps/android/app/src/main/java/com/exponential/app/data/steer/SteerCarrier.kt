package com.exponential.app.data.steer

// EXP-1529 (P2P stabilisation): the carrier rule every viewer runs
// (packages/domain-contract/fixtures/steer-enet.json `fallback` +
// `carrierCases`, TS reference `steerCarrierStep` in
// packages/domain-contract/src/steer-enet.ts). eNet is the PRIMARY carrier;
// the sealed relay tunnel is an invisible FALLBACK used only when eNet stalls,
// has no feed or drops, and eNet takes over again once it delivers.

/** fixtures/steer-enet.json `fallback` (the fixture test pins them). */
internal const val STEER_FALLBACK_STALL_MS = 2_500L
internal const val STEER_TUNNEL_MAX_DATA_BYTES = 1_572_864
internal val STEER_RELAY_RETRY_MS = listOf(1_000L, 3_000L, 10_000L, 30_000L)
internal val STEER_TUNNEL_ERROR_CODES = setOf("device_offline", "device_unsupported")
/** #EXP-1678: the STARTING REDIAL backoff (`fallback.startingRetry`). */
internal const val STEER_STARTING_RETRY_BASE_MS = 3_000L
internal const val STEER_STARTING_RETRY_MAX_MS = 30_000L

/** #EXP-1678 (`startingRetryCases`): the cap of the n-th consecutive NO FEED redial. */
internal fun steerStartingRetryCapMs(
    retries: Int,
    baseMs: Long = STEER_STARTING_RETRY_BASE_MS,
    maxMs: Long = STEER_STARTING_RETRY_MAX_MS,
): Long {
    var cap = baseMs
    repeat(retries.coerceAtLeast(0)) {
        if (cap >= maxMs) return maxMs
        cap *= 2
    }
    return cap.coerceAtMost(maxMs)
}

/** #EXP-1678: the wait itself, equal jitter in [cap/2, cap). */
internal fun steerStartingRetryDelayMs(
    retries: Int,
    baseMs: Long = STEER_STARTING_RETRY_BASE_MS,
    maxMs: Long = STEER_STARTING_RETRY_MAX_MS,
    random: Double = Math.random(),
): Long {
    val cap = steerStartingRetryCapMs(retries, baseMs, maxMs)
    return cap / 2 + (random * (cap / 2)).toLong()
}

enum class SteerActiveCarrier(val wire: String) { None("none"), Enet("enet"), Relay("relay") }

enum class SteerRelayLeg(val wire: String) { Closed("closed"), Open("open") }

data class SteerCarrierState(
    val active: SteerActiveCarrier = SteerActiveCarrier.None,
    val relay: SteerRelayLeg = SteerRelayLeg.Closed,
)

enum class SteerCarrierEvent(val wire: String) {
    Start("start"),
    Stall("stall"),
    EnetNoFeed("enetNoFeed"),
    EnetRefused("enetRefused"),
    EnetReset("enetReset"),
    EnetSynced("enetSynced"),
    EnetDropped("enetDropped"),
    RelayReset("relayReset"),
    RelaySynced("relaySynced"),
    RelayDropped("relayDropped"),
    RelayUnavailable("relayUnavailable"),
}

enum class SteerCarrierAction(val wire: String) {
    DialEnet("dialEnet"),
    ArmStall("armStall"),
    OpenRelay("openRelay"),
    RejoinRelay("rejoinRelay"),
    CloseRelay("closeRelay"),
    ReopenRelayLater("reopenRelayLater"),
    RedialEnetLater("redialEnetLater"),
    Apply("apply"),
    Drop("drop"),
}

data class SteerCarrierStep(val state: SteerCarrierState, val actions: List<SteerCarrierAction>)

/**
 * The one carrier rule: eNet first, the sealed relay only when eNet stalls
 * (`stallMs`), has no feed or drops; eNet keeps redialing while the relay is
 * active and takes over on its `activity_reset`. Frames apply only from the
 * active carrier ([SteerCarrierAction.Apply]), anything else is dropped.
 * `*Reset` = that carrier's `activity_reset`; `*Synced` = every later frame.
 */
fun steerCarrierStep(state: SteerCarrierState, event: SteerCarrierEvent): SteerCarrierStep {
    val (active, relay) = state
    fun at(a: SteerActiveCarrier, r: SteerRelayLeg, vararg actions: SteerCarrierAction) =
        SteerCarrierStep(SteerCarrierState(a, r), actions.toList())
    val none = SteerActiveCarrier.None
    val enet = SteerActiveCarrier.Enet
    val closed = SteerRelayLeg.Closed
    val open = SteerRelayLeg.Open
    return when (event) {
        SteerCarrierEvent.Start -> at(none, closed, SteerCarrierAction.DialEnet, SteerCarrierAction.ArmStall)
        SteerCarrierEvent.Stall ->
            if (active == none && relay == closed) at(none, open, SteerCarrierAction.OpenRelay) else at(active, relay)
        SteerCarrierEvent.EnetNoFeed ->
            if (active == none && relay == closed) {
                at(none, open, SteerCarrierAction.OpenRelay, SteerCarrierAction.RedialEnetLater)
            } else {
                at(active, relay, SteerCarrierAction.RedialEnetLater)
            }
        // #EXP-1678: the device itself said no; the relay reaches the same device.
        SteerCarrierEvent.EnetRefused ->
            if (active == SteerActiveCarrier.Relay) at(active, relay, SteerCarrierAction.RedialEnetLater) else at(active, relay)
        SteerCarrierEvent.EnetReset -> at(enet, relay, SteerCarrierAction.Apply)
        SteerCarrierEvent.EnetSynced -> when {
            active != enet -> at(active, relay, SteerCarrierAction.Drop)
            relay == open -> at(enet, closed, SteerCarrierAction.Apply, SteerCarrierAction.CloseRelay)
            else -> at(enet, relay, SteerCarrierAction.Apply)
        }
        SteerCarrierEvent.EnetDropped -> when {
            active != enet -> at(active, relay, SteerCarrierAction.RedialEnetLater)
            relay == open -> at(none, open, SteerCarrierAction.RejoinRelay, SteerCarrierAction.RedialEnetLater)
            else -> at(none, open, SteerCarrierAction.OpenRelay, SteerCarrierAction.RedialEnetLater)
        }
        SteerCarrierEvent.RelayReset ->
            if (active == enet) at(active, relay, SteerCarrierAction.Drop) else at(SteerActiveCarrier.Relay, relay, SteerCarrierAction.Apply)
        SteerCarrierEvent.RelaySynced ->
            if (active == SteerActiveCarrier.Relay) at(active, relay, SteerCarrierAction.Apply) else at(active, relay, SteerCarrierAction.Drop)
        SteerCarrierEvent.RelayDropped ->
            if (active == enet) at(enet, closed) else at(none, closed, SteerCarrierAction.ReopenRelayLater)
        SteerCarrierEvent.RelayUnavailable -> at(active, closed)
    }
}
