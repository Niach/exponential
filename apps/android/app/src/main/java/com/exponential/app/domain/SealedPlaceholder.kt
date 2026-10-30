package com.exponential.app.domain

/**
 * EXP-1459: THE locked state ×4 (fixtures/sealed-content.json `placeholder`,
 * replayed by SealedContentFixtureTest): content this install cannot open
 * (yet) is the transcript pill with the [ICON] concept and [copy]; the chats
 * list preview reads the same copy italic muted.
 */
object SealedPlaceholder {
    const val ICON = "ui-private"
    const val COPY_NO_KEY = "Waiting for the key"
    const val COPY_UNREADABLE = "Can't be opened here"

    /** `noKey` → waiting for a holder's wrap; anything else → never opens here. */
    fun copy(reason: String): String = if (reason == "noKey") COPY_NO_KEY else COPY_UNREADABLE
}
