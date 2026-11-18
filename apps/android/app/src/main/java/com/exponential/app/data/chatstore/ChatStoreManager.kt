package com.exponential.app.data.chatstore

import android.content.Context
import android.util.Log
import at.exponential.chatstore.ffi.CatchUp
import at.exponential.chatstore.ffi.CatchUpInstall
import at.exponential.chatstore.ffi.CatchUpSession
import at.exponential.chatstore.ffi.ChatStore
import at.exponential.chatstore.ffi.deleteAccountStore
import at.exponential.chatstore.ffi.isCatchUpPage
import com.exponential.app.data.api.TeamInstall
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CatchUpRow
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.data.enet.PeerChat
import com.exponential.app.data.enet.PeerNode
import com.exponential.app.data.sealed.SealedContent
import com.exponential.app.data.sealed.SealedFfi
import com.exponential.app.data.sealed.SealedKeys
import com.exponential.app.data.steer.SteerFeedStore
import com.exponential.app.data.steer.SteerTailFiles
import com.exponential.app.data.steer.StoredTailCodec
import com.exponential.app.data.steer.TailTree
import com.exponential.app.data.steer.encodeSteerEnvelope
import com.exponential.app.data.steer.parseSteerEnvelope
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.TailEvent
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.onTimeout
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

private const val TAG = "ChatStore"

/** #EDE-114: what the store reads of the account's synced rows. */
interface ChatStoreRows {
    suspend fun session(accountId: String, sessionId: String): SessionEntity?
    suspend fun catchUpRows(accountId: String): List<CatchUpRow>
    suspend fun messagesSince(accountId: String, mark: String, limit: Int, offset: Int): List<MessageEntity>
    suspend fun messagesByIds(accountId: String, ids: List<String>): List<MessageEntity>
    fun messagesStamp(accountId: String): Flow<String?>
    fun sessionsStamp(accountId: String): Flow<String?>
}

/** #EDE-114: what the store needs of the account's eNet node. */
interface ChatStoreNet {
    /** The accounts whose node is up (registered, allowlist loaded). */
    val ready: StateFlow<Set<String>>
    fun peer(accountId: String): PeerNode?
    fun registry(accountId: String): Map<String, List<TeamInstall>>
    fun userId(accountId: String): String?
}

/** The store's clocks (the tests shrink them). */
data class ChatStoreTimings(
    val writeDebounceMs: Long = 500,
    val messagesDebounceMs: Long = 3_000,
    val keysDebounceMs: Long = 3_000,
    val openWaitMs: Long = 2_000,
    val readyWaitMs: Long = 60_000,
    val wipeTries: Int = 5,
)

/**
 * #EDE-114 — THE CHAT STORE on Android (`packages/domain-contract/fixtures/chat-store.json`,
 * S3): one store per signed-in account at
 * `{noBackupFilesDir}/chat-store/accounts/<account>/chat-store.sqlite`
 * (app-private, never backed up), the facade's `ChatStore`
 * (`chat-store-ffi`, linked into `libexponential_mobile_ffi.so`). Everything
 * runs off the main thread.
 *
 * * FED by what the phone receives: every steer feed a session view applies
 *   ([feedStore]: replay, live, the device's pages; keyed by the device the
 *   session is on now, written in batches), the synced messages of the
 *   account's chats (opened with the tree key; a still-sealed one stored
 *   without a body and opened again when keys arrive) and the catch-up pages.
 * * OPEN ([openTail]): a session view paints the store's REPLAY TAIL before
 *   the live feed attaches (it replaced the #PERF-9 stored tail: an old tail
 *   file is read in once, then deleted); Load earlier pages the store first.
 * * THE PHONE WALK: a catch-up pass ([CatchUpDriver]) while the app is in the
 *   foreground on an unmetered network, every `passIntervalMs` and when the
 *   account's sessions change; in the background inside WorkManager's
 *   unmetered + charging window ([ChatStoreCatchUpWorker]). The answers are
 *   routed ahead of every viewer ([CatchUpChunks]); a device-naming
 *   `history_page` from another install is answered off this store under
 *   the join's access rule.
 * * RETENTION at store open and after each pass, with the storage setting
 *   ([ChatStoreSettings], applied at once).
 * * SIGN-OUT: a deliberate one ([signOut]) stops the walk and the writer,
 *   closes the store and deletes its files; an expiry (the token gone without
 *   one) or an account switch only closes it.
 */
@Singleton
@OptIn(FlowPreview::class, ExperimentalCoroutinesApi::class)
class ChatStoreManager internal constructor(
    private val dataDir: File,
    private val tailFiles: SteerTailFiles,
    /** The active account while it holds credentials (null = none). */
    private val signedIn: Flow<String?>,
    private val rows: ChatStoreRows,
    private val net: ChatStoreNet,
    private val settings: ChatStoreSettings,
    /** The active network is unmetered (Wi-Fi, ethernet). */
    private val unmetered: StateFlow<Boolean>,
    /** A synced message's opened body, null = its chat key is not here yet. */
    private val messageBody: (MessageEntity) -> String?,
    /** An old #PERF-9 tail file opened as before (null = it does not open here). */
    private val openOldTail: (stored: String, tree: TailTree) -> String?,
    /** Changes whenever this install's tree keys do. */
    private val keys: Flow<Any>,
    private val scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.IO),
    private val nowMs: () -> Long = System::currentTimeMillis,
    private val timings: ChatStoreTimings = ChatStoreTimings(),
) {
    @Inject constructor(
        @ApplicationContext context: Context,
        auth: AuthRepository,
        holder: DatabaseHolder,
        enetNodes: EnetNodeManager,
        settings: ChatStoreSettings,
    ) : this(
        dataDir = File(context.noBackupFilesDir, STORE_DIR),
        tailFiles = SteerTailFiles(File(context.filesDir, "steer-tail")),
        signedIn = combine(auth.activeAccountId, auth.accounts) { id, accounts ->
            id?.takeIf { active -> accounts.any { it.id == active && it.token != null && it.userId != null } }
        },
        rows = DatabaseChatStoreRows(holder),
        net = object : ChatStoreNet {
            override val ready: StateFlow<Set<String>> = enetNodes.ready
            override fun peer(accountId: String) = enetNodes.peer(accountId)
            override fun registry(accountId: String) = enetNodes.registry(accountId)
            override fun userId(accountId: String) = enetNodes.userId(accountId)
        },
        settings = settings,
        unmetered = UnmeteredNetwork.watch(context),
        messageBody = { row -> SealedContent.openMessage(row).let { if (it.locked != null) null else it.body } },
        openOldTail = { stored, tree -> StoredTailCodec.open(stored, tree, SealedKeys.of(tree.rootId), SealedFfi) },
        keys = SealedKeys.keys,
    )

    /** One account's open store and everything feeding it. */
    private class Opened(val accountId: String, val store: ChatStore, val job: Job) {
        @Volatile var catchUp: CatchUp? = null
    }

    private class FeedWrite(val accountId: String, val sessionId: String, val events: List<TailEvent>)

    private val lock = Mutex()
    private val open = MutableStateFlow<Opened?>(null)
    private val suppressed = MutableStateFlow<Set<String>>(emptySet())
    private val feeds = Channel<FeedWrite>(Channel.UNLIMITED)
    private val foreground = MutableStateFlow(false)
    private val kicks = Channel<Unit>(Channel.CONFLATED)
    private val passing = Mutex()

    /** The account whose store is open (tests, logs). */
    val openAccountId: String? get() = open.value?.accountId

    fun start() {
        scope.launch {
            combine(signedIn.distinctUntilChanged(), suppressed) { id, gone -> id to gone }
                .collectLatest { (id, gone) ->
                    // A sign-out holds only while its account is still the signed-in one.
                    if (gone.any { it != id }) suppressed.update { set -> set.filterTo(mutableSetOf()) { it == id } }
                    lock.withLock { reconcile(id?.takeIf { it !in gone }) }
                }
        }
    }

    /** App visibility (ExponentialApp's process lifecycle): foreground passes only. */
    fun setForeground(on: Boolean) {
        foreground.value = on
        if (on) kicks.trySend(Unit)
    }

    private suspend fun reconcile(wanted: String?) {
        val current = open.value
        if (current?.accountId == wanted) return
        current?.let { close(it) }
        if (wanted != null) openStore(wanted)
    }

    private fun openStore(accountId: String) {
        val store = try {
            ChatStore.open(dataDir.path, accountId)
        } catch (err: Throwable) {
            Log.w(TAG, "opening the store failed: ${err.javaClass.simpleName}: ${err.message}")
            return
        }
        val job = SupervisorJob(scope.coroutineContext.job)
        val opened = Opened(accountId, store, job)
        open.value = opened
        val within = CoroutineScope(scope.coroutineContext + job)
        // RETENTION at store open.
        retention(opened)
        within.launch { writer(opened) }
        within.launch {
            migrateTails(opened)
            catchUpMessages(opened, slow = true)
        }
        within.launch {
            rows.messagesStamp(accountId).drop(1).debounce(timings.messagesDebounceMs).collect {
                catchUpMessages(opened, slow = false)
            }
        }
        within.launch {
            // Keys arrived: the still-sealed messages and old tails open now.
            keys.drop(1).debounce(timings.keysDebounceMs).collect {
                catchUpMessages(opened, slow = true)
                migrateTails(opened)
            }
        }
        within.launch {
            // THE STORAGE SETTING: a shorter window applies at once.
            settings.days.drop(1).collect { days ->
                opened.catchUp?.setWindowDays(days?.toUInt())
                retention(opened)
            }
        }
        within.launch { serve(opened) }
        within.launch { foregroundLoop(opened, within) }
        Log.i(TAG, "store open")
    }

    private suspend fun close(opened: Opened) {
        if (open.value === opened) open.value = null
        opened.catchUp?.let { walk -> runCatching { walk.stop() } }
        opened.job.cancelAndJoin()
        opened.catchUp?.let { walk -> runCatching { walk.destroy() } }
        opened.catchUp = null
        runCatching { opened.store.destroy() }
        Log.i(TAG, "store closed")
    }

    // ── SIGN-OUT ─────────────────────────────────────────────────────────

    /**
     * #EDE-120 SIGN-OUT (`signOut`, Danny's decision): a DELIBERATE sign-out
     * of [accountId] stops its walk and writer, closes the store and deletes
     * its files (`accounts/<id>/chat-store.sqlite` + `-wal`/`-shm`/`-journal`,
     * never another account's) and its old tail files. A delete that fails is
     * tried again a few times in the background: the returned job, never
     * awaited by the sign-out itself.
     */
    suspend fun signOut(accountId: String): Job {
        suppressed.update { it + accountId }
        lock.withLock { open.value?.takeIf { it.accountId == accountId }?.let { close(it) } }
        return scope.launch {
            runCatching { tailFiles.forget(accountId) }
            for (attempt in 1..timings.wipeTries) {
                val wiped = runCatching { deleteAccountStore(dataDir.path, accountId) }
                    .onFailure { Log.w(TAG, "deleting the signed-out store failed: ${it.javaClass.simpleName}") }
                    .isSuccess
                if (wiped) return@launch
                delay(500L * attempt)
            }
        }
    }

    // ── FEED ─────────────────────────────────────────────────────────────

    /** One session view's feed into the store (and its pages out of it). */
    fun feedStore(sessionId: String): SteerFeedStore = object : SteerFeedStore {
        override fun record(events: List<TailEvent>) {
            val accountId = open.value?.accountId ?: return
            if (events.none { it.seq != null }) return
            feeds.trySend(FeedWrite(accountId, sessionId, events))
        }

        override suspend fun older(beforeSeq: Long, limit: Int): List<TailEvent>? = withContext(Dispatchers.IO) {
            val opened = open.value ?: return@withContext null
            val row = rows.session(opened.accountId, sessionId) ?: return@withContext null
            if (row.deviceId.isEmpty()) return@withContext null
            runCatching {
                ChatStoreCodec.parsePage(opened.store.page(sessionId, row.deviceId, beforeSeq.toULong(), limit.toUInt()))
            }.getOrNull()
        }
    }

    /** The writer: feeds batched over [ChatStoreTimings.writeDebounceMs], one `putFeed` per session. */
    private suspend fun writer(opened: Opened) {
        while (true) {
            val first = feeds.receive()
            delay(timings.writeDebounceMs)
            val batch = mutableListOf(first)
            while (true) batch += feeds.tryReceive().getOrNull() ?: break
            for ((sessionId, writes) in batch.filter { it.accountId == opened.accountId }.groupBy { it.sessionId }) {
                val row = rows.session(opened.accountId, sessionId) ?: continue
                if (row.deviceId.isEmpty()) continue
                val rootId = row.rootId.ifEmpty { row.id }
                writes.flatMap { it.events }.chunked(FEED_CHUNK).forEach { chunk ->
                    runCatching { opened.store.putFeed(rootId, sessionId, row.deviceId, ChatStoreCodec.feedJson(chunk), nowMs()) }
                        .onFailure { Log.w(TAG, "storing a feed failed: ${it.javaClass.simpleName}") }
                }
            }
        }
    }

    /** Waits until everything [feedStore] took so far is written (tests). */
    internal suspend fun flushFeeds() {
        while (!feeds.isEmpty) delay(10)
        delay(timings.writeDebounceMs + 50)
    }

    // ── OPEN ─────────────────────────────────────────────────────────────

    /**
     * OPEN (`chat-store.json`): [sessionId]'s stored events on the device it
     * is on now, through the REPLAY TAIL (`steerFeed.replayTail`); an old
     * tail file of it is read in first. Null = nothing stored (or no store).
     */
    suspend fun openTail(sessionId: String): StoredTail? = withContext(Dispatchers.IO) {
        val opened = withTimeoutOrNull(timings.openWaitMs) { open.filterNotNull().first() } ?: return@withContext null
        val row = rows.session(opened.accountId, sessionId) ?: return@withContext null
        if (row.deviceId.isEmpty()) return@withContext null
        migrateTail(opened, sessionId, row)
        runCatching {
            ChatStoreCodec.parseTail(opened.store.openTail(sessionId, row.deviceId, DomainContract.steerFeedReplayTail.toUInt()))
        }.getOrNull()?.takeIf { it.events.isNotEmpty() }
    }

    /**
     * #EDE-130 FIND IN CHAT (`chat-store.json` SEARCH): [rootId]'s chat
     * (every session of its tree) searched for [query], newest first, at
     * most [limit] hits. Null = no store open (or it failed).
     */
    suspend fun searchChat(rootId: String, query: String, limit: Int): List<StoredChatHit>? = withContext(Dispatchers.IO) {
        val opened = open.value ?: return@withContext null
        runCatching { ChatStoreCodec.parseHits(opened.store.searchChat(rootId, query, limit.toUInt())) }.getOrNull()
    }

    /** Reads every old #PERF-9 tail of the account into the store (those that open here). */
    private suspend fun migrateTails(opened: Opened) {
        for (sessionId in tailFiles.sessions(opened.accountId)) {
            val row = rows.session(opened.accountId, sessionId) ?: continue
            migrateTail(opened, sessionId, row)
        }
        // A tail that never opened here goes with the old pruning rule.
        runCatching { tailFiles.prune(opened.accountId) }
    }

    /**
     * An old tail file read in ONCE (opened like before: sealed for a keyed
     * tree), then deleted. One that does not open YET (no key) stays for
     * a later try; a clear one on a keyed tree is never read, only deleted.
     */
    private suspend fun migrateTail(opened: Opened, sessionId: String, row: SessionEntity) {
        val stored = tailFiles.read(opened.accountId, sessionId) ?: return
        val rootId = row.rootId.ifEmpty { row.id }
        val root = if (rootId == row.id) row else rows.session(opened.accountId, rootId) ?: return
        val tree = TailTree(rootId, root.sealPublicKey, root.sealEpoch ?: 1)
        val clear = openOldTail(stored, tree)
        if (clear == null) {
            if (!SealedContent.isSealedBody(stored)) tailFiles.delete(opened.accountId, sessionId)
            return
        }
        val events = StoredTailCodec.decode(clear, sessionId)
        if (events != null && row.deviceId.isNotEmpty()) {
            val put = runCatching {
                opened.store.putFeed(rootId, sessionId, row.deviceId, ChatStoreCodec.feedJson(events), nowMs())
            }
            if (put.isFailure) return
        }
        tailFiles.delete(opened.accountId, sessionId)
    }

    // ── MESSAGES ─────────────────────────────────────────────────────────

    /** The synced messages since the last mark (inclusive); [slow] = also the ones stored sealed, opened again. */
    private suspend fun catchUpMessages(opened: Opened, slow: Boolean) {
        val store = opened.store
        try {
            val mark = store.meta(MESSAGES_MARK).orEmpty()
            var newest = mark
            var offset = 0
            while (true) {
                val page = rows.messagesSince(opened.accountId, mark, MESSAGES_PAGE, offset)
                if (page.isEmpty()) break
                val stored = page.mapNotNull(::storedMessage)
                if (stored.isNotEmpty()) store.putMessages(ChatStoreCodec.messagesJson(stored))
                page.maxOf { it.createdAt }.let { if (it > newest) newest = it }
                if (page.size < MESSAGES_PAGE) break
                offset += MESSAGES_PAGE
            }
            if (newest > mark) store.setMeta(MESSAGES_MARK, newest)
            if (!slow) return
            for (ids in store.lockedMessageIds().chunked(MESSAGES_PAGE)) {
                val unlocked = rows.messagesByIds(opened.accountId, ids).mapNotNull(::storedMessage).filter { it.body != null }
                if (unlocked.isNotEmpty()) store.putMessages(ChatStoreCodec.messagesJson(unlocked))
            }
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            Log.w(TAG, "messages catch-up failed: ${err.javaClass.simpleName}")
        }
    }

    /** A synced message as the store keeps it (null = no chat's, or no time). */
    private fun storedMessage(row: MessageEntity): StoredMessageRow? {
        val sessionId = row.sessionId?.takeIf { it.isNotEmpty() } ?: return null
        val createdAt = ChatStoreCodec.epochMs(row.createdAt) ?: return null
        return StoredMessageRow(
            id = row.id,
            rootId = row.rootId.ifEmpty { sessionId },
            sessionId = sessionId,
            authorKind = row.authorKind,
            body = messageBody(row),
            createdAtMs = createdAt,
        )
    }

    private fun retention(opened: Opened) {
        runCatching { opened.store.applyRetention(nowMs(), settings.days.value?.toUInt()) }
            .onFailure { Log.w(TAG, "retention failed: ${it.javaClass.simpleName}") }
    }

    // ── THE PHONE WALK ───────────────────────────────────────────────────

    /** Foreground passes: on an unmetered network with the node up, every `passIntervalMs` and when the sessions change. */
    private suspend fun foregroundLoop(opened: Opened, within: CoroutineScope) {
        val allowed = combine(foreground, unmetered, net.ready) { fg, free, ready -> fg && free && opened.accountId in ready }
            .distinctUntilChanged()
        within.launch { rows.sessionsStamp(opened.accountId).drop(1).collect { kicks.trySend(Unit) } }
        var intervalMs = CatchUpDefaults.PASS_INTERVAL_MS
        while (true) {
            allowed.first { it }
            coroutineScope {
                val pass = launch { runPass(opened)?.let { intervalMs = it } }
                val guard = launch {
                    allowed.first { !it }
                    pass.cancel()
                }
                pass.join()
                guard.cancel()
            }
            select<Unit> {
                kicks.onReceive { }
                onTimeout(intervalMs) { }
            }
        }
    }

    /**
     * #EDE-114 background window ([ChatStoreCatchUpWorker]): one pass once the
     * store is open and the node is up; false = it could not run.
     */
    suspend fun backgroundPass(): Boolean {
        val opened = withTimeoutOrNull(timings.readyWaitMs) { open.filterNotNull().first() } ?: return false
        withTimeoutOrNull(timings.readyWaitMs) { net.ready.first { opened.accountId in it } } ?: return false
        return runPass(opened) != null
    }

    /** One pass (one at a time), then RETENTION; returns the pass interval, null = it could not run. */
    private suspend fun runPass(opened: Opened): Long? = passing.withLock {
        val peer = net.peer(opened.accountId) ?: return null
        val userId = net.userId(opened.accountId) ?: return null
        val walk = opened.catchUp ?: CatchUp(opened.store, OWN_DEVICE, userId, settings.days.value?.toUInt())
            .also { opened.catchUp = it }
        val installs = net.registry(opened.accountId).values.flatten().distinctBy { it.endpointId }
            .map { CatchUpInstall(it.endpointId, it.userId, it.deviceId, it.lastSeenAt) }
        walk.setInstalls(peer.endpointId, installs)
        val sessions = rows.catchUpRows(opened.accountId).map {
            CatchUpSession(it.id, it.rootId.ifEmpty { it.id }, it.deviceId, it.updatedAt.ifEmpty { null }, it.busy)
        }
        val driver = CatchUpDriver(
            catchUp = walk,
            carrier = { to, sessionId, frame ->
                peer.sendChat(to, encodeSteerEnvelope(sessionId, listOf(Json.parseToJsonElement(frame).jsonObject)))
                true
            },
            nowMs = nowMs,
        )
        val due = driver.pass(sessions)
        retention(opened)
        if (due > 0) Log.i(TAG, "catch-up pass: $due session(s), ${walk.stats()}")
        walk.passIntervalMs().toLong()
    }

    // ── THE HOLDER ───────────────────────────────────────────────────────

    /** The account's node while it is up: answers routed to the walk, device-naming pages answered. */
    private suspend fun serve(opened: Opened) {
        net.ready.map { opened.accountId in it }.distinctUntilChanged().collectLatest { up ->
            if (!up) return@collectLatest
            val peer = net.peer(opened.accountId) ?: return@collectLatest
            coroutineScope {
                peer.chats.collect { chat -> route(opened, peer, chat, this) }
            }
        }
    }

    /** Quick, on the node's pump: anything heavier goes to [within]. */
    private fun route(opened: Opened, peer: PeerNode, chat: PeerChat, within: CoroutineScope) {
        if (!mentionsHistory(chat.body)) return
        val envelope = parseSteerEnvelope(chat.body.decodeToString()) ?: return
        for (frame in envelope.frames) {
            if (CatchUpChunks.take(frame)) continue
            if (isCatchUpPageFrame(frame)) within.launch(Dispatchers.IO) { answer(opened, peer, chat.from, envelope.sessionId, frame) }
        }
    }

    /** THE HOLDER: one device-naming page answered off this store, to the asker only. */
    private suspend fun answer(opened: Opened?, peer: PeerNode, from: String, sessionId: String, frame: JsonObject) {
        val holder = opened ?: open.value ?: return
        val page = frame.toString()
        if (!runCatching { isCatchUpPage(page) }.getOrDefault(false)) return
        val row = rows.session(holder.accountId, sessionId)
        val asker = row?.let { session ->
            net.registry(holder.accountId)[session.teamId]?.firstOrNull { it.endpointId == from }?.userId
        }
        val access = pageAccess(row != null, row?.userId, row?.shared == true, asker)
        val chunk = runCatching { holder.store.answerPage(sessionId, page, access) }.getOrNull() ?: return
        runCatching {
            peer.sendChat(from, encodeSteerEnvelope(sessionId, listOf(Json.parseToJsonElement(chunk).jsonObject)))
        }.onFailure { if (it is CancellationException) throw it }
    }

    /** Test hook: the holder's answer with the open store. */
    internal suspend fun answerPage(peer: PeerNode, from: String, sessionId: String, frame: JsonObject) =
        answer(null, peer, from, sessionId, frame)

    /** Test hook: a pass now (the node and the store must be up). */
    internal suspend fun passNow(): Boolean = open.value?.let { runPass(it) != null } ?: false

    /** Test hook: the open store. */
    internal val openStoreForTests: ChatStore? get() = open.value?.store

    companion object {
        const val STORE_DIR = "chat-store"
        /** A phone runs no sessions: no own device to skip. */
        private const val OWN_DEVICE = ""
        private const val MESSAGES_MARK = "messages.createdAt"
        private const val MESSAGES_PAGE = 500
        private const val FEED_CHUNK = 1_000
        private val HISTORY = "\"history_".encodeToByteArray()

        /** A chat body that may carry a `history_page` / `history_chunk` (the node pump's cheap filter). */
        internal fun mentionsHistory(body: ByteArray): Boolean {
            val needle = HISTORY
            outer@ for (i in 0..body.size - needle.size) {
                for (j in needle.indices) if (body[i + j] != needle[j]) continue@outer
                return true
            }
            return false
        }
    }
}

/** chat-store.json `catchUp.constants` the app reads before the facade answers. */
internal object CatchUpDefaults {
    const val PASS_INTERVAL_MS = 300_000L
}

/** The rows from the account's Room database. */
private class DatabaseChatStoreRows(private val holder: DatabaseHolder) : ChatStoreRows {
    override suspend fun session(accountId: String, sessionId: String) =
        holder.database(accountId).sessionDao().getById(sessionId)

    override suspend fun catchUpRows(accountId: String) = holder.database(accountId).sessionDao().catchUpRows()

    override suspend fun messagesSince(accountId: String, mark: String, limit: Int, offset: Int) =
        holder.database(accountId).messageDao().rawSince(mark, limit, offset)

    override suspend fun messagesByIds(accountId: String, ids: List<String>) =
        holder.database(accountId).messageDao().rawByIds(ids)

    override fun messagesStamp(accountId: String) = holder.database(accountId).messageDao().observeStamp()

    override fun sessionsStamp(accountId: String) = holder.database(accountId).sessionDao().observeCatchUpStamp()
}
