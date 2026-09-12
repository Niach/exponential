package com.exponential.app.domain

import com.exponential.app.data.api.SteerDevice

// EXP-836: WHICH machine a start lands on, and what to say when the machine a
// play button NAMED cannot take it.
//
// Two rules, both mirrored from the web (`lib/launch-device.ts` for the
// precedence, `use-launch-composer.ts` `deviceRequestNote` for the strings —
// byte-identical, they are the same sentences on every client):
//
//  1. A start needs an ONLINE machine with a RUNNABLE agent (EXP-409). The
//     machines list used to gate its play button on the signed-out case alone,
//     so a machine that reported NO agents at all (nothing installed, a build
//     too old to advertise them) kept its button, opened the composer, and the
//     pre-picked machine silently lost to the default one.
//  2. A play button's REQUEST is one-shot and distinct from a person's PICK: it
//     outranks the pick (and the default) while its machine is a candidate, a
//     pick clears it, and a request that cannot be honoured is SAID rather than
//     swallowed.
object LaunchDeviceRules {

    /** Whether a remote start can be delivered to [device] right now. */
    fun startable(device: SteerDevice): Boolean = device.online && device.hasRunnableAgent

    /**
     * EXP-409: online, but every installed agent is signed out — as unstartable
     * as an offline machine, and the one case the row can name the agents of.
     */
    fun signInNeeded(device: SteerDevice): Boolean =
        device.online && !device.hasRunnableAgent && device.unauthedAgentIds.isNotEmpty()

    /** Online with nothing runnable AND nothing signed out to blame (web `my-machines.tsx`). */
    const val NO_RUNNABLE_AGENT = "No agent is signed in on this machine."

    /**
     * Why an ONLINE machine cannot take a start, for the row's status line —
     * null when it can (the line then reads "Online") and when it is offline
     * (the last-seen caption already says so).
     */
    fun blockedCaption(device: SteerDevice): String? = when {
        !device.online || device.hasRunnableAgent -> null
        device.unauthedAgentIds.isNotEmpty() ->
            "${device.unauthedAgentIds.joinToString(", ")} not signed in"
        else -> NO_RUNNABLE_AGENT
    }

    /**
     * The machine a launcher is on: an explicit [requested] one while it is
     * still a candidate, else the person's [picked] one, else their default
     * (EXP-622), else the first candidate. Null when [devices] is empty.
     */
    fun resolve(
        devices: List<SteerDevice>,
        requested: String?,
        picked: String?,
    ): SteerDevice? = devices.firstOrNull { it.deviceId == requested }
        ?: devices.firstOrNull { it.deviceId == picked }
        ?: devices.firstOrNull { it.isDefault }
        ?: devices.firstOrNull()

    /**
     * What to say when the [requested] machine is not the one a run would go to
     * ([settledId]) — read off the WHOLE [registry], not the startable pool, so
     * the note can tell "offline" from "gone". Null when the request was
     * honoured, when nothing was requested, and while the registry is still
     * syncing ([registry] null: a machine that has not arrived yet is not a
     * missing one, and the request keeps outranking the default until it does).
     */
    fun requestNote(
        requested: String?,
        settledId: String?,
        registry: List<SteerDevice>?,
    ): String? {
        if (requested.isNullOrEmpty() || requested == settledId) return null
        if (registry == null) return null
        val row = registry.firstOrNull { it.deviceId == requested }
            ?: return "That machine is no longer in your registry."
        val label = row.deviceLabel.ifBlank { row.deviceId }
        return when {
            !row.online -> "$label is offline."
            !row.hasRunnableAgent -> "No agent is signed in on $label."
            else -> null
        }
    }
}
