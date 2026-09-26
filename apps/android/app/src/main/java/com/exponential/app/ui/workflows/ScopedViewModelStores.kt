package com.exponential.app.ui.workflows

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelStore

/**
 * The workflow page's per-node [ViewModelStore]s, one per key (`node:<issue>`,
 * `node-run:<session>`, `question:<session>`). The holder itself is a view
 * model of the page's nav entry, so every store outlives a configuration
 * change (a rotation keeps a typed comment draft, a relay connection) and all
 * of them go when the page is popped ([onCleared]). A key's store is cleared
 * the moment its composition leaves for any other reason: a pick elsewhere, a
 * face change, the node leaving the workflow, the question closing.
 */
class ScopedViewModelStores : ViewModel() {
    private val stores = LinkedHashMap<String, ViewModelStore>()

    /** The keys with a live store, in creation order (tests). */
    val keys: Set<String> get() = stores.keys.toSet()

    /** The store for [key], created on first use and kept until released. */
    fun store(key: String): ViewModelStore = stores.getOrPut(key) { ViewModelStore() }

    /**
     * The composition using [key] is gone. [retain] = the activity is being
     * recreated for a configuration change: the store stays for the next
     * composition to pick up; otherwise its view models are cleared now.
     */
    fun release(key: String, retain: Boolean) {
        if (retain) return
        stores.remove(key)?.clear()
    }

    /** Every store cleared: the page is popped. */
    fun clearAll() {
        val all = stores.values.toList()
        stores.clear()
        all.forEach { it.clear() }
    }

    override fun onCleared() = clearAll()
}
