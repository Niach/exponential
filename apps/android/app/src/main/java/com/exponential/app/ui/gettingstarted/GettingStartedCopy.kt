package com.exponential.app.ui.gettingstarted

/**
 * EXP-1270 (EXP-1259 D6): the Getting started block's words, the fixture's
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

    const val CARD_WIDTH = 104
    const val CARD_HEIGHT = 112
    const val CARD_PADDING = 12
    const val CARD_ICON = 16
    const val CARD_GAP = 8

    const val BLOCK_TEST_ID = "getting-started"
    const val CARD_TEST_ID = "getting-started-card-{key}"
    const val DOCS_TEST_ID = "getting-started-docs"
    const val ENTRY_TEST_ID = "getting-started-entry"

    fun progress(done: Int, total: Int): String =
        PROGRESS.replace("{done}", done.toString()).replace("{total}", total.toString())

    fun cardTestId(key: GettingStartedStepKey): String = CARD_TEST_ID.replace("{key}", key.wire)

    // The device-setup block's copy (DeviceSetup.kt).
    const val DESKTOP_TITLE = "Get the desktop app"
    const val DESKTOP_DESCRIPTION = "Runs agents on your machine and registers it as one of your devices."
    const val DESKTOP_ACTION = "Download the desktop app"
    const val SERVER_TITLE = "Set up a server"
    const val SERVER_DESCRIPTION = "Run the headless daemon on an always-on machine to take remote Start coding requests."
}
