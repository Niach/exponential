package com.exponential.app.domain

// NEW-59: THE work screen, the display coding agents drive with computer use.
// Android mirrors `copy` and `options` only (the gate lives in the desktop
// computer crate). A pure mirror of `packages/domain-contract/src/work-screen.ts`
// (`workScreenOptions`), byte-locked by `fixtures/work-screen.json`
// (`optionCases`, replayed by WorkScreenTest). The fixture's `$comment` holds
// the rules.

object WorkScreen {

    /** `work-screen.json` `copy` (the refusals stay on the desktop), byte-identical ×4. */
    object Copy {
        const val ROW = "Work screen"
        const val AUTO = "Auto"
        const val DISPLAY = "Display {number}"
        const val PRIMARY = "Primary"
        const val NOT_CONNECTED = "Not connected"

        /** Fixture key → constant, for the replay test. */
        val all: Map<String, String> = mapOf(
            "row" to ROW,
            "auto" to AUTO,
            "display" to DISPLAY,
            "primary" to PRIMARY,
            "notConnected" to NOT_CONNECTED,
        )
    }

    /** `work-screen.json` `testIds.row`: the device settings picker row. */
    const val ROW_TEST_ID = "device-settings-work-screen"

    private const val SEP = " · "

    private fun displayLabel(number: Int): String =
        Copy.DISPLAY.replace("{number}", number.toString())

    /**
     * The Work screen picker's options: Auto, the reported displays, then a
     * stored display the device no longer reports.
     */
    fun options(displays: List<WorkScreenDisplay>?, stored: Int?): List<WorkScreenOption> {
        val out = mutableListOf(WorkScreenOption(value = null, label = Copy.AUTO))
        for (display in displays.orEmpty()) {
            var label = "${displayLabel(display.number)}$SEP${display.width} × ${display.height}"
            if (display.primary) label += "$SEP${Copy.PRIMARY}"
            out += WorkScreenOption(value = display.number, label = label)
        }
        if (stored != null && out.none { it.value == stored }) {
            out += WorkScreenOption(
                value = stored,
                label = "${displayLabel(stored)}$SEP${Copy.NOT_CONNECTED}",
            )
        }
        return out
    }
}

/** One display as the device reports it in `doctor.displays` (1-based [number], pixels). */
data class WorkScreenDisplay(
    val number: Int,
    val width: Int,
    val height: Int,
    val primary: Boolean,
)

/** One picker option; [value] = the stored `launch_defaults.computerUseDisplay`, null = Auto. */
data class WorkScreenOption(val value: Int?, val label: String)

/** The TS reference's name. */
fun workScreenOptions(displays: List<WorkScreenDisplay>?, stored: Int?): List<WorkScreenOption> =
    WorkScreen.options(displays, stored)
