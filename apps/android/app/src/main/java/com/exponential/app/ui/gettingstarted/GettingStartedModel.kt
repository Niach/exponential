package com.exponential.app.ui.gettingstarted

// The pure state model behind the getting-started checklist — a 1:1 port of
// web's `components/getting-started/getting-started-model.ts`
// (`deriveEntryStates`) minus the three web-only entries (widget, helpdesk,
// mcp), which mobile does not show. Kept free of Room, Hilt and Compose so
// the order / lock / done rules are unit tested without a device.

/** The seven steps mobile shows, in their single static display order. */
enum class GettingStartedEntryKey {
    Desktop,
    Github,
    Invite,
    Board,
    Coding,
    Action,
    Server,
}

enum class GettingStartedEntryState { Done, Available, Locked }

/** Everything the derivation reads — all of it off Electric except [githubInstalled]. */
data class GettingStartedSignals(
    /** An own `devices` row of kind `desktop` — the IDE registered. */
    val hasDesktopDevice: Boolean = false,
    /** An own `devices` row of kind `server` — the CLI daemon registered. */
    val hasServerDevice: Boolean = false,
    /** integrations.github.status → installed (the team has a linked App install). */
    val githubInstalled: Boolean = false,
    /** More than one member, or any open invite. */
    val hasInvitedTeam: Boolean = false,
    /** Any live (non-trashed) board in the team. */
    val hasBoard: Boolean = false,
    /** Any live board with a repository attached. */
    val hasRepoBoard: Boolean = false,
    /** Any `coding_sessions` row in the team, running or ended. */
    val hasCodingSession: Boolean = false,
    /** Any synced `actions` row (the two builtins are not rows). */
    val hasAction: Boolean = false,
)

data class GettingStartedEntry(
    val key: GettingStartedEntryKey,
    val state: GettingStartedEntryState,
    /** For a locked entry: the step whose completion unlocks it. */
    val lockedBy: GettingStartedEntryKey? = null,
)

/**
 * Derive every entry's state in the fixed order desktop → github → invite →
 * board → coding → action → server. Completion always wins over locking (a
 * signal that exists proves the prerequisite was satisfiable).
 *
 * The natives have no `canManageMembers` capability of their own, so the two
 * owner-gated web entries — invite (teamInvites.create is owner-only) and
 * action (action writes are owner-only) — gate on [isOwner]; a non-owner
 * neither sees them nor counts them in the total.
 */
fun deriveGettingStartedEntries(
    signals: GettingStartedSignals,
    isOwner: Boolean,
): List<GettingStartedEntry> {
    val entries = mutableListOf<GettingStartedEntry>()

    entries += GettingStartedEntry(
        GettingStartedEntryKey.Desktop,
        if (signals.hasDesktopDevice) GettingStartedEntryState.Done else GettingStartedEntryState.Available,
    )
    entries += GettingStartedEntry(
        GettingStartedEntryKey.Github,
        if (signals.githubInstalled) GettingStartedEntryState.Done else GettingStartedEntryState.Available,
    )
    if (isOwner) {
        entries += GettingStartedEntry(
            GettingStartedEntryKey.Invite,
            if (signals.hasInvitedTeam) GettingStartedEntryState.Done else GettingStartedEntryState.Available,
        )
    }
    entries += GettingStartedEntry(
        GettingStartedEntryKey.Board,
        if (signals.hasBoard) GettingStartedEntryState.Done else GettingStartedEntryState.Available,
    )

    // Coding needs a repo-backed board AND a machine to run on; when locked,
    // point at whichever feeder step is still missing, in display order
    // (desktop first — without a machine nothing can run; then GitHub, without
    // which the board step cannot attach a repo either).
    val hasDevice = signals.hasDesktopDevice || signals.hasServerDevice
    entries += when {
        signals.hasCodingSession ->
            GettingStartedEntry(GettingStartedEntryKey.Coding, GettingStartedEntryState.Done)
        signals.hasRepoBoard && hasDevice ->
            GettingStartedEntry(GettingStartedEntryKey.Coding, GettingStartedEntryState.Available)
        else -> GettingStartedEntry(
            GettingStartedEntryKey.Coding,
            GettingStartedEntryState.Locked,
            lockedBy = when {
                !hasDevice -> GettingStartedEntryKey.Desktop
                signals.githubInstalled -> GettingStartedEntryKey.Board
                else -> GettingStartedEntryKey.Github
            },
        )
    }

    // EXP-548: actions are authored by the builtin creator run, which — like
    // any coding session — needs a machine; the desktop step is the feeder.
    if (isOwner) {
        entries += when {
            signals.hasAction ->
                GettingStartedEntry(GettingStartedEntryKey.Action, GettingStartedEntryState.Done)
            hasDevice ->
                GettingStartedEntry(GettingStartedEntryKey.Action, GettingStartedEntryState.Available)
            else -> GettingStartedEntry(
                GettingStartedEntryKey.Action,
                GettingStartedEntryState.Locked,
                lockedBy = GettingStartedEntryKey.Desktop,
            )
        }
    }

    entries += GettingStartedEntry(
        GettingStartedEntryKey.Server,
        if (signals.hasServerDevice) GettingStartedEntryState.Done else GettingStartedEntryState.Available,
    )

    return entries
}

/**
 * What the cards render. [loading] means the signals have not all resolved
 * yet — it counts as incomplete-unknown, so the section stays up.
 */
data class GettingStartedState(
    val entries: List<GettingStartedEntry> = emptyList(),
    val loading: Boolean = true,
) {
    val done: Int get() = entries.count { it.state == GettingStartedEntryState.Done }
    val total: Int get() = entries.size

    /**
     * EXP-548: the checklist has no dismissal — it simply disappears once
     * every visible entry is done.
     */
    val complete: Boolean get() = !loading && total > 0 && done == total
}
