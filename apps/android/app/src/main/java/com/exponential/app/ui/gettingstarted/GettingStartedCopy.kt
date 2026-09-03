package com.exponential.app.ui.gettingstarted

/**
 * The getting-started checklist's words, byte-identical to the other three
 * clients (EXP-698 r5, Mechanism B). Web owns the canonical table in
 * `components/getting-started/getting-started-copy.ts` and its
 * `getting-started-copy.test.ts` reads THIS file, asserting every string
 * appears here verbatim — so each value must stay a plain, single-line,
 * double-quoted literal with no escapes and no non-ASCII punctuation.
 *
 * Mobile carries seven of the ten entries: the widget, helpdesk and MCP steps
 * are set up on web or the IDE, and pointing a phone at them would be a step
 * nobody can finish where they are standing.
 */
object GettingStartedCopy {
    const val DESKTOP_TITLE = "Get the desktop app"
    const val DESKTOP_DESCRIPTION = "Runs coding sessions on your machine and registers it as one of your devices."
    const val DESKTOP_ACTION = "Download the desktop app"

    const val GITHUB_TITLE = "Connect a GitHub repo"
    const val GITHUB_DESCRIPTION = "Boards attach repositories; pull requests and coding sessions flow back into issues."
    const val GITHUB_ACTION = "Connect GitHub"

    const val INVITE_TITLE = "Invite your team"
    const val INVITE_DESCRIPTION = "Teammates share boards, reviews, and the support inbox."
    const val INVITE_ACTION = "Invite in team settings"

    const val BOARD_TITLE = "Create a board"
    const val BOARD_DESCRIPTION = "Boards hold your issues; connect a repository to code on one."
    const val BOARD_ACTION = "Create a board"

    const val CODING_TITLE = "Start coding with an agent"
    const val CODING_DESCRIPTION = "Start coding on an issue hands it to your agent, which plans, implements, and opens the PR."
    const val CODING_ACTION = "Open Devices"

    const val ACTION_TITLE = "Create an action"
    const val ACTION_DESCRIPTION = "Reusable agent runs for your team, written by your agent from a description."
    const val ACTION_ACTION = "New action"

    const val SERVER_TITLE = "Set up a server"
    const val SERVER_DESCRIPTION = "Run the headless daemon on an always-on machine to take remote Start coding requests."
    const val SERVER_ACTION = "Copy install command"

    /** The section's own chrome, shared with web + the IDE. */
    const val SECTION_TITLE = "Getting started"

    fun title(key: GettingStartedEntryKey): String = when (key) {
        GettingStartedEntryKey.Desktop -> DESKTOP_TITLE
        GettingStartedEntryKey.Github -> GITHUB_TITLE
        GettingStartedEntryKey.Invite -> INVITE_TITLE
        GettingStartedEntryKey.Board -> BOARD_TITLE
        GettingStartedEntryKey.Coding -> CODING_TITLE
        GettingStartedEntryKey.Action -> ACTION_TITLE
        GettingStartedEntryKey.Server -> SERVER_TITLE
    }

    fun description(key: GettingStartedEntryKey): String = when (key) {
        GettingStartedEntryKey.Desktop -> DESKTOP_DESCRIPTION
        GettingStartedEntryKey.Github -> GITHUB_DESCRIPTION
        GettingStartedEntryKey.Invite -> INVITE_DESCRIPTION
        GettingStartedEntryKey.Board -> BOARD_DESCRIPTION
        GettingStartedEntryKey.Coding -> CODING_DESCRIPTION
        GettingStartedEntryKey.Action -> ACTION_DESCRIPTION
        GettingStartedEntryKey.Server -> SERVER_DESCRIPTION
    }

    fun action(key: GettingStartedEntryKey): String = when (key) {
        GettingStartedEntryKey.Desktop -> DESKTOP_ACTION
        GettingStartedEntryKey.Github -> GITHUB_ACTION
        GettingStartedEntryKey.Invite -> INVITE_ACTION
        GettingStartedEntryKey.Board -> BOARD_ACTION
        GettingStartedEntryKey.Coding -> CODING_ACTION
        GettingStartedEntryKey.Action -> ACTION_ACTION
        GettingStartedEntryKey.Server -> SERVER_ACTION
    }

    /**
     * One-line hint for a locked entry, keyed by the entry plus the step that
     * unlocks it. Unchanged wording from web's `lockedHint` (only the two
     * lockable mobile entries can reach here).
     */
    fun lockedHint(entry: GettingStartedEntryKey, lockedBy: GettingStartedEntryKey): String = when {
        entry == GettingStartedEntryKey.Coding && lockedBy == GettingStartedEntryKey.Desktop ->
            "Connect a machine first — coding sessions run on the desktop app or a registered server."
        entry == GettingStartedEntryKey.Coding && lockedBy == GettingStartedEntryKey.Github ->
            "Connect a GitHub repo first. Coding sessions need a repo-backed board."
        entry == GettingStartedEntryKey.Coding && lockedBy == GettingStartedEntryKey.Board ->
            "Create a board with a repository first."
        entry == GettingStartedEntryKey.Action ->
            "Connect a machine first — the action creator runs on the desktop app or a registered server."
        else -> "Complete \"${title(lockedBy)}\" first."
    }
}
