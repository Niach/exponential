package com.exponential.app.domain

// NEW-59: THE work screen, the display coding agents drive with computer use.
// Android mirrors `copy`, `options` and the picker keys only (the gate lives in the desktop
// epilot crate). A pure mirror of `packages/domain-contract/src/work-screen.ts`
// (`workScreenOptions`), byte-locked by `fixtures/work-screen.json`
// (`optionCases`, replayed by WorkScreenTest). The fixture's `$comment` holds
// the rules. #EXP-1514: the agent space (`space`, `spaceOffered`;
// `spaceOptionCases`, `spaceKeyCases`).

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

    /**
     * #EXP-1514 `work-screen.json` `space`: the reserved pick [VALUE] (agents
     * work in an agent space only, never on this device's screen) and the
     * picker's words for it, kept apart from [Copy] (WorkScreenTest locks
     * that key set).
     */
    object Space {
        const val VALUE = "space"
        const val LABEL = "Agent space"
        const val ONLY = "Agent space only"

        /** Fixture key → constant, for the replay test. */
        val all: Map<String, String> = mapOf(
            "value" to VALUE,
            "label" to LABEL,
            "only" to ONLY,
        )
    }

    /** `work-screen.json` `testIds.row`: the device settings picker row. */
    const val ROW_TEST_ID = "device-settings-work-screen"

    /** #FEED-164 `work-screen.json` `pickerKeys`: the picker's string keys. */
    object PickerKeys {
        const val AUTO = "auto"
        const val DISPLAY = "id:"
        /** #EXP-1514: the agent space's key. */
        const val SPACE = "space"
    }

    /** #FEED-164: the picker key of an option value (null = Auto): Auto's
     *  key, #EXP-1514 the agent space's key, else the display prefix + the
     *  id, so no id can read as Auto or as the space. */
    fun pickerKey(value: String?): String = when (value) {
        null -> PickerKeys.AUTO
        Space.VALUE -> PickerKeys.SPACE
        else -> "${PickerKeys.DISPLAY}$value"
    }

    /** #FEED-164: the stored value (the bare display id, #EXP-1514 the
     *  space's value, null = Auto) of a picker key. */
    fun pickedId(key: String): String? = when {
        key == PickerKeys.SPACE -> Space.VALUE
        key.startsWith(PickerKeys.DISPLAY) -> key.removePrefix(PickerKeys.DISPLAY)
        else -> null
    }

    /** device-doctor.json's agent space row (#EXP-1514). */
    private const val AGENT_SPACE_ITEM = "agent_space"

    /**
     * #EXP-1514: whether the device offers an agent space, read by the
     * picker: its doctor carries the `agent_space` item in state `ok`. No
     * doctor (an older build) = not offered.
     */
    fun spaceOffered(doctor: DeviceDoctor?): Boolean =
        doctor?.items.orEmpty().any { it.key == AGENT_SPACE_ITEM && it.state == DeviceReadiness.STATE_OK }

    private const val SEP = " · "

    private fun displayLabel(number: Int): String =
        Copy.DISPLAY.replace("{number}", number.toString())

    /**
     * The Work screen picker's options: Auto, the agent space, the reported
     * displays, then a stored display the device no longer reports.
     * #EXP-1514: while the device offers a space ([spaceOffered]) Auto says
     * it lands there and Agent space only follows it; a stored `space` stays
     * listed in that place, never Not connected.
     */
    fun options(
        displays: List<WorkScreenDisplay>?,
        stored: String?,
        spaceOffered: Boolean = false,
    ): List<WorkScreenOption> {
        val auto = if (spaceOffered) "${Copy.AUTO}$SEP${Space.LABEL}" else Copy.AUTO
        val out = mutableListOf(WorkScreenOption(value = null, label = auto))
        if (spaceOffered || stored == Space.VALUE) {
            out += WorkScreenOption(value = Space.VALUE, label = Space.ONLY)
        }
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

/** One picker option; [value] = the stored `launch_defaults.computerUseDisplayId` (a display id or
 *  #EXP-1514 [WorkScreen.Space.VALUE]), null = Auto. */
data class WorkScreenOption(val value: String?, val label: String)

/** The TS reference's name. */
fun workScreenOptions(
    displays: List<WorkScreenDisplay>?,
    stored: String?,
    spaceOffered: Boolean = false,
): List<WorkScreenOption> = WorkScreen.options(displays, stored, spaceOffered)
