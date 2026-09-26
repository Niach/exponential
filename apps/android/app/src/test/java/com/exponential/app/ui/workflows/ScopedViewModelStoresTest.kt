package com.exponential.app.ui.workflows

import androidx.lifecycle.ViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The workflow page's per-node view-model stores: a configuration change
 * keeps them (a typed comment draft survives a rotation), any other leave
 * clears them, a popped page clears every one.
 */
class ScopedViewModelStoresTest {

    private class Probe : ViewModel() {
        var cleared = false
            private set

        override fun onCleared() {
            cleared = true
        }
    }

    @Test
    fun `one store per key, the same store while it lives`() {
        val stores = ScopedViewModelStores()
        val a = stores.store("node:a")
        assertSame(a, stores.store("node:a"))
        assertNotSame(a, stores.store("node:b"))
        assertEquals(setOf("node:a", "node:b"), stores.keys)
    }

    @Test
    fun `a configuration change keeps the store and its view models`() {
        val stores = ScopedViewModelStores()
        val store = stores.store("node:a")
        val probe = Probe()
        store.put("comments", probe)

        stores.release("node:a", retain = true)

        assertFalse(probe.cleared)
        assertSame(store, stores.store("node:a"))
        assertSame(probe, stores.store("node:a")["comments"])
    }

    @Test
    fun `any other leave clears the store and forgets the key`() {
        val stores = ScopedViewModelStores()
        val store = stores.store("node:a")
        val probe = Probe()
        store.put("comments", probe)

        stores.release("node:a", retain = false)

        assertTrue(probe.cleared)
        assertFalse("node:a" in stores.keys)
        assertNotSame(store, stores.store("node:a"))
    }

    @Test
    fun `releasing an unknown key is a no-op`() {
        val stores = ScopedViewModelStores()
        stores.release("node:missing", retain = false)
        assertTrue(stores.keys.isEmpty())
    }

    @Test
    fun `a popped page clears every store`() {
        val stores = ScopedViewModelStores()
        val a = Probe().also { stores.store("node:a").put("issue", it) }
        val b = Probe().also { stores.store("question:s1").put("session", it) }

        stores.clearAll()

        assertTrue(a.cleared)
        assertTrue(b.cleared)
        assertTrue(stores.keys.isEmpty())
    }
}
