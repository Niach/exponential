package com.exponential.app.data.sealed

import android.util.Log
import com.exponential.app.data.db.ConversationEntity
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.SessionEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * EXP-1459: opens sealed session content for the screens
 * (fixtures/sealed-content.json). Room keeps the rows exactly as synced
 * (sealed); the DAOs' observe flows pass them through here with the tree
 * keys this install holds ([SealedKeys]), so every screen reads plaintext and
 * nothing opened is ever written back.
 *
 * What cannot be opened stays as it came: a body keeps its envelope
 * ([isSealedBody]), a card keeps its `sealed` field ([isSealedCard]); both
 * mean "show the locked placeholder" and never show as text. A `noKey` asks
 * the holder for the wraps ([SealedKeys.requestMissing]).
 */
object SealedContent {
    const val BODY_PREFIX = "exp-sealed:"
    private const val CARD_SEALED = "sealed"
    private const val TAG = "SealedContent"

    /** Swapped in tests (the host build through JNA); the app's FFI otherwise. */
    @Volatile var crypto: SealedCrypto = SealedFfi

    private const val CACHE_MAX = 4096
    private val cache = object : LinkedHashMap<String, String>(256, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, String>?) = size > CACHE_MAX
    }

    // (root, epoch) → when a missing key was last requested, so a screen
    // re-emitting the same locked rows does not hammer the holder.
    private val asked = mutableMapOf<String, Long>()
    private const val ASK_EVERY_MS = 30_000L

    fun isSealedBody(body: String?): Boolean = body?.startsWith(BODY_PREFIX) == true

    fun isSealedCard(card: JsonObject?): Boolean = card?.containsKey(CARD_SEALED) == true

    /** [body] opened with [rootId]'s keys, or itself (plaintext, or not openable here). */
    fun openBody(rootId: String, body: String, keys: TreeKeys = SealedKeys.keys.value): String = bodyResult(rootId, body, keys).first

    /** A card's JSON opened (the sealed card with the clear fields over it), or itself. */
    fun openCardJson(rootId: String, cardJson: String, keys: TreeKeys = SealedKeys.keys.value): String {
        val (json, status) = cardResult(rootId, cardJson, keys)
        return if (status == SealStatus.Opened) json else cardJson
    }

    /** The text (or the body itself when it does not open) and the open status. */
    private fun bodyResult(rootId: String, body: String, keys: TreeKeys): Pair<String, SealStatus> {
        if (!isSealedBody(body)) return body to SealStatus.Plain
        val cacheKey = "b|$rootId|$body"
        // Only while the tree's keys are held (a signed-out account's are dropped).
        if (keys[rootId].isNullOrEmpty().not()) synchronized(cache) { cache[cacheKey] }?.let { return it to SealStatus.Opened }
        val opened = try {
            crypto.openBody(body, keys[rootId].orEmpty())
        } catch (err: Throwable) {
            Log.w(TAG, "open body failed: ${err.javaClass.simpleName}")
            return body to SealStatus.Malformed
        }
        return when (opened.status) {
            SealStatus.Opened, SealStatus.Plain -> (opened.text ?: body).also { synchronized(cache) { cache[cacheKey] = it } } to SealStatus.Opened
            SealStatus.NoKey -> body.also { askFor(rootId, opened.epoch) } to SealStatus.NoKey
            else -> body to opened.status
        }
    }

    /** The opened card JSON, or its clear part when it does not open, and the open status. */
    private fun cardResult(rootId: String, cardJson: String, keys: TreeKeys): Pair<String, SealStatus> {
        if (!cardJson.contains("\"$CARD_SEALED\"")) return cardJson to SealStatus.Plain
        val cacheKey = "c|$rootId|$cardJson"
        if (keys[rootId].isNullOrEmpty().not()) synchronized(cache) { cache[cacheKey] }?.let { return it to SealStatus.Opened }
        val opened = try {
            crypto.openCard(cardJson, keys[rootId].orEmpty())
        } catch (err: Throwable) {
            Log.w(TAG, "open card failed: ${err.javaClass.simpleName}")
            return clearPart(cardJson) to SealStatus.Malformed
        }
        return when (opened.status) {
            SealStatus.Opened -> opened.cardJson.also { synchronized(cache) { cache[cacheKey] = it } } to SealStatus.Opened
            SealStatus.Plain -> cardJson to SealStatus.Plain
            SealStatus.NoKey -> opened.cardJson.also { askFor(rootId, opened.epoch) } to SealStatus.NoKey
            else -> opened.cardJson to opened.status
        }
    }

    private fun clearPart(cardJson: String): String =
        runCatching { JsonObject((Json.parseToJsonElement(cardJson) as JsonObject) - CARD_SEALED).toString() }.getOrDefault("{}")

    /** sealed-content.json `placeholder.reasons`: null = opened (or plaintext). */
    fun lockedReason(status: SealStatus): String? = when (status) {
        SealStatus.Plain, SealStatus.Opened -> null
        SealStatus.NoKey -> LOCKED_NO_KEY
        SealStatus.Unsupported, SealStatus.Malformed, SealStatus.Refused -> LOCKED_UNREADABLE
    }

    const val LOCKED_NO_KEY = "noKey"
    const val LOCKED_UNREADABLE = "unreadable"

    /** A message opened (the TS reference `openMessage`): when the body or card stays sealed it is [OpenedMessage.locked], with an empty body and the card's clear part. */
    data class OpenedMessage(val body: String, val cardJson: String?, val locked: String?)

    fun openMessage(rootId: String, body: String, cardJson: String?, keys: TreeKeys = SealedKeys.keys.value): OpenedMessage {
        val (text, bodyStatus) = bodyResult(rootId, body, keys)
        val card = cardJson?.let { cardResult(rootId, it, keys) }
        val reasons = listOfNotNull(lockedReason(bodyStatus), card?.let { lockedReason(it.second) })
        val locked = when {
            LOCKED_NO_KEY in reasons -> LOCKED_NO_KEY
            LOCKED_UNREADABLE in reasons -> LOCKED_UNREADABLE
            else -> null
        }
        return OpenedMessage(body = if (locked != null) "" else text, cardJson = card?.first, locked = locked)
    }

    fun openCard(rootId: String, card: JsonObject, keys: TreeKeys = SealedKeys.keys.value): JsonObject {
        if (!isSealedCard(card)) return card
        val opened = openCardJson(rootId, card.toString(), keys)
        return runCatching { Json.parseToJsonElement(opened) as? JsonObject }.getOrNull() ?: card
    }

    fun message(row: MessageEntity, keys: TreeKeys = SealedKeys.keys.value): MessageEntity {
        val body = openBody(row.rootId, row.body, keys)
        val card = row.card?.let { openCardJson(row.rootId, it, keys) }
        return if (body === row.body && card === row.card) row else row.copy(body = body, card = card)
    }

    /** The sealed title opened into [SessionEntity.title]; the neutral title when it cannot be. */
    fun session(row: SessionEntity, keys: TreeKeys = SealedKeys.keys.value): SessionEntity {
        val sealed = row.sealedTitle ?: return row
        if (!isSealedBody(sealed)) return row.copy(title = sealed)
        val opened = openBody(row.rootId.ifEmpty { row.id }, sealed, keys)
        return if (opened === sealed) row else row.copy(title = opened)
    }

    /** `conversations.digest`: its first / preview / waiting bodies and cards opened with [rootId]'s keys. */
    fun conversation(row: ConversationEntity, rootId: String?, keys: TreeKeys = SealedKeys.keys.value): ConversationEntity {
        val digest = row.digest ?: return row
        if (rootId == null || !digest.contains(BODY_PREFIX) && !digest.contains("\"$CARD_SEALED\"")) return row
        val opened = openDigest(rootId, digest, keys)
        return if (opened === digest) row else row.copy(digest = opened)
    }

    internal fun openDigest(rootId: String, digest: String, keys: TreeKeys): String {
        val root = runCatching { Json.parseToJsonElement(digest) as? JsonObject }.getOrNull() ?: return digest
        var changed = false
        val next = root.toMutableMap()
        for (slot in DIGEST_SLOTS) {
            val message = root[slot] as? JsonObject ?: continue
            val out = message.toMutableMap()
            val body = (message["body"] as? JsonPrimitive)?.takeIf { it.isString }?.content.orEmpty()
            val card = message["card"] as? JsonObject
            if (!isSealedBody(body) && !isSealedCard(card)) continue
            val opened = openMessage(rootId, body, card?.toString(), keys)
            out["body"] = JsonPrimitive(opened.body)
            opened.cardJson?.let { json -> runCatching { Json.parseToJsonElement(json) }.getOrNull()?.let { out["card"] = it } }
            // The locked reason rides the slot (Chats.digestMessages reads it).
            opened.locked?.let { out["locked"] = JsonPrimitive(it) }
            if (out != message) {
                next[slot] = JsonObject(out)
                changed = true
            }
        }
        return if (changed) JsonObject(next as Map<String, JsonElement>).toString() else digest
    }

    private val DIGEST_SLOTS = listOf("first", "preview", "waiting")

    private fun askFor(rootId: String, epoch: Int?) {
        val key = "$rootId|$epoch"
        val now = System.currentTimeMillis()
        val ask = synchronized(asked) {
            val last = asked[key]
            if (last != null && now - last < ASK_EVERY_MS) false else true.also { asked[key] = now }
        }
        if (ask) SealedKeys.requestMissing()
    }

    /** Test hook: forget opened results (keys changed under a test). */
    internal fun clearCache() {
        synchronized(cache) { cache.clear() }
        synchronized(asked) { asked.clear() }
    }
}
