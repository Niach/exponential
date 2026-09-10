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

    /** Whether the draft (before its embeds) fits the contract cap. */
    fun withinLimit(text: String): Boolean = text.length <= MAX_LENGTH

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
