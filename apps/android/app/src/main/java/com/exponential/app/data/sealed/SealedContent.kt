package com.exponential.app.data.sealed

import android.util.Log
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.SessionEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
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

    // #ERR-19: bounded by the CHARACTERS it holds, not only by count (4,096
    // opened plans of 5–10 KB were tens of MB), and keyed by a digest of the
    // ciphertext ([contentKey]), never by a copy of the ciphertext itself.
    private const val CACHE_MAX = 4096
    internal const val CACHE_MAX_CHARS = 2_000_000
    private val cache = CharBoundedLru<String>(CACHE_MAX, CACHE_MAX_CHARS) { it.length }

    /** A content cache key: the kind, the root and the sealed text's length +
     *  two independent hashes (32-bit `hashCode` + 64-bit FNV-1a), so the key
     *  is a few dozen chars whatever the ciphertext weighs. */
    internal fun contentKey(kind: Char, rootId: String, sealed: String): String {
        var fnv = -0x340d631b7bdddcdbL
        for (ch in sealed) {
            fnv = (fnv xor ch.code.toLong()) * 0x100000001b3L
        }
        return "$kind|$rootId|${sealed.length}|${sealed.hashCode()}|$fnv"
    }

    // (root, epoch) → when a missing key was last requested, so a screen
    // re-emitting the same locked rows does not hammer the holder.
    private val asked = mutableMapOf<String, Long>()
    private const val ASK_EVERY_MS = 30_000L

    fun isSealedBody(body: String?): Boolean = body?.startsWith(BODY_PREFIX) == true

    fun isSealedCard(card: JsonObject?): Boolean = card?.containsKey(CARD_SEALED) == true

    /** [body] opened with [rootId]'s keys, or itself (plaintext, or not openable here). */
    fun openBody(rootId: String, body: String, keys: TreeKeys = SealedKeys.keys.value): String = bodyResult(rootId, body, keys).text

    /** A card's JSON opened (the sealed card with the clear fields over it), or itself. */
    fun openCardJson(rootId: String, cardJson: String, keys: TreeKeys = SealedKeys.keys.value): String {
        val result = cardResult(rootId, cardJson, keys)
        return if (result.status == SealStatus.Opened) result.json else cardJson
    }

    /** One body's opening: the text (or the body itself when it does not
     *  open), the status and the epoch it was sealed with. */
    private class BodyResult(val text: String, val status: SealStatus, val epoch: Int?)

    /** One card's opening: [json] = the full card (plain / opened) or its
     *  clear part, the status and the epoch. */
    private class CardResult(val json: String, val status: SealStatus, val epoch: Int?)

    /** The text (or the body itself when it does not open) and the open status. */
    private fun bodyResult(rootId: String, body: String, keys: TreeKeys): BodyResult {
        if (!isSealedBody(body)) return BodyResult(body, SealStatus.Plain, null)
        val cacheKey = contentKey('b', rootId, body)
        // Only while the tree's keys are held (a signed-out account's are dropped).
        if (keys[rootId].isNullOrEmpty().not()) synchronized(cache) { cache[cacheKey] }?.let { return BodyResult(it, SealStatus.Opened, null) }
        val opened = try {
            crypto.openBody(body, keys[rootId].orEmpty())
        } catch (err: Throwable) {
            Log.w(TAG, "open body failed: ${err.javaClass.simpleName}")
            return BodyResult(body, SealStatus.Malformed, null)
        }
        return when (opened.status) {
            SealStatus.Opened, SealStatus.Plain ->
                BodyResult((opened.text ?: body).also { synchronized(cache) { cache[cacheKey] = it } }, SealStatus.Opened, opened.epoch)
            SealStatus.NoKey -> BodyResult(body.also { askFor(rootId, opened.epoch) }, SealStatus.NoKey, opened.epoch)
            else -> BodyResult(body, opened.status, opened.epoch)
        }
    }

    /** The opened card JSON, or its clear part when it does not open, and the open status. */
    private fun cardResult(rootId: String, cardJson: String, keys: TreeKeys): CardResult {
        if (!cardJson.contains("\"$CARD_SEALED\"")) return CardResult(cardJson, SealStatus.Plain, null)
        val cacheKey = contentKey('c', rootId, cardJson)
        if (keys[rootId].isNullOrEmpty().not()) synchronized(cache) { cache[cacheKey] }?.let { return CardResult(it, SealStatus.Opened, null) }
        val opened = try {
            crypto.openCard(cardJson, keys[rootId].orEmpty())
        } catch (err: Throwable) {
            Log.w(TAG, "open card failed: ${err.javaClass.simpleName}")
            return CardResult(clearPart(cardJson), SealStatus.Malformed, null)
        }
        return when (opened.status) {
            SealStatus.Opened -> CardResult(opened.cardJson.also { synchronized(cache) { cache[cacheKey] = it } }, SealStatus.Opened, opened.epoch)
            SealStatus.Plain -> CardResult(cardJson, SealStatus.Plain, opened.epoch)
            SealStatus.NoKey -> CardResult(opened.cardJson.also { askFor(rootId, opened.epoch) }, SealStatus.NoKey, opened.epoch)
            else -> CardResult(opened.cardJson, opened.status, opened.epoch)
        }
    }

    /**
     * Large chats: ONE opening per message row, whatever its outcome — a
     * `noKey` / unreadable row too, which the content cache above never holds
     * (it keeps only opened text), so every re-emitted query re-ran the native
     * open for each locked row, twice (the DAO, then [SessionTree.message]).
     * Keyed by the message id and valid while the row's body and card are the
     * same (length + hash) and the root's keys are the same map: [SealedKeys]
     * replaces a root's key map when keys for it arrive (`merge`) or go
     * (`drop`), which invalidates exactly that tree's rows.
     */
    private class RowOpening(
        val rootId: String,
        val bodyLength: Int,
        val bodyHash: Int,
        val cardLength: Int,
        val cardHash: Int,
        val rootKeys: Map<Int, ByteArray>?,
        /** Null = the row's own body (plain / not openable here). */
        val bodyText: String?,
        val bodyStatus: SealStatus,
        val bodyEpoch: Int?,
        /** Null = the row's own card. */
        val cardJson: String?,
        val cardStatus: SealStatus?,
        val cardEpoch: Int?,
    )

    // #ERR-19: bounded by the opened characters it holds too (32,768 opened
    // bodies + cards of a long tree reached a large share of a 256 MB heap).
    private const val ROWS_MAX = 32_768
    internal const val ROWS_MAX_CHARS = 4_000_000
    private val rows = CharBoundedLru<RowOpening>(ROWS_MAX, ROWS_MAX_CHARS) {
        (it.bodyText?.length ?: 0) + (it.cardJson?.length ?: 0)
    }

    private fun openRow(id: String, rootId: String, body: String, card: String?, keys: TreeKeys): Pair<BodyResult, CardResult?> {
        val sealedCard = card != null && card.contains("\"$CARD_SEALED\"")
        // Nothing sealed: no lookup, no entry (an opened row read again).
        if (!isSealedBody(body) && !sealedCard) {
            return BodyResult(body, SealStatus.Plain, null) to card?.let { CardResult(it, SealStatus.Plain, null) }
        }
        val rootKeys = keys[rootId]
        val bodyHash = body.hashCode()
        val cardHash = card?.hashCode() ?: 0
        val cardLength = card?.length ?: -1
        val hit = synchronized(rows) { rows[id] }?.takeIf {
            it.rootId == rootId && it.bodyLength == body.length && it.bodyHash == bodyHash &&
                it.cardLength == cardLength && it.cardHash == cardHash && it.rootKeys === rootKeys
        }
        if (hit != null) {
            val bodyResult = BodyResult(hit.bodyText ?: body, hit.bodyStatus, hit.bodyEpoch)
            val cardResult = card?.let { CardResult(hit.cardJson ?: it, hit.cardStatus ?: SealStatus.Plain, hit.cardEpoch) }
            // The holder is still asked (throttled) while a row stays keyless.
            if (bodyResult.status == SealStatus.NoKey) askFor(rootId, bodyResult.epoch)
            if (cardResult?.status == SealStatus.NoKey) askFor(rootId, cardResult.epoch)
            return bodyResult to cardResult
        }
        val bodyResult = bodyResult(rootId, body, keys)
        val cardResult = card?.let { cardResult(rootId, it, keys) }
        val entry = RowOpening(
            rootId = rootId,
            bodyLength = body.length,
            bodyHash = bodyHash,
            cardLength = cardLength,
            cardHash = cardHash,
            rootKeys = rootKeys,
            bodyText = bodyResult.text.takeIf { it !== body },
            bodyStatus = bodyResult.status,
            bodyEpoch = bodyResult.epoch,
            cardJson = cardResult?.json?.takeIf { it !== card },
            cardStatus = cardResult?.status,
            cardEpoch = cardResult?.epoch,
        )
        synchronized(rows) { rows[id] = entry }
        return bodyResult to cardResult
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

    fun openMessage(rootId: String, body: String, cardJson: String?, keys: TreeKeys = SealedKeys.keys.value): OpenedMessage =
        openedMessage(bodyResult(rootId, body, keys), cardJson?.let { cardResult(rootId, it, keys) })

    /** [openMessage] of a synced (or DAO-opened) row: its outcome is cached
     *  per message id ([openRow]), a locked row's too. */
    fun openMessage(row: MessageEntity, keys: TreeKeys = SealedKeys.keys.value): OpenedMessage {
        val (body, card) = openRow(row.id, row.rootId, row.body, row.card, keys)
        return openedMessage(body, card)
    }

    private fun openedMessage(body: BodyResult, card: CardResult?): OpenedMessage {
        val reasons = listOfNotNull(lockedReason(body.status), card?.let { lockedReason(it.status) })
        val locked = when {
            LOCKED_NO_KEY in reasons -> LOCKED_NO_KEY
            LOCKED_UNREADABLE in reasons -> LOCKED_UNREADABLE
            else -> null
        }
        return OpenedMessage(body = if (locked != null) "" else body.text, cardJson = card?.json, locked = locked)
    }

    fun openCard(rootId: String, card: JsonObject, keys: TreeKeys = SealedKeys.keys.value): JsonObject {
        if (!isSealedCard(card)) return card
        val opened = openCardJson(rootId, card.toString(), keys)
        return runCatching { Json.parseToJsonElement(opened) as? JsonObject }.getOrNull() ?: card
    }

    fun message(row: MessageEntity, keys: TreeKeys = SealedKeys.keys.value): MessageEntity {
        val (opened, openedCard) = openRow(row.id, row.rootId, row.body, row.card, keys)
        val body = opened.text
        // [openCardJson]: only an OPENED card replaces the synced one.
        val card = if (openedCard != null && openedCard.status == SealStatus.Opened) openedCard.json else row.card
        return if (body === row.body && card === row.card) row else row.copy(body = body, card = card)
    }

    /** The sealed title opened into [SessionEntity.title] (the neutral title
     *  when it cannot be) and its [SessionEntity.digest] opened with the
     *  root's keys (#EXP-1730: the digest lives on the session). */
    fun session(row: SessionEntity, keys: TreeKeys = SealedKeys.keys.value): SessionEntity {
        val rootId = row.rootId.ifEmpty { row.id }
        val sealedTitle = row.sealedTitle
        val digest = row.digest
        // Nothing to open natively (no sealed title, no sealed digest slot):
        // the plain path, no lookup.
        val sealed = (sealedTitle != null && isSealedBody(sealedTitle)) ||
            (digest != null && (digest.contains(BODY_PREFIX) || digest.contains("\"$CARD_SEALED\"")))
        if (!sealed) return openSession(row, rootId, keys)
        // Large chats: ONE opening per session row whatever its outcome (a
        // keyless title / digest too), valid while its sealed title and digest
        // are the same and the root's keys are the same map ([RowOpening]).
        val rootKeys = keys[rootId]
        val titleHash = sealedTitle?.hashCode() ?: 0
        val titleLength = sealedTitle?.length ?: -1
        val digestHash = digest?.hashCode() ?: 0
        val digestLength = digest?.length ?: -1
        val hit = synchronized(sessionRows) { sessionRows[row.id] }?.takeIf {
            it.rootId == rootId && it.titleLength == titleLength && it.titleHash == titleHash &&
                it.digestLength == digestLength && it.digestHash == digestHash && it.rootKeys === rootKeys
        }
        if (hit != null) {
            // The holder is still asked (throttled) while a part stays keyless.
            for (epoch in hit.asks) askFor(rootId, epoch)
            var out = row
            if (hit.titleChanged) out = out.copy(title = hit.title)
            if (hit.digestChanged) out = out.copy(digest = hit.digest)
            return out
        }
        val asks = mutableListOf<Int?>()
        val opened = recordingAsks(asks) { openSession(row, rootId, keys) }
        val entry = SessionOpening(
            rootId = rootId,
            titleLength = titleLength,
            titleHash = titleHash,
            digestLength = digestLength,
            digestHash = digestHash,
            rootKeys = rootKeys,
            titleChanged = opened.title !== row.title,
            title = opened.title.takeIf { it !== row.title },
            digestChanged = opened.digest !== row.digest,
            digest = opened.digest.takeIf { it !== row.digest },
            asks = asks.distinct(),
        )
        synchronized(sessionRows) { sessionRows[row.id] = entry }
        return opened
    }

    private fun openSession(row: SessionEntity, rootId: String, keys: TreeKeys): SessionEntity {
        val titled = sessionTitle(row, rootId, keys)
        val opened = digest(titled.digest, rootId, keys)
        return if (opened === titled.digest) titled else titled.copy(digest = opened)
    }

    /** One session row's opening ([session]): what changed + the keyless epochs it asked for. */
    private class SessionOpening(
        val rootId: String,
        val titleLength: Int,
        val titleHash: Int,
        val digestLength: Int,
        val digestHash: Int,
        val rootKeys: Map<Int, ByteArray>?,
        val titleChanged: Boolean,
        val title: String?,
        val digestChanged: Boolean,
        val digest: String?,
        val asks: List<Int?>,
    )

    private const val SESSIONS_MAX = 8_192
    internal const val SESSIONS_MAX_CHARS = 2_000_000
    private val sessionRows = CharBoundedLru<SessionOpening>(SESSIONS_MAX, SESSIONS_MAX_CHARS) {
        (it.title?.length ?: 0) + (it.digest?.length ?: 0)
    }

    // The keyless epochs an opening meets ([askFor]), recorded so a cached
    // hit asks the holder again exactly as the opening did.
    private val askLog = ThreadLocal<MutableList<Int?>?>()

    private inline fun <T> recordingAsks(into: MutableList<Int?>, block: () -> T): T {
        val previous = askLog.get()
        askLog.set(into)
        try {
            return block()
        } finally {
            askLog.set(previous)
        }
    }

    private fun sessionTitle(row: SessionEntity, rootId: String, keys: TreeKeys): SessionEntity {
        val sealed = row.sealedTitle ?: return row
        if (!isSealedBody(sealed)) return row.copy(title = sealed)
        val opened = openBody(rootId, sealed, keys)
        return if (opened === sealed) row else row.copy(title = opened)
    }

    /** A list digest (`sessions.digest`, was `conversations.digest`): its
     *  first / preview / waiting bodies and cards opened with [rootId]'s keys;
     *  no root = as synced. */
    fun digest(digest: String?, rootId: String?, keys: TreeKeys = SealedKeys.keys.value): String? {
        if (digest == null || rootId == null) return digest
        if (!digest.contains(BODY_PREFIX) && !digest.contains("\"$CARD_SEALED\"")) return digest
        return openDigest(rootId, digest, keys)
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
        // #FEED-191: every waiting card (`waits`) opens the same way.
        (root["waits"] as? JsonArray)?.let { waits ->
            val opened = waits.map { element ->
                val message = element as? JsonObject ?: return@map element
                val body = (message["body"] as? JsonPrimitive)?.takeIf { it.isString }?.content.orEmpty()
                val card = message["card"] as? JsonObject
                if (!isSealedBody(body) && !isSealedCard(card)) return@map element
                val result = openMessage(rootId, body, card?.toString(), keys)
                val out = message.toMutableMap()
                out["body"] = JsonPrimitive(result.body)
                result.cardJson?.let { json -> runCatching { Json.parseToJsonElement(json) }.getOrNull()?.let { out["card"] = it } }
                result.locked?.let { out["locked"] = JsonPrimitive(it) }
                JsonObject(out)
            }
            if (opened != waits.toList()) {
                next["waits"] = JsonArray(opened)
                changed = true
            }
        }
        return if (changed) JsonObject(next as Map<String, JsonElement>).toString() else digest
    }

    private val DIGEST_SLOTS = listOf("first", "preview", "waiting")

    private fun askFor(rootId: String, epoch: Int?) {
        askLog.get()?.add(epoch)
        val key = "$rootId|$epoch"
        val now = System.currentTimeMillis()
        val ask = synchronized(asked) {
            val last = asked[key]
            if (last != null && now - last < ASK_EVERY_MS) false else true.also { asked[key] = now }
        }
        if (ask) SealedKeys.requestMissing()
    }

    /** #ERR-19 (tests): the characters each cache holds now. */
    internal fun cachedChars(): Triple<Long, Long, Long> = Triple(
        synchronized(cache) { cache.chars },
        synchronized(rows) { rows.chars },
        synchronized(sessionRows) { sessionRows.chars },
    )

    /** Test hook: forget opened results (keys changed under a test). */
    internal fun clearCache() {
        synchronized(cache) { cache.clear() }
        synchronized(rows) { rows.clear() }
        synchronized(sessionRows) { sessionRows.clear() }
        synchronized(asked) { asked.clear() }
    }
}
