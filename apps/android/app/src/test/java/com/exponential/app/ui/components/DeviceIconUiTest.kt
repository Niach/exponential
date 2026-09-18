package com.exponential.app.ui.components

import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.icons.ExpIcons
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-924: the ONE device glyph resolver. A device's icon is its owner's pick
 * when that pick is in the device set, and the KIND default otherwise — which
 * is what a NULL column (every freshly registered machine), an unknown name and
 * a BOARD-set name all are.
 */
class DeviceIconUiTest {

    private fun device(icon: String?, kind: String): SteerDevice =
        SteerDevice(deviceId = "dev-1", icon = icon, kind = kind)

    @Test
    fun `a device-pickable name wins over the kind default`() {
        assertEquals("laptop", deviceIconName("laptop", isServer = false))
        // Even on a server: the pick is the owner's answer, not a guess.
        assertEquals("laptop", deviceIconName("laptop", isServer = true))
        assertSame(ExpIcons.byName("laptop"), deviceIcon("laptop", isServer = true))
        assertSame(ExpIcons.byName("os-apple"), deviceIcon("os-apple", isServer = false))
    }

    @Test
    fun `null falls back to the kind default`() {
        assertEquals(DEVICE_ICON_DESKTOP_DEFAULT, deviceIconName(null, isServer = false))
        assertEquals(DEVICE_ICON_SERVER_DEFAULT, deviceIconName(null, isServer = true))
        assertSame(ExpIcons.uiDevice, deviceIcon(null, isServer = false))
        assertSame(ExpIcons.uiServer, deviceIcon(null, isServer = true))
    }

    @Test
    fun `a board-set name is not a device icon`() {
        // `rocket` is pickable for a BOARD; on a device it must read as unset
        // rather than draw a phantom pick (an older client, or a hand-written
        // value, looks exactly like this).
        assertNotNull(ExpIcons.byName("rocket"))
        assertTrue("rocket" in ExpIcons.pickable)
        assertTrue("rocket" !in ExpIcons.devicePickable)
        assertEquals(DEVICE_ICON_DESKTOP_DEFAULT, deviceIconName("rocket", isServer = false))
        assertSame(ExpIcons.uiDevice, deviceIcon("rocket", isServer = false))
        assertSame(ExpIcons.uiServer, deviceIcon("rocket", isServer = true))
        // An unknown name and a blank one take the same path.
        assertEquals(DEVICE_ICON_SERVER_DEFAULT, deviceIconName("not-a-glyph", isServer = true))
        assertEquals(DEVICE_ICON_DESKTOP_DEFAULT, deviceIconName("  ", isServer = false))
        assertNull(deviceIconPick(""))
        assertNull(deviceIconPick(null))
    }

    @Test
    fun `the SteerDevice overloads read the row's kind`() {
        assertEquals(DEVICE_ICON_SERVER_DEFAULT, deviceIconName(device(null, "server")))
        assertEquals(DEVICE_ICON_DESKTOP_DEFAULT, deviceIconName(device(null, "desktop")))
        assertEquals("os-linux", deviceIconName(device("os-linux", "server")))
        assertSame(ExpIcons.byName("os-linux"), deviceIcon(device("os-linux", "server")))
        assertSame(ExpIcons.uiDevice, deviceIcon(device(null, "desktop")))
    }

    @Test
    fun `the kind defaults ARE the concept glyphs, and are offered by the picker`() {
        // The defaults are named so the PICKER can preselect them; they must
        // stay the same art the concepts resolve to, or a fresh machine would
        // draw one glyph and show another selected.
        assertSame(ExpIcons.uiDevice, ExpIcons.byName(DEVICE_ICON_DESKTOP_DEFAULT))
        assertSame(ExpIcons.uiServer, ExpIcons.byName(DEVICE_ICON_SERVER_DEFAULT))
        assertTrue(DEVICE_ICON_DESKTOP_DEFAULT in ExpIcons.devicePickable)
        assertTrue(DEVICE_ICON_SERVER_DEFAULT in ExpIcons.devicePickable)
    }

    @Test
    fun `the device set matches the contract and every name draws`() {
        assertEquals(DomainContract.deviceIconValues, ExpIcons.devicePickable)
        assertEquals(
            emptyList<String>(),
            ExpIcons.devicePickable.filter { ExpIcons.byName(it) == null },
        )
    }
}
