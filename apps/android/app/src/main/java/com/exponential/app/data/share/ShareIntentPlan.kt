package com.exponential.app.data.share

/**
 * The pure half of [ShareIntentParser]: what a share intent's extras mean,
 * with no Android type in sight (the JVM tests replay it). [ShareIntentParser]
 * reads the [Intent] into [Fields], asks [plan] what to copy, then caches each
 * stream under [cachedName] with the type [mimeOf] settles on.
 */
object ShareIntentPlan {
    const val ACTION_SEND = "android.intent.action.SEND"
    const val ACTION_SEND_MULTIPLE = "android.intent.action.SEND_MULTIPLE"
    const val ACTION_PROCESS_TEXT = "android.intent.action.PROCESS_TEXT"

    /** Everything a share intent carries, read off the Intent as plain values. */
    data class Fields<U>(
        val action: String?,
        val text: CharSequence? = null,
        val subject: CharSequence? = null,
        /** `EXTRA_PROCESS_TEXT` (selected text, ACTION_PROCESS_TEXT). */
        val processText: CharSequence? = null,
        /** `EXTRA_STREAM`: the one Uri of SEND, the list of SEND_MULTIPLE. */
        val streams: List<U> = emptyList(),
        /** The ClipData items' Uris (some senders fill only these). */
        val clipUris: List<U> = emptyList(),
        val shortcutId: String? = null,
    )

    /** What to do with a share: the text, and the streams to copy, in order. */
    data class Plan<U>(
        val text: String?,
        val subject: String?,
        val streams: List<U>,
        val directShareId: String?,
    )

    fun isShareAction(action: String?): Boolean =
        action == ACTION_SEND || action == ACTION_SEND_MULTIPLE || action == ACTION_PROCESS_TEXT

    /** null = not a share, or a share with nothing in it. */
    fun <U> plan(fields: Fields<U>): Plan<U>? {
        if (!isShareAction(fields.action)) return null
        if (fields.action == ACTION_PROCESS_TEXT) {
            val selected = fields.processText?.toString()?.takeIf { it.isNotBlank() } ?: return null
            return Plan(text = selected, subject = null, streams = emptyList(), directShareId = null)
        }
        val text = fields.text?.toString()?.takeIf { it.isNotBlank() }
        val subject = fields.subject?.toString()?.takeIf { it.isNotBlank() }
        // EXTRA_STREAM first (its order is the sender's), then any ClipData
        // item it does not already name; a stream named twice is one stream.
        val streams = (fields.streams + fields.clipUris).distinct()
        if (text == null && streams.isEmpty()) return null
        return Plan(text = text, subject = subject, streams = streams, directShareId = fields.shortcutId)
    }

    /**
     * One stream's type: the provider's, else the display name's extension,
     * else the intent's own type when it names one (`image/png`, or the
     * image family of a wildcard image type, which still makes it an image). A
     * provider that answers nothing (or octet-stream) for a photo must not
     * turn it into a file tile.
     */
    fun mimeOf(
        resolverType: String?,
        displayName: String?,
        intentType: String?,
        mimeForExtension: (String) -> String?,
    ): String? {
        resolverType?.trim()?.lowercase()
            ?.takeIf { it.isNotEmpty() && it != OCTET_STREAM && !it.endsWith("/*") }
            ?.let { return it }
        extensionOf(displayName)?.let(mimeForExtension)?.let { return it.lowercase() }
        val declared = intentType?.trim()?.lowercase()?.takeIf { it.isNotEmpty() && it != "*/*" } ?: return null
        return declared
    }

    fun isImage(mime: String?): Boolean = mime?.startsWith("image/") == true

    /**
     * The cached copy's name: the sender's display name (its last path
     * segment only), with an extension the [mime] implies when it has none,
     * so the composer's file:// reads infer the same type and show the name.
     */
    fun cachedName(displayName: String?, mime: String?, extensionForMime: (String) -> String?): String {
        val base = displayName?.substringAfterLast('/')?.trim()
            ?.takeIf { it.isNotEmpty() && it != "." && it != ".." }
            ?: if (isImage(mime)) "image" else "file"
        if (extensionOf(base) != null) return base
        val ext = mime?.takeIf { !it.endsWith("/*") }?.let(extensionForMime)
            ?: mime?.substringAfter('/', "")?.takeIf { it.isNotEmpty() && it != "*" && it.all(Char::isLetterOrDigit) }
        return if (ext != null) "$base.$ext" else base
    }

    /**
     * Consume-once: an intent is acted on only on a FRESH delivery. A
     * recreation (rotation, process-death restore) carries saved state and
     * redelivers the intent already handled; a launch from Recents replays
     * the task's original intent (FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY), whose
     * share was placed long ago and whose read grant is gone.
     */
    fun isFreshDelivery(hasSavedState: Boolean, flags: Int): Boolean =
        !hasSavedState && (flags and FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) == 0

    /** `Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY`. */
    const val FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY = 0x00100000

    private const val OCTET_STREAM = "application/octet-stream"

    private fun extensionOf(name: String?): String? =
        name?.substringAfterLast('/')?.substringAfterLast('.', "")
            ?.takeIf { it.isNotEmpty() && it.length <= 8 && it != name.substringAfterLast('/') }
            ?.lowercase()
}
