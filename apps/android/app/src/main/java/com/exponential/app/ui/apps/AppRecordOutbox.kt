package com.exponential.app.ui.apps

import android.util.Log
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
import at.exponential.ui.json.JsonValue
import com.exponential.app.data.api.AppRecordBatchResult
import com.exponential.app.data.api.AppRecordUpsertResult
import com.exponential.app.data.api.AppsApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.auth.AuthRepository
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
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
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
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

/**
 * VAPP-235 (O1, app-records.json `outbox`, docs/spec/apps.md §14): the
 * OFFLINE OUTBOX. Every `records.upsert` / `records.delete` / `records.batch`
 * an app host makes is ONE entry in the account's Room table
 * (`app_record_outbox`), laid over the synced rows ([AppRecords.outboxRows])
 * and replayed by ONE flusher per account: FIFO, one request at a time, the
 * entry's `writeId` as the server's dedupe key, stopping on a retry. It runs
 * on every enqueue, when the sync connection comes back (a failing account's
 * polls succeed again, or a [SyncManager] kick: foreground / network back)
 * and at launch ([start]). A refused replay rolls its entry back and raises
 * `Couldn't <verb> the record` ([refusals], toasted by [AppOutboxRefusals]);
 * a host call made with nothing pending awaits its own entry ([submit]).
 */
@Singleton
class AppRecordOutbox @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
    private val api: AppsApi,
    private val health: SyncHealthTracker,
    private val sync: SyncManager,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val flushLocks = ConcurrentHashMap<String, Mutex>()
    private val enqueueLocks = ConcurrentHashMap<String, Mutex>()
    private val awaiting = ConcurrentHashMap<String, CompletableDeferred<Sent>>()
    private val refusalChannel = Channel<Refusal>(Channel.BUFFERED)
    @Volatile private var started = false

    /** A later replay the server refused (the entry is gone): the failed-action toast's. */
    data class Refusal(val verb: String, val message: String?)

    /** The refusals of replays nobody awaits, for the app's one toaster. */
    val refusals: Flow<Refusal> = refusalChannel.receiveAsFlow()

    /** A host call's answer: landed (the server's answer), refused, or queued (answer from the entry). */
    sealed class Answer {
        data class Done(val upsert: AppRecordUpsertResult? = null, val batch: AppRecordBatchResult? = null) : Answer()
        data class Refused(val message: String?) : Answer()
        data object Queued : Answer()
    }

    private class Sent(
        val outcome: String,
        val upsert: AppRecordUpsertResult? = null,
        val batch: AppRecordBatchResult? = null,
        val message: String? = null,
    )

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

    /** A new entry for a host call ([call] = upsert | delete | batch): client
     *  uuids for its write id and every create. */
    fun entry(appId: String, call: String, args: Map<String, JsonValue>): AppRecords.OutboxEntry {
        val newIds = List(AppRecords.outboxCreates(call, args)) { UUID.randomUUID().toString() }
        return AppRecords.OutboxEntry(
            writeId = UUID.randomUUID().toString(),
            appId = appId,
            call = call,
            writes = AppRecords.outboxWrites(call, args, newIds),
            at = AppRecords.isoNow(),
        )
    }

    /** Persist [entry]; alone in the queue = await its first outcome, else answer at once. */
    suspend fun submit(account: String, entry: AppRecords.OutboxEntry): Answer {
        val dao = holder.database(account).appRecordOutboxDao()
        val waiter = CompletableDeferred<Sent>()
        val alone = enqueueLocks.getOrPut(account) { Mutex() }.withLock {
            val alone = dao.pendingCount() == 0
            if (alone) awaiting[entry.writeId] = waiter
            dao.insert(rowOf(entry, seq = dao.maxSeq() + 1))
            alone
        }
        if (!alone) {
            launchFlush(account)
            return Answer.Queued
        }
        flush(account)
        awaiting.remove(entry.writeId)
        if (!waiter.isCompleted) return Answer.Queued
        val sent = waiter.await()
        return when (sent.outcome) {
            AppRecords.OutboxOutcome.DONE -> Answer.Done(sent.upsert, sent.batch)
            AppRecords.OutboxOutcome.REFUSED -> Answer.Refused(sent.message)
            else -> Answer.Queued
        }
    }

    private fun launchFlush(account: String) {
        scope.launch { flush(account) }
    }

    /** Replay [account]'s pending entries, oldest first, until one must retry. */
    suspend fun flush(account: String) {
        flushLocks.getOrPut(account) { Mutex() }.withLock {
            try {
                val dao = holder.database(account).appRecordOutboxDao()
                while (true) {
                    val entry = dao.oldestPending()?.let(::entryOf) ?: break
                    val sent = send(account, entry)
                    when (sent.outcome) {
                        AppRecords.OutboxOutcome.DONE -> dao.ack(entry.writeId, AppRecords.isoNow())
                        AppRecords.OutboxOutcome.DROPPED, AppRecords.OutboxOutcome.REFUSED -> dao.delete(listOf(entry.writeId))
                    }
                    val waiter = awaiting.remove(entry.writeId)
                    if (sent.outcome == AppRecords.OutboxOutcome.REFUSED && waiter == null) {
                        refusalChannel.trySend(Refusal(AppRecords.outboxVerb(entry), sent.message))
                    }
                    waiter?.complete(sent)
                    if (sent.outcome == AppRecords.OutboxOutcome.RETRY) break
                }
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                // A closed (signed-out / switched) account's database: the next trigger retries.
                Log.w("AppRecordOutbox", "flush failed: ${e.message}")
            }
        }
    }

    /** One entry's replay ([AppRecords.outboxRequest]) and its outcome. */
    private suspend fun send(account: String, entry: AppRecords.OutboxEntry): Sent {
        val request = AppRecords.outboxRequest(entry)
        val input = request.input
        return try {
            // VAPP-231 (via-app.json `claim`): the synced row at send time; the server checks it.
            val claim = viaAppClaim(entry.appId, holder.database(account).appDao().observeById(entry.appId).first()?.publishedVersion)
            when (request.call) {
                "upsert" -> Sent(
                    AppRecords.OutboxOutcome.DONE,
                    upsert = api.recordsUpsert(
                        account,
                        entry.appId,
                        kind = input["kind"]?.string.orEmpty(),
                        data = kotlinJson(input["data"]) as? JsonObject ?: JsonObject(emptyMap()),
                        recordId = input["recordId"]?.string,
                        newId = input["newId"]?.string,
                        writeId = entry.writeId,
                        viaApp = claim,
                    ),
                )
                "delete" -> {
                    api.recordsDelete(account, entry.appId, input["recordId"]?.string.orEmpty(), writeId = entry.writeId)
                    Sent(AppRecords.OutboxOutcome.DONE)
                }
                else -> Sent(
                    AppRecords.OutboxOutcome.DONE,
                    batch = api.recordsBatch(
                        account,
                        entry.appId,
                        kotlinJson(input["writes"]) as? JsonArray ?: JsonArray(emptyList()),
                        writeId = entry.writeId,
                        viaApp = claim,
                    ),
                )
            }
        } catch (e: Exception) {
            if (e is CancellationException) throw e
            Sent(AppRecords.outboxOutcome(entry, resultOf(e)), message = e.message)
        }
    }

    /** Delete [account]'s acked entries the synced [rows] reflect (or past the grace). */
    fun prune(account: String, entries: List<AppRecords.OutboxEntry>, rows: List<AppRecords.Row>) {
        val kept = AppRecords.outboxPrune(entries, rows, AppRecords.isoNow()).mapTo(HashSet()) { it.writeId }
        val gone = entries.map { it.writeId }.filter { it !in kept }
        if (gone.isEmpty()) return
        scope.launch {
            try {
                holder.database(account).appRecordOutboxDao().delete(gone)
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                Log.w("AppRecordOutbox", "prune failed: ${e.message}")
            }
        }
    }

    companion object {
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

        fun rowOf(entry: AppRecords.OutboxEntry, seq: Long): AppRecordOutboxEntity = AppRecordOutboxEntity(
            writeId = entry.writeId,
            seq = seq,
            appId = entry.appId,
            call = entry.call,
            writes = AppRecords.outboxWritesJson(entry.writes),
            at = entry.at,
            state = entry.state,
            ackedAt = entry.ackedAt,
        )

        private fun kotlinJson(v: JsonValue?) = Json.parseToJsonElement((v ?: JsonValue.Null).json)

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
