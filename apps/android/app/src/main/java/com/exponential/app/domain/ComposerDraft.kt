package com.exponential.app.domain

/**
 * #FEED-219: every composer's synced DRAFT. Pure and locked by
 * `fixtures/composer-draft.json` (`keys`, `unparsable`, `writes`, `fields`,
 * `debounceMs`; the `$comment` holds the rules); mirrors the TS reference
 * `src/composer-draft.ts`.
 */
object ComposerDraft {

    /** The kinds a key may name, the fixture's `<kind>` half. */
    enum class Kind(val wire: String) {
        Comment("comment"),
        Reply("reply"),
        Session("session"),
        Agent("agent"),
        App("app"),
        NewChat("newChat"),
        ;

        companion object {
            fun fromWire(wire: String): Kind? = entries.firstOrNull { it.wire == wire }
        }
    }

    /** The target a composer writes to. */
    data class Target(val kind: Kind, val id: String) {
        val key: String get() = "${kind.wire}:$id"
    }

    /** The fixture's debounce after the last keystroke. */
    const val DEBOUNCE_MS = 800L

    fun key(target: Target): String = target.key

    /** An unknown kind or a missing id parses to null (the row is ignored). */
    fun parseKey(key: String): Target? {
        val at = key.indexOf(':')
        if (at <= 0) return null
        val id = key.substring(at + 1)
        if (id.isEmpty()) return null
        val kind = Kind.fromWire(key.substring(0, at)) ?: return null
        return Target(kind, id)
    }

    sealed interface Write {
        data class Upsert(val text: String) : Write
        data object Delete : Write
    }

    /** What writing the field's text does to the row: blank deletes. */
    fun write(text: String): Write = if (text.isBlank()) Write.Delete else Write.Upsert(text)

    /**
     * What the field shows: [field] once [edited], else this client's own
     * unechoed write ([local]), else the synced row ([synced]), else empty.
     */
    fun field(field: String, edited: Boolean, local: String?, synced: String?): String =
        if (edited) field else local ?: synced ?: ""
}
