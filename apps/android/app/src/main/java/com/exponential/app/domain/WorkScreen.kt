package com.exponential.app.domain

// NEW-59: THE work screen, the display coding agents drive with computer use.
// Android mirrors `copy`, `options` and the picker keys only (the gate lives in the desktop
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

    /** #FEED-164 `work-screen.json` `pickerKeys`: the picker's string keys. */
    object PickerKeys {
        const val AUTO = "auto"
        const val DISPLAY = "id:"
    }

    /** #FEED-164: the picker key of an option value (null = Auto): Auto's
     *  key, else the display prefix + the id, so no id can read as Auto. */
    fun pickerKey(value: String?): String = value?.let { "${PickerKeys.DISPLAY}$it" } ?: PickerKeys.AUTO

    /** #FEED-164: the stored value (the bare display id, null = Auto) of a picker key. */
    fun pickedId(key: String): String? =
        key.takeIf { it.startsWith(PickerKeys.DISPLAY) }?.removePrefix(PickerKeys.DISPLAY)

    private const val SEP = " · "

    private fun displayLabel(number: Int): String =
        Copy.DISPLAY.replace("{number}", number.toString())

    /**
     * The Work screen picker's options: Auto, the reported displays, then a
     * stored display the device no longer reports.
     */
    fun options(displays: List<WorkScreenDisplay>?, stored: String?): List<WorkScreenOption> {
        val out = mutableListOf(WorkScreenOption(value = null, label = Copy.AUTO))
        for (display in displays.orEmpty()) {
            var label = "${displayLabel(display.number)}$SEP${display.width} × ${display.height}"
            if (display.primary) label += "$SEP${Copy.PRIMARY}"
            out += WorkScreenOption(value = display.id, label = label)
        }
        if (stored != null && out.none { it.value == stored }) {
            out += WorkScreenOption(value = stored, label = Copy.NOT_CONNECTED)
        }
        return out
    }
}

/**
 * One display as the device reports it in `doctor.displays`: [id] = stable
 * across reconnects and reorders (the stored value, #FEED-161), [number] =
 * 1-based position for the label only, pixels.
 */
data class WorkScreenDisplay(
    val id: String,
    val number: Int,
    val width: Int,
    val height: Int,
    val primary: Boolean,
)

/** One picker option; [value] = the stored `launch_defaults.computerUseDisplayId` (a display id), null = Auto. */
data class WorkScreenOption(val value: String?, val label: String)

/** The TS reference's name. */
fun workScreenOptions(displays: List<WorkScreenDisplay>?, stored: String?): List<WorkScreenOption> =
    WorkScreen.options(displays, stored)
