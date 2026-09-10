package com.exponential.app.domain

import androidx.lifecycle.SavedStateHandle

/**
 * EXP-825: what a play button hands the Agent page composer. Every launcher
 * entry point is NAVIGATION now — the issue detail's Start coding, the bulk
 * bar, an action's Run, New action / a suggestion, a machine's play glyph, the
 * Fix conflicts pills — and this is the preselection it carries, mirrored ×4
 * (web search params on `/t/$teamSlug/agent`, desktop
 * `Navigation::pending_chat_seed`, iOS `AgentComposerSeed`). On Android it
 * rides the `agent?…` route's nullable query args ([agentRoute] mints them,
 * [fromArgs] reads them back off the `SavedStateHandle`).
 */
data class AgentComposerSeed(
    /** Pre-checked issue ids (1 = a single-issue run, 2+ = a batch). */
    val issueIds: List<String> = emptyList(),
    /**
     * A preselected action — a team row or one of the builtins
     * (`builtin:create-action`, `builtin:fix-conflicts`). Wins over
     * [issueIds] when both arrive (the web rule).
     */
    val actionId: String? = null,
    /** The machine to preselect (the Devices tab's play glyph). */
    val deviceId: String? = null,
    /**
     * An issue linked to the open PR a `pr` input should pre-pick (ANY linked
     * issue resolves — the picker normalizes by membership, EXP-323).
     */
    val prIssueId: String? = null,
    /**
     * Text dropped into an EMPTY draft — a suggestion's description, with its
     * automation note appended when the suggestion carries a trigger.
     */
    val text: String? = null,
    /** A curated icon name seeding the Create action builtin's `icon` input. */
    val icon: String? = null,
) {
    /** Whether the seed names a subject at all. */
    val hasSubject: Boolean get() = actionId != null || issueIds.isNotEmpty()

    /**
     * The issue ids the composer should check — EMPTY when an action is seeded
     * too, because `action` wins over `issues` (web parity).
     */
    val effectiveIssueIds: List<String> get() = if (actionId == null) issueIds else emptyList()

    companion object {
        /** The Chat FAB's seed: nothing preselected. */
        val EMPTY = AgentComposerSeed()

        /**
         * The seed off the route's args, [get] answering one query key at a
         * time. Malformed issue ids are DROPPED rather than refused — a link
         * is a shortcut, not a guarantee (the web `seedFromSearch` rule).
         * Android-free on purpose: this is the unit-tested half.
         */
        fun fromArgs(get: (String) -> String?): AgentComposerSeed = AgentComposerSeed(
            issueIds = get(ARG_ISSUES).orEmpty()
                .split(',')
                .map { it.trim() }
                .filter { UUID_RE.matches(it) },
            actionId = get(ARG_ACTION)?.takeIf { it.isNotEmpty() },
            deviceId = get(ARG_DEVICE)?.takeIf { it.isNotEmpty() },
            prIssueId = get(ARG_PR)?.takeIf { UUID_RE.matches(it) },
            text = get(ARG_TEXT)?.takeIf { it.isNotEmpty() },
            icon = get(ARG_ICON)?.takeIf { it.isNotEmpty() },
        )

        /** The seed the `agent?…` route entry carries. */
        fun fromArgs(handle: SavedStateHandle): AgentComposerSeed = fromArgs { handle[it] }

        private val UUID_RE = Regex(
            "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
        )
    }
}

/** The Agent page's route root — [agentRoute] appends the seed's query. */
const val AGENT_ROUTE = "agent"

// The route's six optional query args, all nullable strings on the NavHost
// side (`AGENT_ROUTE_ARGS`). Named like the web search params so a link and a
// push read the same.
const val ARG_ISSUES = "issues"
const val ARG_ACTION = "action"
const val ARG_DEVICE = "device"
const val ARG_PR = "pr"
const val ARG_TEXT = "text"
const val ARG_ICON = "icon"

/** Every query arg the `agent` destination declares, in pattern order. */
val AGENT_ROUTE_ARGS: List<String> = listOf(ARG_ISSUES, ARG_ACTION, ARG_DEVICE, ARG_PR, ARG_TEXT, ARG_ICON)

/**
 * The NavHost pattern: every arg is a `{placeholder}` query value so an absent
 * one simply stays null (`DeepLinkRoutes.concreteRoute` fills the same shape).
 */
val AGENT_ROUTE_PATTERN: String =
    AGENT_ROUTE + "?" + AGENT_ROUTE_ARGS.joinToString("&") { "$it={$it}" }

/**
 * The concrete route a play button navigates with: `agent` alone for an empty
 * seed, else `agent?issues=a,b&action=…&device=…&pr=…&text=…&icon=…` with
 * EMPTY params omitted, so the route carries only what was picked. Values are
 * percent-encoded ([encodeRouteValue]) — the suggestion text carries newlines,
 * backticks and JSON — and Navigation decodes them back on the way in.
 */
fun agentRoute(seed: AgentComposerSeed): String {
    val params = buildList {
        if (seed.issueIds.isNotEmpty()) add(ARG_ISSUES to seed.issueIds.joinToString(","))
        seed.actionId?.takeIf { it.isNotEmpty() }?.let { add(ARG_ACTION to it) }
        seed.deviceId?.takeIf { it.isNotEmpty() }?.let { add(ARG_DEVICE to it) }
        seed.prIssueId?.takeIf { it.isNotEmpty() }?.let { add(ARG_PR to it) }
        seed.text?.takeIf { it.isNotEmpty() }?.let { add(ARG_TEXT to it) }
        seed.icon?.takeIf { it.isNotEmpty() }?.let { add(ARG_ICON to it) }
    }
    if (params.isEmpty()) return AGENT_ROUTE
    return AGENT_ROUTE + "?" + params.joinToString("&") { (key, value) -> "$key=${encodeRouteValue(value)}" }
}

/**
 * RFC 3986 percent-encoding of ONE query value: unreserved characters
 * (`A-Z a-z 0-9 - _ . ~`) pass, everything else (including `,`, `&`, `=`, `+`
 * and every non-ASCII byte of the UTF-8 form) becomes `%XX`. Hand-rolled
 * rather than `Uri.encode` so the mint is JVM-testable, and never
 * `URLEncoder`, whose `+` for a space the navigation decoder would keep.
 */
fun encodeRouteValue(value: String): String {
    val out = StringBuilder(value.length + 16)
    for (byte in value.toByteArray(Charsets.UTF_8)) {
        val c = byte.toInt() and 0xFF
        val ch = c.toChar()
        if (ch in 'A'..'Z' || ch in 'a'..'z' || ch in '0'..'9' || ch == '-' || ch == '_' || ch == '.' || ch == '~') {
            out.append(ch)
        } else {
            out.append('%')
            out.append(HEX[c shr 4])
            out.append(HEX[c and 0x0F])
        }
    }
    return out.toString()
}

private const val HEX = "0123456789ABCDEF"
