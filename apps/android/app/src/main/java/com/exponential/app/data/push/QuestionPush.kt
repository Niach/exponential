package com.exponential.app.data.push

import com.exponential.app.data.sealed.SealStatus
import com.exponential.app.data.sealed.SealedCrypto
import com.exponential.app.domain.Chats

/**
 * #VAPP-45 (fixtures/question-push.json, its `$comment` is the spec): the
 * push of a question or plan card carries the question TEXT sealed to the
 * session tree (`data.sealedText` + `data.rootId`). FcmService opens it with
 * the tree keys this install holds and shows THE chat ask preview as the
 * body; anything else keeps the push's own body. Mirrors the TS reference
 * `src/question-push.ts` and iOS ExpPush `QuestionPush.swift`.
 */
internal object QuestionPush {
    const val SEALED_TEXT_KEY = "sealedText"
    const val ROOT_ID_KEY = "rootId"

    /** The shown body's cap in code points (longer = cut, `…` last). */
    const val BODY_MAX = 200

    /** [text] cut to [BODY_MAX] code points. */
    fun clip(text: String): String {
        val count = text.codePointCount(0, text.length)
        if (count <= BODY_MAX) return text
        return text.substring(0, text.offsetByCodePoints(0, BODY_MAX - 1)) + "…"
    }

    /** The shown body of an opened ask text: the chat ask preview, clipped. */
    fun text(askText: String): String = clip(Chats.askTextPreview(askText).text)

    /**
     * The body for a delivered push: `sealedText` opened with the keys of
     * `rootId` ([keysFor], epoch → secret; null = none held), else [fallback].
     * Never throws; nothing opened is logged.
     */
    fun body(
        data: Map<String, String>,
        keysFor: (String) -> Map<Int, ByteArray>?,
        crypto: SealedCrypto,
        fallback: String?,
    ): String? {
        val sealed = data[SEALED_TEXT_KEY]?.takeIf { it.isNotEmpty() } ?: return fallback
        val rootId = data[ROOT_ID_KEY]?.takeIf { it.isNotEmpty() } ?: return fallback
        val keys = runCatching { keysFor(rootId) }.getOrNull()?.takeIf { it.isNotEmpty() } ?: return fallback
        val opened = runCatching { crypto.openBody(sealed, keys) }.getOrNull() ?: return fallback
        if (opened.status != SealStatus.Opened) return fallback
        val shown = text(opened.text ?: return fallback)
        return shown.ifBlank { fallback }
    }
}
