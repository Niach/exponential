package com.exponential.app.ui.gettingstarted

/**
 * EXP-1270 (EXP-1259 D6): the Getting started block's words and row
 * geometry, the fixture's
 * (`packages/domain-contract/fixtures/getting-started.json`) — locked by
 * GettingStartedCopyTest, which reads the fixture. The step titles live on
 * [GettingStarted.STEPS].
 *
 * The DESKTOP_* / SERVER_* lines are the ONE device-setup block's
 * ([com.exponential.app.ui.components.DeviceSetup]) — the setup steps the
 * old checklist shared with it, kept here because that block still reads
 * them.
 */
object GettingStartedCopy {
    const val TITLE = "Getting started"
    const val PROGRESS = "{done} of {total}"
    const val METER = 160
    const val DOCS_LABEL = "Docs"
    const val DOCS_ICON = "ui-docs"
    const val DOCS_URL = "https://exponential.at/docs/getting-started/"

    /** The expanded next row's trailing label. */
    const val NEXT_LABEL = "Next"

    /** The block's widest (the dialog's width); the phone sheet is full width. */
    const val MAX_WIDTH = 560
    const val ROW_HEIGHT = 44
    const val ROW_HEIGHT_PHONE = 48
    const val ROW_ICON = 16
    const val ROW_PADDING = 16
    const val ROW_EXPANDED_PADDING = 14

    const val BLOCK_TEST_ID = "getting-started"
    const val ROW_TEST_ID = "getting-started-row-{key}"
    const val CTA_TEST_ID = "getting-started-cta"
    const val DOCS_TEST_ID = "getting-started-docs"
    const val ENTRY_TEST_ID = "getting-started-entry"

    fun progress(done: Int, total: Int): String =
        PROGRESS.replace("{done}", done.toString()).replace("{total}", total.toString())

    fun rowTestId(key: GettingStartedStepKey): String = ROW_TEST_ID.replace("{key}", key.wire)

    /** The expanded row's one sentence (`steps[].why`). */
    fun why(key: GettingStartedStepKey): String = when (key) {
        GettingStartedStepKey.Board -> "A board holds the issues your agents work on."
        GettingStartedStepKey.Github -> "Sessions branch, push and open pull requests on your repositories."
        GettingStartedStepKey.Device -> "Sessions run on your computer. The desktop app registers it."
        GettingStartedStepKey.Run -> "Pick an issue or type what to build, then watch it work."
        GettingStartedStepKey.Invite -> "Teammates see every board, session and review."
        GettingStartedStepKey.Server -> "An always-on machine takes sessions while your laptop is closed."
        GettingStartedStepKey.Mcp -> "Claude, ChatGPT or Cursor read and file issues through MCP."
        GettingStartedStepKey.Widget -> "A snippet on your site turns feedback into issues."
    }

    /** The expanded row's button label (`steps[].cta`). */
    fun cta(key: GettingStartedStepKey): String = when (key) {
        GettingStartedStepKey.Board -> "Create a board"
        GettingStartedStepKey.Github -> "Connect GitHub"
        GettingStartedStepKey.Device -> "Add a device"
        GettingStartedStepKey.Run -> "Start a session"
        GettingStartedStepKey.Invite -> "Invite your team"
        GettingStartedStepKey.Server -> "Set up a server"
        GettingStartedStepKey.Mcp -> "Connect your tools"
        GettingStartedStepKey.Widget -> "Feedback widget"
    }

    // The device-setup block's copy (DeviceSetup.kt).
    const val DESKTOP_TITLE = "Get the desktop app"
    const val DESKTOP_DESCRIPTION = "Runs agents on your machine and registers it as one of your devices."
    const val DESKTOP_ACTION = "Download the desktop app"
    const val SERVER_TITLE = "Set up a server"
    const val SERVER_DESCRIPTION = "Run the headless daemon on an always-on machine to take remote Start coding requests."
}
