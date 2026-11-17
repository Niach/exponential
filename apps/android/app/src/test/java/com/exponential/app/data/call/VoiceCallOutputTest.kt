package com.exponential.app.data.call

import android.media.AudioDeviceInfo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** #EXP-1940: the call's output follows a headset that connects or leaves mid-call. */
class VoiceCallOutputTest {
    private class Phone(vararg outputs: Int) : CallOutputRouter.Port {
        val available = outputs.toMutableList()
        val routed = mutableListOf<Int>()
        var watcher: (() -> Unit)? = null

        override fun outputs(): List<Int> = available.toList()
        override fun route(type: Int) { routed += type }
        override fun watch(onChange: (() -> Unit)?) { watcher = onChange }
        fun connect(type: Int) { available += type; watcher?.invoke() }
        fun disconnect(type: Int) { available -= type; watcher?.invoke() }
    }

    private val speaker = AudioDeviceInfo.TYPE_BUILTIN_SPEAKER
    private val earpiece = AudioDeviceInfo.TYPE_BUILTIN_EARPIECE
    private val earbuds = AudioDeviceInfo.TYPE_BLUETOOTH_SCO

    @Test
    fun aHeadsetWinsOverTheLoudspeakerAndTheEarpiece() {
        val wired = AudioDeviceInfo.TYPE_WIRED_HEADSET
        assertEquals(speaker, pickCallOutput(listOf(earpiece, speaker), speaker = true))
        assertEquals(earpiece, pickCallOutput(listOf(earpiece, speaker), speaker = false))
        assertEquals(wired, pickCallOutput(listOf(earpiece, speaker, wired), speaker = true))
        assertEquals(earbuds, pickCallOutput(listOf(earpiece, speaker, earbuds), speaker = false))
    }

    @Test
    fun aHeadsetConnectingMidCallTakesTheOutput() {
        val phone = Phone(earpiece, speaker)
        val router = CallOutputRouter(phone)
        router.start()
        assertEquals(listOf(speaker), phone.routed)
        phone.connect(earbuds)
        assertEquals(earbuds, phone.routed.last())
    }

    @Test
    fun aHeadsetLeavingMidCallGivesTheOutputBackToTheSpeakerChoice() {
        val phone = Phone(earpiece, speaker, earbuds)
        val router = CallOutputRouter(phone)
        router.start()
        assertEquals(listOf(earbuds), phone.routed)
        phone.disconnect(earbuds)
        assertEquals(speaker, phone.routed.last())
        router.speaker = false
        router.apply()
        phone.connect(earbuds)
        phone.disconnect(earbuds)
        assertEquals(earpiece, phone.routed.last())
    }

    @Test
    fun anEndedCallNoLongerFollowsTheRoute() {
        val phone = Phone(earpiece, speaker)
        val router = CallOutputRouter(phone)
        router.start()
        val late = phone.watcher
        router.stop()
        assertEquals(null, phone.watcher)
        // A change already on its way to the main thread routes nothing.
        phone.available += earbuds
        late?.invoke()
        assertFalse(earbuds in phone.routed)
    }
}
