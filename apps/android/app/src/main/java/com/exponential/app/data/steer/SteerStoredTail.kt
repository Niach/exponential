package com.exponential.app.data.steer

import com.exponential.app.data.sealed.SealStatus
import com.exponential.app.data.sealed.SealedContent
import com.exponential.app.data.sealed.SealedCrypto
import com.exponential.app.domain.TailEvent
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

/** fixtures/steer-enet.json `storedTail`: the feed events a viewer keeps per opened session. */
const val STEER_STORED_TAIL = 200

/** How long a changed tail waits before it is written (at most one write per window). */
internal const val STEER_TAIL_WRITE_DEBOUNCE_MS = 2_000L

/**
 * The latest-wins slot kinds: each event replaces the last one of its kind,
 * so the tail keeps only the newest of each. A working agent republishes its
 * `turn` every couple of seconds; kept one by one they pushed every row out
 * of the tail within minutes.
 */
internal val STEER_TAIL_SLOT_KINDS = setOf(
    "turn", "usage", "config_state", "context_layout", "background_tasks",
    "task_list", "queue", "rate_limit", "diff",
)

private fun TailEvent.slotKind(): String? =
    (event["kind"] as? JsonPrimitive)?.content?.takeIf { it in STEER_TAIL_SLOT_KINDS }

/** The tree a session's tail belongs to: [publicKey] = the root's `seal_public_key` (null = keyless). */
data class TailTree(val rootId: String, val publicKey: String?, val epoch: Int)

/**
 * #PERF-9 (steer-enet.json STORED TAIL): the tail's on-disk form. Clear form
 * = `{"v":1,"sessionId":…,"events":[{"seq":…,"event":{…}}]}`; a keyed tree
 * stores that JSON SEALED as one body envelope (`exp-sealed:v1:<epoch>:…`)
 * with the tree's current key, never in clear.
 */
object StoredTailCodec {
    private const val VERSION = 1

    fun encode(sessionId: String, events: List<TailEvent>): String = buildJsonObject {
        put("v", VERSION)
        put("sessionId", sessionId)
        put(
            "events",
            buildJsonArray {
                events.forEach { tail ->
                    add(
                        buildJsonObject {
                            tail.seq?.let { put("seq", it) }
                            put("event", tail.event)
                        },
                    )
                }
            },
        )
    }.toString()

    /** Null = not a tail of [sessionId] (malformed, another version, another session). */
    fun decode(text: String, sessionId: String): List<TailEvent>? = runCatching {
        val obj = Json.parseToJsonElement(text).jsonObject
        if ((obj["v"] as? JsonPrimitive)?.intOrNull != VERSION) return null
        if ((obj["sessionId"] as? JsonPrimitive)?.content != sessionId) return null
        val events = obj["events"] as? JsonArray ?: return null
        events.mapNotNull { entry ->
            val row = entry as? JsonObject ?: return@mapNotNull null
            val event = row["event"] as? JsonObject ?: return@mapNotNull null
            TailEvent(event, (row["seq"] as? JsonPrimitive)?.longOrNull)
        }
    }.getOrNull()

    /** What goes on disk for [tree], or null = write nothing (a keyed tree whose seal failed). */
    fun seal(clear: String, tree: TailTree, crypto: SealedCrypto): String? {
        val key = tree.publicKey ?: return clear
        val sealed = runCatching { crypto.sealBody(key, tree.epoch, clear) }.getOrNull() ?: return null
        // Never in clear for a keyed tree, whatever the primitive returned.
        return sealed.takeIf { SealedContent.isSealedBody(it) }
    }

    /** The clear JSON of a stored tail, or null = it does not open here (no key yet, malformed, or clear on a keyed tree). */
    fun open(stored: String, tree: TailTree, keys: Map<Int, ByteArray>, crypto: SealedCrypto): String? {
        if (!SealedContent.isSealedBody(stored)) return stored.takeIf { tree.publicKey == null }
        val opened = runCatching { crypto.openBody(stored, keys) }.getOrNull() ?: return null
        return opened.text.takeIf { opened.status == SealStatus.Opened }
    }
}

/**
 * The tails on disk, app-private and per account: `<root>/<account>/<session>.tail`.
 * [forget] (signing out) deletes an account's tails and bumps its generation,
 * so a write that was already on its way for the signed-out account lands nowhere.
 */
class SteerTailFiles(private val root: File) {
    private val lock = Any()
    private val generations = mutableMapOf<String, Int>()

    fun generation(accountId: String): Int = synchronized(lock) { generations[accountId] ?: 0 }

    fun read(accountId: String, sessionId: String): String? = synchronized(lock) {
        val file = file(accountId, sessionId)
        if (file.isFile) runCatching { file.readText() }.getOrNull() else null
    }

    /** False = not written: the account was forgotten since [generation] was read. */
    fun write(accountId: String, sessionId: String, text: String, generation: Int): Boolean = synchronized(lock) {
        if ((generations[accountId] ?: 0) != generation) return false
        val file = file(accountId, sessionId)
        runCatching {
            file.parentFile?.mkdirs()
            val tmp = File(file.parentFile, "${file.name}.tmp")
            tmp.writeText(text)
            if (!tmp.renameTo(file)) {
                file.delete()
                tmp.renameTo(file)
            }
        }.isSuccess
    }

    fun delete(accountId: String, sessionId: String) {
        synchronized(lock) { file(accountId, sessionId).delete() }
    }

    fun forget(accountId: String) {
        synchronized(lock) {
            generations[accountId] = (generations[accountId] ?: 0) + 1
            File(root, safe(accountId)).deleteRecursively()
        }
    }

    private fun file(accountId: String, sessionId: String) = File(File(root, safe(accountId)), "${safe(sessionId)}.tail")

    private fun safe(id: String) = id.replace(Regex("[^A-Za-z0-9._-]"), "_")
}

/**
 * One connection's tail: the newest [cap] events its feed applied, in memory
 * (written on the connection's thread), persisted off it at most once per
 * [debounceMs]. A join's replay [replace]s it; live events [append].
 */
class SteerTailRecorder(
    private val scope: CoroutineScope,
    private val persist: suspend (List<TailEvent>) -> Unit,
    private val cap: Int = STEER_STORED_TAIL,
    private val debounceMs: Long = STEER_TAIL_WRITE_DEBOUNCE_MS,
) {
    private val lock = Any()
    private val events = ArrayDeque<TailEvent>(cap + 1)
    private var dirty = false
    private var job: Job? = null

    fun replace(next: List<TailEvent>) {
        synchronized(lock) {
            events.clear()
            next.forEach(::addLocked)
        }
        schedule()
    }

    fun append(event: TailEvent) {
        synchronized(lock) { addLocked(event) }
        schedule()
    }

    private fun addLocked(event: TailEvent) {
        event.slotKind()?.let { kind -> events.removeAll { it.slotKind() == kind } }
        events.addLast(event)
        while (events.size > cap) events.removeFirst()
    }

    fun snapshot(): List<TailEvent> = synchronized(lock) { events.toList() }

    /** Write what changed now (the connection closing). */
    fun flush() {
        synchronized(lock) { job?.cancel() }
        scope.launch { writeNow() }
    }

    /** One write at a time, so an older snapshot never lands after a newer one. */
    private val writing = Mutex()

    internal suspend fun writeNow() {
        writing.withLock {
            val snapshot = synchronized(lock) {
                if (!dirty) return
                dirty = false
                events.toList()
            }
            runCatching { persist(snapshot) }
        }
    }

    private fun schedule() {
        synchronized(lock) {
            dirty = true
            if (job?.isActive == true) return
            job = scope.launch {
                delay(debounceMs)
                writeNow()
            }
        }
    }
}
