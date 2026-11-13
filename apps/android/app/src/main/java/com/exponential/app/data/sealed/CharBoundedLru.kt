package com.exponential.app.data.sealed

/**
 * #ERR-19: an access-ordered LRU bounded by entry count AND by the characters
 * its values hold ([weigh]), so a cache of opened message text can never grow
 * past a few MB however long the opened texts are. A value heavier than a
 * quarter of [maxChars] is never kept (it would flush the cache). Not
 * thread-safe: callers synchronize on it.
 */
internal class CharBoundedLru<V : Any>(
    private val maxEntries: Int,
    private val maxChars: Int,
    private val weigh: (V) -> Int,
) {
    private val map = LinkedHashMap<String, V>(256, 0.75f, true)

    /** The characters held now. */
    var chars: Long = 0
        private set

    val size: Int get() = map.size

    operator fun get(key: String): V? = map[key]

    operator fun set(key: String, value: V) {
        val weight = weigh(value)
        map.remove(key)?.let { chars -= weigh(it) }
        if (weight > maxChars / 4) return
        map[key] = value
        chars += weight
        val it = map.entries.iterator()
        while ((map.size > maxEntries || chars > maxChars) && it.hasNext()) {
            val eldest = it.next()
            chars -= weigh(eldest.value)
            it.remove()
        }
    }

    fun clear() {
        map.clear()
        chars = 0
    }
}
