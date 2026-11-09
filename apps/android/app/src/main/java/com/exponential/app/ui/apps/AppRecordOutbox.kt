package com.exponential.app.ui.apps

import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import at.exponential.engine.ffi.EngineException
import at.exponential.engine.ffi.OutboxLog
import at.exponential.engine.ffi.Submitted
import at.exponential.engine.ffi.SyncTransport
import at.exponential.engine.ffi.SyncedListener
import at.exponential.engine.ffi.SyncedRecordStore
import at.exponential.ui.json.JsonValue
import com.exponential.app.NativeGuard
import com.exponential.app.Observability
import com.exponential.app.data.api.AppsApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AppRecordOutboxDao
import com.exponential.app.data.db.AppRecordOutboxEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.AccountHealth
import com.exponential.app.data.electric.SyncHealthTracker
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.viaAppClaim
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.failedAction
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import dagger.hilt.android.EntryPointAccessors
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/**
 * VAPP-235 (O1, app-records.json `outbox`, docs/spec/apps.md §14) / VAPP-272:
 * the OFFLINE OUTBOX, on the Engine. Each account has one `SyncedRecordStore`:
 * the queue, the flusher, the waiters and the prune live there (`outbox.json`
 * `syncedCases`). This class fills in its providers (the account's Room
 * table, tRPC) and says when to flush: on every enqueue, when the sync
 * connection comes back (a failing account's polls succeed again, or a
 * [SyncManager] kick: foreground / network back) and at launch ([start]). A
 * refused replay nobody awaits raises `Couldn't <verb> the record`
 * ([refusals], toasted by [AppOutboxRefusals]). The Engine's calls block on
 * the network: they run on [Dispatchers.IO].
 */
@Singleton
class AppRecordOutbox @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
    private val api: AppsApi,
    private val health: SyncHealthTracker,
    private val sync: SyncManager,
) {
    // A failure no call site caught is reported, never the crash handler's.
    private val scope = CoroutineScope(
        SupervisorJob() + Dispatchers.IO + CoroutineExceptionHandler { _, e -> Observability.nonFatal(TAG, "outbox job failed", e) },
    )
    /** The Engine (`eengine-ffi`): its `.so` loads with the first store; off = no outbox, the app keeps running. */
    internal val engine = NativeGuard("Engine")
    /** Each account's store over the database it opened on (a new database = a new queue). */
    private val stores = HashMap<String, Pair<ExponentialDatabase, SyncedRecordStore>>()
    private val refusalChannel = Channel<Refusal>(Channel.BUFFERED)
    @Volatile private var started = false

    /** A later replay the server refused (the entry is gone): the failed-action toast's. */
    data class Refusal(val verb: String, val message: String?)

    /** The refusals of replays nobody awaits, for the app's one toaster. */
    val refusals: Flow<Refusal> = refusalChannel.receiveAsFlow()

    /** A host call's answer, the server's or the entry's: landed, refused, or queued. */
    sealed class Answer {
        data class Done(val answer: JsonValue?) : Answer()
        /** [invalid] = invalid input (BAD_REQUEST): a record form shows it inline. */
        data class Refused(val message: String?, val invalid: Boolean = false) : Answer()
        data class Queued(val answer: JsonValue?) : Answer()
    }

    /** Flush at launch, on a sign-in, when an account's sync comes back and on every sync kick. */
    fun start() {
        if (started) return
        started = true
        scope.launch {
            auth.accounts
                .map { list -> list.filter { it.token != null }.map { it.id }.toSet() }
                .distinctUntilChanged()
                .collect { ids -> ids.forEach(::launchFlush) }
        }
        scope.launch {
            var failing = emptySet<String>()
            health.state.collect { all ->
                val now = all.filterValues(::isFailing).keys
                (failing - now).filter { it in signedIn() }.forEach(::launchFlush)
                failing = now
            }
        }
        scope.launch {
            sync.lastKickAt.drop(1).collect { signedIn().forEach(::launchFlush) }
        }
    }

    private fun isFailing(health: AccountHealth): Boolean {
        val errorAt = health.lastErrorAtMs ?: return false
        val successAt = health.lastSuccessAtMs ?: return true
        return errorAt > successAt
    }

    private fun signedIn(): Set<String> = auth.accounts.value.filter { it.token != null }.map { it.id }.toSet()

    /** [block] on [account]'s store; null = the Engine is off or the call failed (the next trigger retries). */
    private fun <T> withStore(account: String, what: String, block: (SyncedRecordStore) -> T): T? =
        engine.call(what) { block(store(account)) }

    private fun store(account: String): SyncedRecordStore {
        val db = holder.database(account)
        synchronized(stores) {
            stores[account]?.takeIf { it.first === db }?.let { return it.second }
            val store = SyncedRecordStore(RoomLog(db.appRecordOutboxDao()), TrpcTransport(account, db), Toasts(), null)
            stores[account] = db to store
            return store
        }
    }

    /** Queue one host call (`records_upsert` | `records_delete` | `records_batch`):
     *  alone in the queue it awaits its first outcome, else it answers at once. */
    suspend fun submit(account: String, app: String, function: String, args: Map<String, JsonValue>): Answer = withContext(Dispatchers.IO) {
        when (val answer = withStore(account, "submit") { it.submitCall(app, function, JsonValue.Obj(args).json, true) }) {
            null -> Answer.Refused(null)
            is Submitted.Done -> Answer.Done(jsonValue(answer.answerJson))
            is Submitted.Refused -> Answer.Refused(answer.message, answer.invalid)
            is Submitted.Queued -> {
                launchFlush(account)
                Answer.Queued(jsonValue(answer.answerJson))
            }
        }
    }

    private fun launchFlush(account: String) {
        scope.launch { flush(account) }
    }

    /** Replay [account]'s pending entries, oldest first, until one must retry. */
    suspend fun flush(account: String) {
        // A closed (signed-out / switched) account's database: the next trigger retries.
        withContext(Dispatchers.IO) { withStore(account, "flush") { it.flush() } }
    }

    /** The synced [rows] of [app] a binding read: acked entries they reflect (or past the grace) leave. */
    fun reflect(account: String, app: String, rows: List<AppRecords.Row>) {
        scope.launch {
            withStore(account, "prune") { it.setRows(app, Json.encodeToString(JsonArray.serializer(), JsonArray(rows.map(::rowJson)))) }
        }
    }

    /** tRPC out, as the account and the entry's app. */
    private inner class TrpcTransport(private val account: String, private val db: ExponentialDatabase) : SyncTransport {
        override fun push(app: String, call: String, inputJson: String): String = runBlocking {
            val input = Json.parseToJsonElement(inputJson).jsonObject
            fun text(key: String) = input[key]?.jsonPrimitive?.contentOrNull
            try {
                // VAPP-231 (via-app.json `claim`): the synced row at send time; the server checks it.
                val claim = viaAppClaim(app, db.appDao().observeById(app).first()?.publishedVersion)
                val writeId = text("writeId")
                val answer = when (call) {
                    "upsert" -> {
                        val data = input["data"] as? JsonObject ?: JsonObject(emptyMap())
                        val out = api.recordsUpsert(account, app, kind = text("kind").orEmpty(), data = data, recordId = text("recordId"), newId = text("newId"), writeId = writeId, viaApp = claim)
                        buildJsonObject { put("id", out.id) }
                    }
                    "delete" -> {
                        api.recordsDelete(account, app, text("recordId").orEmpty(), writeId = writeId)
                        JsonNull
                    }
                    else -> {
                        val out = api.recordsBatch(account, app, input["writes"] as? JsonArray ?: JsonArray(emptyList()), writeId = writeId, viaApp = claim)
                        buildJsonObject {
                            put("ids", JsonArray(out.ids.map(::JsonPrimitive)))
                            put("eventIds", JsonArray(out.eventIds.map(::JsonPrimitive)))
                        }
                    }
                }
                buildJsonObject {
                    put("ok", true)
                    put("answer", answer)
                }.toString()
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                when (val result = resultOf(e)) {
                    is AppRecords.OutboxResult.Code -> buildJsonObject {
                        put("code", result.code)
                        put("message", e.message)
                        put("invalid", e is TrpcException && (e.code == "BAD_REQUEST" || (e.code == null && e.status?.value == 400)))
                    }.toString()
                    else -> """{"transport":true}"""
                }
            } catch (e: Throwable) {
                // An Error on the Engine's flusher thread: a transport failure (retried), never a crash.
                Observability.nonFatal(TAG, "outbox push failed", e)
                """{"transport":true}"""
            }
        }
    }

    private inner class Toasts : SyncedListener {
        override fun changed() {}

        override fun refused(entryJson: String, verb: String, message: String?) {
            refusalChannel.trySend(Refusal(verb, message))
        }
    }

    companion object {
        private const val TAG = "AppRecordOutbox"

        /** The entries of [appId] in [db], oldest first. */
        fun entries(db: ExponentialDatabase, appId: String): Flow<List<AppRecords.OutboxEntry>> =
            db.appRecordOutboxDao().observeByApp(appId).map { rows -> rows.map(::entryOf) }

        fun entryOf(row: AppRecordOutboxEntity): AppRecords.OutboxEntry = AppRecords.OutboxEntry(
            writeId = row.writeId,
            appId = row.appId,
            call = row.call,
            writes = AppRecords.outboxWritesOf(row.writes),
            at = row.at,
            state = row.state,
            ackedAt = row.ackedAt,
        )

        /** The Engine's spelling of a stored entry; throws on a row the Engine
         *  could not read (`eengine::outbox::OutboxEntry`): the Engine skips such a
         *  row and never removes it, so it would stay pending (and overlay its
         *  rows) forever. */
        internal fun entryJson(row: AppRecordOutboxEntity): JsonObject {
            require(row.call in ENTRY_CALLS) { "unknown call ${row.call}" }
            require(row.state in ENTRY_STATES) { "unknown state ${row.state}" }
            val writes = Json.parseToJsonElement(row.writes).jsonArray
            require(row.call == "batch" || writes.size == 1) { "a ${row.call} with ${writes.size} writes" }
            for (write in writes) {
                val w = write.jsonObject
                fun text(key: String) = (w[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
                val op = text("op")
                require(op in WRITE_OPS) { "unknown op $op" }
                requireNotNull(text("id")) { "a write without an id" }
                if (op != "delete") requireNotNull(text("kind")) { "a $op without a kind" }
                w["data"]?.let { require(it is JsonObject) { "data is not an object" } }
            }
            return buildJsonObject {
                put("writeId", row.writeId)
                put("appId", row.appId)
                put("call", row.call)
                put("writes", writes)
                put("at", row.at)
                put("state", row.state)
                row.ackedAt?.let { put("ackedAt", it) }
            }
        }

        private val ENTRY_CALLS = setOf("upsert", "delete", "batch")
        private val ENTRY_STATES = setOf("pending", "acked")
        private val WRITE_OPS = setOf("create", "update", "delete")

        private fun rowJson(row: AppRecords.Row): JsonObject = buildJsonObject {
            put("id", row.id)
            put("kind", row.kind)
            put("data", Json.parseToJsonElement(JsonValue.Obj(row.data).json))
            row.createdBy?.let { put("createdBy", it) }
            row.authorId?.let { put("authorId", it) }
            put("createdAt", row.createdAt)
            put("updatedAt", row.updatedAt)
        }

        private fun jsonValue(json: String): JsonValue? = JsonValue.parse(json).takeUnless { it is JsonValue.Null }

        /** A failed replay as the reference's result: the server's tRPC code
         *  (from the body, else the HTTP status), else no network. */
        fun resultOf(e: Throwable): AppRecords.OutboxResult {
            val trpc = e as? TrpcException ?: return AppRecords.OutboxResult.Transport
            trpc.code?.let { return AppRecords.OutboxResult.Code(it) }
            val status = trpc.status?.value ?: return AppRecords.OutboxResult.Transport
            return AppRecords.OutboxResult.Code(
                when (status) {
                    400 -> "BAD_REQUEST"
                    401 -> "UNAUTHORIZED"
                    403 -> "FORBIDDEN"
                    404 -> "NOT_FOUND"
                    408 -> "TIMEOUT"
                    409 -> "CONFLICT"
                    412 -> "PRECONDITION_FAILED"
                    429 -> "TOO_MANY_REQUESTS"
                    499 -> "CLIENT_CLOSED_REQUEST"
                    502 -> "BAD_GATEWAY"
                    503 -> "SERVICE_UNAVAILABLE"
                    504 -> "GATEWAY_TIMEOUT"
                    else -> if (status >= 500) "INTERNAL_SERVER_ERROR" else "BAD_REQUEST"
                },
            )
        }
    }
}

/** The account's Room table; the Engine calls in on [Dispatchers.IO]. */
internal class RoomLog(private val dao: AppRecordOutboxDao) : OutboxLog {
    private fun <T> failing(body: suspend () -> T): T = try {
        runBlocking { body() }
    } catch (e: Exception) {
        throw EngineException.Failure("INTERNAL", e.message.orEmpty())
    }

    override fun entriesJson(): String = failing {
        val rows = dao.all()
        val readable = rows.mapNotNull { row -> runCatching { AppRecordOutbox.entryJson(row) }.getOrNull() }
        // An unreadable row can never replay: it leaves.
        if (readable.size < rows.size) dao.delete(rows.filter { runCatching { AppRecordOutbox.entryJson(it) }.isFailure }.map { it.writeId })
        JsonArray(readable).toString()
    }

    override fun push(entryJson: String) = failing {
        val entry = Json.parseToJsonElement(entryJson).jsonObject
        fun text(key: String) = entry.getValue(key).jsonPrimitive.content
        dao.insert(
            AppRecordOutboxEntity(
                writeId = text("writeId"),
                seq = dao.maxSeq() + 1,
                appId = text("appId"),
                call = text("call"),
                writes = entry.getValue("writes").toString(),
                at = text("at"),
                state = text("state"),
                ackedAt = entry["ackedAt"]?.jsonPrimitive?.contentOrNull,
            ),
        )
    }

    override fun ack(writeId: String, at: String) = failing { dao.ack(writeId, at) }

    override fun remove(writeIds: List<String>) = failing { dao.delete(writeIds) }
}


/** VAPP-235: a later replay's refusal = `Couldn't <verb> the record` with the
 *  server's message (mounted once, under the app's one toaster). */
@Composable
fun AppOutboxRefusals() {
    val context = LocalContext.current
    val toaster = LocalToaster.current
    val outbox = remember {
        EntryPointAccessors.fromApplication(context.applicationContext, AppHostEntryPoint::class.java).appRecordOutbox()
    }
    LaunchedEffect(outbox) {
        outbox.refusals.collect { toaster.failedAction(it.verb, "the record", it.message) }
    }
}

/** VAPP-235: the running app's header glyph while it has pending entries:
 *  `ui-offline`, muted, icon size; tooltip + accessibility label =
 *  [AppRecords.outboxPendingLabel]. Nothing pending = nothing. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AppOutboxPendingGlyph(appId: String, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val label by remember(appId) {
        val deps = EntryPointAccessors.fromApplication(context.applicationContext, AppHostEntryPoint::class.java)
        accountDatabaseFlow(deps.authRepository(), deps.databaseHolder())
            .scopedQuery(emptyList()) { AppRecordOutbox.entries(it, appId) }
            .map { AppRecords.outboxPendingLabel(it, appId) }
            .distinctUntilChanged()
    }.collectAsState(initial = null)
    val text = label ?: return
    TooltipBox(
        positionProvider = TooltipDefaults.rememberPlainTooltipPositionProvider(),
        tooltip = { PlainTooltip { Text(text) } },
        state = rememberTooltipState(),
    ) {
        Icon(
            ExpIcons.uiOffline,
            contentDescription = text,
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = modifier.size(16.dp).testTag("app-outbox-pending"),
        )
    }
}
