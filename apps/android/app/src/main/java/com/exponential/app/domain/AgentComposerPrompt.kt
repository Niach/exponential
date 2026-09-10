package com.exponential.app.domain

/**
 * EXP-825: the composer's free text on the wire. With a subject picked it is
 * OPTIONAL additional instructions (the desktop renders it as an "Additional
 * instructions from the requester" section); with none it IS the chat prompt,
 * and for the Chat and Create action builtins the server requires it. Images
 * ride it as the steer embed format ([buildSteerImageMessage], byte-identical
 * ×4): prose, blank line, one `![image](/api/attachments/<id>)` line per
 * upload, `[Image #k]` markers in the prose. The contract caps both
 * (`startPrompt`). Mirrors iOS `AgentComposerPrompt`.
 */
object AgentComposerPrompt {

    /** The contract's `startPrompt.maxLength` (16384). */
    const val MAX_LENGTH: Int = DomainContract.startPromptMaxLength

    /** The contract's `startPrompt.maxImages` — the same four the steer
     * composer carries ([MAX_STEER_IMAGES]). */
    const val MAX_IMAGES: Int = DomainContract.startPromptMaxImages

    /**
     * The `prompt` to send: null when there is nothing to say (blank text and
     * no images), so the wire omits the key and the server sees NO prompt
     * rather than an empty one.
     */
    fun build(text: String, attachmentIds: List<String>): String? {
        val trimmed = text.trim()
        if (trimmed.isEmpty() && attachmentIds.isEmpty()) return null
        return buildSteerImageMessage(trimmed, attachmentIds)
    }

    /**
     * Whether the COMPOSED prompt — the trimmed draft plus one embed line per
     * pending image — fits the contract cap; the server measures the whole
     * string (`startPromptSchema`), not the prose. Attachment ids are UUIDs,
     * so a fixed-width stand-in measures the embeds exactly before the
     * upload has minted them.
     */
    fun withinLimit(text: String, imageCount: Int = 0): Boolean =
        (build(text, List(imageCount) { ATTACHMENT_ID_STAND_IN })?.length ?: 0) <= MAX_LENGTH

    /** A 36-char UUID-shaped placeholder for a not-yet-uploaded attachment. */
    private const val ATTACHMENT_ID_STAND_IN = "00000000-0000-0000-0000-000000000000"

    /** What the composer is about to start. */
    sealed interface Subject {
        /** No subject: a chat. */
        data object None : Subject

        /** [count] checked issues (1 = a single-issue run, 2+ = a batch). */
        data class Issues(val count: Int) : Subject

        /** A team action or a builtin. */
        data object Action : Subject
    }

    /**
     * The submit label — the ×4 contract: "Start chat" / "Start coding" /
     * "Start batch · N" / "Run action".
     */
    fun submitTitle(subject: Subject): String = when (subject) {
        Subject.None -> "Start chat"
        is Subject.Issues -> if (subject.count > 1) "Start batch · ${subject.count}" else "Start coding"
        Subject.Action -> "Run action"
    }
}
