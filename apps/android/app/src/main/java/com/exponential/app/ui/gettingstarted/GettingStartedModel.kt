package com.exponential.app.ui.gettingstarted

// EXP-1270 (EXP-1259 D6): Getting started ×4 = ONE block of square cards in
// one linear order. The pure model, a port of web
// `components/getting-started/getting-started-model.ts`
// (`deriveGettingStarted`), locked to
// `packages/domain-contract/fixtures/getting-started.json` by
// GettingStartedModelTest (which replays its `cases`, whole objects). Free of
// Room, Hilt and Compose.

/** The eight steps, in the fixture's `steps` order. */
enum class GettingStartedStepKey(val wire: String) {
    Board("board"),
    Github("github"),
    Device("device"),
    Run("run"),
    Invite("invite"),
    Server("server"),
    Mcp("mcp"),
    Widget("widget"),
}

/** What a card's tap opens (fixture `target`). */
enum class GettingStartedTarget(val wire: String) {
    CreateBoard("create-board"),
    Github("github"),
    AddDevice("add-device"),
    Agent("agent"),
    Invite("invite"),
    AddServer("add-server"),
    Mcp("mcp"),
    Widget("widget"),
}

enum class GettingStartedPlatform(val wire: String) { Pointer("pointer"), Phone("phone") }

/** Everything the derivation reads. */
data class GettingStartedSignals(
    /** Any live (non-trashed) board in the team. */
    val hasBoard: Boolean = false,
    /** integrations.github.status → installed. */
    val githubInstalled: Boolean = false,
    /** An own `devices` row of kind `desktop`. */
    val hasDesktopDevice: Boolean = false,
    /** An own `devices` row of kind `server`. */
    val hasServerDevice: Boolean = false,
    /** Any `coding_sessions` row in the team, running or ended. */
    val hasCodingSession: Boolean = false,
    /** More than one member, or any open invite. */
    val hasInvitedTeam: Boolean = false,
    /** A connected team MCP server (web + IDE only). */
    val mcpConnected: Boolean = false,
    /** A widget configured (web + IDE only). */
    val hasWidget: Boolean = false,
)

/** The owner-only surfaces' gates (fixture `visibleWhen`). */
data class GettingStartedPermissions(
    val canManageMembers: Boolean = false,
    val canManageWidgets: Boolean = false,
)

/** One step of the fixture's `steps`. */
data class GettingStartedStep(
    val key: GettingStartedStepKey,
    val title: String,
    /** A concept id (packages/icons/icons.json). */
    val icon: String,
    val target: GettingStartedTarget,
    val needsDevice: Boolean = false,
    /** False = never on iOS/Android. */
    val phones: Boolean = true,
    /** Null = everyone; else the permission it needs. */
    val visibleWhen: String? = null,
)

data class GettingStartedEntry(
    val key: GettingStartedStepKey,
    val done: Boolean,
    val target: GettingStartedTarget,
    /** The first step not done — the highlighted card. */
    val next: Boolean,
)

data class GettingStartedModel(
    val entries: List<GettingStartedEntry>,
    val done: Int,
    val total: Int,
    val progress: String,
    val complete: Boolean,
)

object GettingStarted {
    val STEPS: List<GettingStartedStep> = listOf(
        GettingStartedStep(GettingStartedStepKey.Board, "Create a board", "ui-folder", GettingStartedTarget.CreateBoard),
        GettingStartedStep(GettingStartedStepKey.Github, "Connect GitHub", "ui-github", GettingStartedTarget.Github),
        GettingStartedStep(GettingStartedStepKey.Device, "Add a device", "ui-device", GettingStartedTarget.AddDevice),
        GettingStartedStep(
            GettingStartedStepKey.Run, "Start a session", "action-run", GettingStartedTarget.Agent,
            needsDevice = true,
        ),
        GettingStartedStep(
            GettingStartedStepKey.Invite, "Invite your team", "ui-invite", GettingStartedTarget.Invite,
            visibleWhen = "canManageMembers",
        ),
        GettingStartedStep(GettingStartedStepKey.Server, "Set up a server", "ui-server", GettingStartedTarget.AddServer),
        GettingStartedStep(
            GettingStartedStepKey.Mcp, "Connect your tools", "ui-mcp", GettingStartedTarget.Mcp,
            phones = false,
        ),
        GettingStartedStep(
            GettingStartedStepKey.Widget, "Feedback widget", "ui-widget", GettingStartedTarget.Widget,
            phones = false, visibleWhen = "canManageWidgets",
        ),
    )

    fun step(key: GettingStartedStepKey): GettingStartedStep = STEPS.first { it.key == key }

    private fun done(key: GettingStartedStepKey, s: GettingStartedSignals): Boolean = when (key) {
        GettingStartedStepKey.Board -> s.hasBoard
        GettingStartedStepKey.Github -> s.githubInstalled
        GettingStartedStepKey.Device -> s.hasDesktopDevice || s.hasServerDevice
        GettingStartedStepKey.Run -> s.hasCodingSession
        GettingStartedStepKey.Invite -> s.hasInvitedTeam
        GettingStartedStepKey.Server -> s.hasServerDevice
        GettingStartedStepKey.Mcp -> s.mcpConnected
        GettingStartedStepKey.Widget -> s.hasWidget
    }

    private fun visible(step: GettingStartedStep, p: GettingStartedPermissions, platform: GettingStartedPlatform): Boolean {
        if (platform == GettingStartedPlatform.Phone && !step.phones) return false
        return when (step.visibleWhen) {
            null -> true
            "canManageMembers" -> p.canManageMembers
            "canManageWidgets" -> p.canManageWidgets
            else -> false
        }
    }

    /**
     * The block for [signals]: the visible steps in order, each done or not,
     * its target (a step needing a device with none opens the device step),
     * the first open one marked `next`; complete once every visible step is
     * done.
     */
    fun derive(
        signals: GettingStartedSignals,
        permissions: GettingStartedPermissions,
        platform: GettingStartedPlatform,
    ): GettingStartedModel {
        val hasDevice = signals.hasDesktopDevice || signals.hasServerDevice
        val rows = STEPS.filter { visible(it, permissions, platform) }.map { step ->
            val target = if (step.needsDevice && !hasDevice) GettingStartedTarget.AddDevice else step.target
            Triple(step.key, done(step.key, signals), target)
        }
        val nextIndex = rows.indexOfFirst { !it.second }
        val entries = rows.mapIndexed { index, (key, done, target) ->
            GettingStartedEntry(key = key, done = done, target = target, next = index == nextIndex)
        }
        val doneCount = entries.count { it.done }
        return GettingStartedModel(
            entries = entries,
            done = doneCount,
            total = entries.size,
            progress = GettingStartedCopy.progress(doneCount, entries.size),
            complete = entries.isNotEmpty() && doneCount == entries.size,
        )
    }
}

/**
 * What the block renders. [loading] = the signals have not all resolved yet;
 * the block stays hidden rather than flipping a wrong count a moment later.
 */
data class GettingStartedState(
    val model: GettingStartedModel? = null,
    val loading: Boolean = true,
) {
    val complete: Boolean get() = !loading && model?.complete == true

    /** The block (and the entry) shows once loaded and until complete. */
    val visible: Boolean get() = !loading && model != null && !model.complete && model.entries.isNotEmpty()
}
