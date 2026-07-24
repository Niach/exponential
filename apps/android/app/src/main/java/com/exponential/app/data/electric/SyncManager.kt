package com.exponential.app.data.electric

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.os.SystemClock
import androidx.room.withTransaction
import androidx.sqlite.db.SupportSQLiteDatabase
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.IssueSubscriberEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.NotificationEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.TeamInviteEntity
import com.exponential.app.data.db.TeamMemberEntity
import dagger.hilt.android.qualifiers.ApplicationContext
import io.ktor.client.HttpClient
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject

/// Multi-account sync orchestrator. Maintains one set of 14 shape jobs per
/// signed-in account; each pipeline writes to that account's per-account Room
/// instance (`exponential-<accountId>-v2.db`). Sign-out on one account cancels
/// just that pipeline; other accounts keep syncing.
@Singleton
class SyncManager @Inject constructor(
    private val auth: AuthRepository,
    private val databaseHolder: DatabaseHolder,
    private val client: HttpClient,
    private val json: Json,
    private val stats: SyncStats,
    @ApplicationContext private val context: Context,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val lock = Any()

    /** One account's 14 shape loops, plus the clients a [kick] has to reach. */
    private class Pipeline(val jobs: List<Job>, val clients: List<ShapeClient<*>>)

    private val pipelines = mutableMapOf<String, Pipeline>()

    // elapsedRealtime of the last kick that actually went out (0 = none yet).
    // The "Syncing…" chip measures shape freshness against this.
    private val _lastKickAt = MutableStateFlow(0L)
    val lastKickAt: StateFlow<Long> = _lastKickAt.asStateFlow()

    // Debounce gate for unforced kicks: foreground + network-available +
    // several pushes can land within the same second, and 14 shapes each
    // dropping a live connection per trigger is a real cost.
    private val lastKickGate = AtomicLong(0L)

    // registerDefaultNetworkCallback replays the CURRENT network immediately —
    // but ONLY when a default network exists at registration time. Seeded in
    // registerNetworkCallback: false (skip the next onAvailable, it's the
    // replay) when online at registration, true when offline — else an offline
    // cold start would discard the first REAL connectivity-restored callback,
    // the exact transition this callback exists to catch.
    private val sawFirstNetworkCallback = AtomicBoolean(false)
    // Throttles the Logcat line for dropped columns to once per (account, shape,
    // column-set) — the diagnostics Set already dedupes for the UI.
    private val loggedDroppedColumns = java.util.Collections.synchronizedSet(mutableSetOf<String>())

    private fun reportDroppedColumns(accountId: String, shape: String, columns: Set<String>) {
        if (columns.isEmpty()) return
        stats.reportDropped(accountId, shape, columns)
        val key = "$accountId|$shape|${columns.sorted().joinToString(",")}"
        if (loggedDroppedColumns.add(key)) {
            android.util.Log.i("SyncManager", "[$shape] dropped unknown columns: ${columns.sorted()}")
        }
    }

    fun start() {
        scope.launch {
            // Reconcile on every change to the signed-in-account set. Map to
            // just the set of "signed in" accountIds so unrelated mutations
            // (lastUsedAt touches, name updates) don't churn the pipelines.
            auth.accounts
                .map { list -> list.filter { it.token != null }.map { it.id }.toSet() }
                .distinctUntilChanged()
                .collect { signedIn -> reconcile(signedIn) }
        }
        registerNetworkCallback()
    }

    /**
     * A returning network is the other half of the foreground kick: coming back
     * from a dead radio, the parked loops are sitting on sockets that will only
     * fail when their timeout expires. Kick them the moment connectivity is
     * back instead.
     */
    private fun registerNetworkCallback() {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return
        // Some OEM builds throw TooManyRequestsException here; losing the
        // network kick is survivable, crashing at startup is not.
        runCatching {
            // The registration replay only happens when a default network
            // exists RIGHT NOW; offline, the first onAvailable is already the
            // real offline→online transition and must not be swallowed.
            sawFirstNetworkCallback.set(manager.activeNetwork == null)
            manager.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
                // Binder thread — ShapeClient.kick is thread-safe by design.
                override fun onAvailable(network: Network) {
                    if (!sawFirstNetworkCallback.getAndSet(true)) return
                    kick("network-available")
                }
            })
        }.onFailure {
            android.util.Log.w("SyncManager", "network callback not registered: ${it.message}")
        }
    }

    /**
     * Poll every shape of every account NOW (see [ShapeClient.kick]) —
     * app foreground, a returning network, an arriving push, a pull-to-refresh.
     * Unforced calls are debounced to one per [KICK_DEBOUNCE_MS]; [force]
     * bypasses that for user-initiated refreshes, which must never silently do
     * nothing.
     */
    fun kick(reason: String, force: Boolean = false) {
        val now = SystemClock.elapsedRealtime()
        if (!force) {
            val previous = lastKickGate.get()
            if (now - previous < KICK_DEBOUNCE_MS) return
            if (!lastKickGate.compareAndSet(previous, now)) return
        } else {
            lastKickGate.set(now)
        }
        android.util.Log.i("SyncManager", "kick($reason)")
        _lastKickAt.value = now
        val clients = synchronized(lock) { pipelines.values.flatMap { it.clients } }
        clients.forEach { it.kick() }
    }

    /**
     * Kick, then wait until every core shape of [accountId] has completed a
     * poll (or [timeoutMs] elapses). Returns true when the account is known
     * caught up — the pull-to-refresh spinner runs exactly this long.
     *
     * Shapes whose kick was suppressed as redundant already polled within
     * [KICK_FRESHNESS_MS] of t0, which is why the comparison allows that much
     * slack: on a healthy app this resolves immediately instead of hanging for
     * the full timeout.
     */
    suspend fun refresh(accountId: String, timeoutMs: Long = 5_000): Boolean {
        val t0 = SystemClock.elapsedRealtime()
        kick("refresh", force = true)
        return withTimeoutOrNull(timeoutMs) {
            stats.state.first { all ->
                val shapes = all[accountId]
                CORE_SHAPES.all { name ->
                    (shapes?.get(name)?.lastSuccessAtMs ?: 0L) >= t0 - KICK_FRESHNESS_MS
                }
            }
        } != null
    }

    /// Sign out a specific account: cancel its pipeline. The Room cache stays
    /// so the user can resume offline browsing if they sign back in. Full
    /// deletion happens via `DatabaseHolder.deleteFiles(accountId)` from
    /// Settings.
    suspend fun signOut(accountId: String) {
        cancelPipeline(accountId)
        stats.clearAccount(accountId)
    }

    /// Sign out the ACTIVE account. Used by AppViewModel's active-account
    /// paths (clearInstance / signOut); ServerDetailScreen manages arbitrary
    /// accounts and calls the explicit `signOut(accountId)` overload instead.
    suspend fun signOut() {
        auth.activeAccountId.value?.let { signOut(it) }
    }

    // MARK: - Reconciliation

    // Keyed to the SET of signed-in accounts, not the active account: rapid
    // account switches don't churn pipelines at all (every signed-in account
    // keeps syncing in the background). UI account scoping is reactive
    // (accountDatabaseFlow), so there is no rebuild race to coordinate with.
    // Sign-out cleanup still works through two redundant paths: signOut()
    // cancels the pipeline directly, and the accounts collector in start()
    // sees the token disappear and reconciles (both are idempotent).
    private fun reconcile(signedIn: Set<String>) {
        synchronized(lock) {
            val running = pipelines.keys.toSet()

            // Cancel pipelines for accounts no longer signed in.
            for (accountId in running - signedIn) {
                pipelines.remove(accountId)?.jobs?.forEach { it.cancel() }
                stats.clearAccount(accountId)
                android.util.Log.i("SyncManager", "Cancelled shape pipeline for $accountId")
            }

            // Launch pipelines for newly signed-in accounts.
            for (accountId in signedIn - running) {
                val db = databaseHolder.database(forAccountId = accountId)
                pipelines[accountId] = launchPipeline(accountId, db)
                android.util.Log.i("SyncManager", "Launched shape pipeline (14 shapes) for $accountId")
            }
        }
    }

    private suspend fun cancelPipeline(accountId: String) {
        val pipeline = synchronized(lock) { pipelines.remove(accountId) }
        pipeline?.jobs?.forEach { it.cancel() }
    }

    // MARK: - Per-account shape launch

    private fun launchPipeline(accountId: String, db: ExponentialDatabase): Pipeline {
        // Threaded into every shape so each one reports phase/rows/errors to the
        // Sync Diagnostics screen.
        fun reporter(shape: String) = ShapeReporter(
            onPhase = { phase -> stats.setPhase(accountId, shape, phase) },
            onApplied = { n -> stats.addRows(accountId, shape, n) },
            onError = { authFailure, message, schema ->
                stats.incError(accountId, shape, authFailure = authFailure, message = message, schema = schema)
            },
            onSuccess = { stats.clearError(accountId, shape) },
            onDropped = { cols -> reportDroppedColumns(accountId, shape, cols) },
            onDecodeDrop = { stats.reportDecodeDrop(accountId, shape) },
            onRecovering = { stats.setRecovering(accountId, shape) },
        )

        // Per-account credential providers: read the specific account's URL +
        // token from AuthRepository.accounts at call time, so a future token
        // refresh on one server doesn't disturb any other.
        val baseUrl: () -> String? = {
            auth.accounts.value.firstOrNull { it.id == accountId }?.instanceUrl
        }
        val token: () -> String? = {
            auth.accounts.value.firstOrNull { it.id == accountId }?.token
        }

        val offsetDao = db.electricOffsetDao()
        val teamDao = db.teamDao()
        val boardDao = db.boardDao()
        val issueDao = db.issueDao()
        val labelDao = db.labelDao()
        val issueLabelDao = db.issueLabelDao()
        val userDao = db.userDao()
        val teamMemberDao = db.teamMemberDao()
        val teamInviteDao = db.teamInviteDao()
        val commentDao = db.commentDao()
        val attachmentDao = db.attachmentDao()
        val notificationDao = db.notificationDao()
        val issueSubscriberDao = db.issueSubscriberDao()
        val issueEventDao = db.issueEventDao()
        val codingSessionDao = db.codingSessionDao()

        val shapes = listOf(
            launchShape(
                shape = "teams", path = "/api/shapes/teams", tableName = "teams",
                serializer = TeamEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("teams"),
                onInsert = { teamDao.upsert(it) },
                onUpdate = { teamDao.upsert(it) },
                onDelete = { teamDao.deleteById(it.id) },
                onRefetch = { teamDao.clear() },
            ),
            launchShape(
                shape = "boards", path = "/api/shapes/boards", tableName = "boards",
                serializer = BoardEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("boards"),
                onInsert = { boardDao.upsert(it) },
                onUpdate = { boardDao.upsert(it) },
                onDelete = { boardDao.deleteById(it.id) },
                onRefetch = { boardDao.clear() },
            ),
            launchShape(
                shape = "issues", path = "/api/shapes/issues", tableName = "issues",
                serializer = IssueEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("issues"),
                onInsert = { issueDao.upsert(it) },
                onUpdate = { issueDao.upsert(it) },
                onDelete = { issueDao.deleteById(it.id) },
                onRefetch = { issueDao.clear() },
            ),
            launchShape(
                shape = "labels", path = "/api/shapes/labels", tableName = "labels",
                serializer = LabelEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("labels"),
                onInsert = { labelDao.upsert(it) },
                onUpdate = { labelDao.upsert(it) },
                onDelete = { labelDao.deleteById(it.id) },
                onRefetch = { labelDao.clear() },
            ),
            launchShape(
                shape = "issue_labels", path = "/api/shapes/issue-labels", tableName = "issue_labels",
                serializer = IssueLabelEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("issue_labels"),
                onInsert = { issueLabelDao.upsert(it) },
                onUpdate = { issueLabelDao.upsert(it) },
                onDelete = { issueLabelDao.delete(it.issueId, it.labelId) },
                onRefetch = { issueLabelDao.clear() },
            ),
            launchShape(
                shape = "users", path = "/api/shapes/users", tableName = "users",
                serializer = UserEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("users"),
                onInsert = { userDao.upsert(it) },
                onUpdate = { userDao.upsert(it) },
                onDelete = { userDao.deleteById(it.id) },
                onRefetch = { userDao.clear() },
            ),
            launchShape(
                shape = "team_members", path = "/api/shapes/team-members", tableName = "team_members",
                serializer = TeamMemberEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("team_members"),
                onInsert = { teamMemberDao.upsert(it) },
                onUpdate = { teamMemberDao.upsert(it) },
                onDelete = { teamMemberDao.deleteById(it.id) },
                onRefetch = { teamMemberDao.clear() },
            ),
            launchShape(
                shape = "team_invites", path = "/api/shapes/team-invites", tableName = "team_invites",
                serializer = TeamInviteEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("team_invites"),
                onInsert = { teamInviteDao.upsert(it) },
                onUpdate = { teamInviteDao.upsert(it) },
                onDelete = { teamInviteDao.deleteById(it.id) },
                onRefetch = { teamInviteDao.clear() },
            ),
            launchShape(
                shape = "comments", path = "/api/shapes/comments", tableName = "comments",
                serializer = CommentEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("comments"),
                onInsert = { commentDao.upsert(it) },
                onUpdate = { commentDao.upsert(it) },
                onDelete = { commentDao.deleteById(it.id) },
                onRefetch = { commentDao.clear() },
            ),
            launchShape(
                shape = "attachments", path = "/api/shapes/attachments", tableName = "attachments",
                serializer = AttachmentEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("attachments"),
                onInsert = { attachmentDao.upsert(it) },
                onUpdate = { attachmentDao.upsert(it) },
                onDelete = { attachmentDao.deleteById(it.id) },
                onRefetch = { attachmentDao.clear() },
            ),
            launchShape(
                shape = "notifications", path = "/api/shapes/notifications", tableName = "notifications",
                serializer = NotificationEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("notifications"),
                onInsert = { notificationDao.upsert(it) },
                onUpdate = { notificationDao.upsert(it) },
                onDelete = { notificationDao.deleteById(it.id) },
                onRefetch = { notificationDao.clear() },
            ),
            launchShape(
                shape = "issue_events", path = "/api/shapes/issue-events", tableName = "issue_events",
                serializer = IssueEventEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("issue_events"),
                onInsert = { issueEventDao.upsert(it) },
                onUpdate = { issueEventDao.upsert(it) },
                onDelete = { issueEventDao.deleteById(it.id) },
                onRefetch = { issueEventDao.clear() },
            ),
            launchShape(
                shape = "issue_subscribers", path = "/api/shapes/issue-subscribers", tableName = "issue_subscribers",
                serializer = IssueSubscriberEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("issue_subscribers"),
                onInsert = { issueSubscriberDao.upsert(it) },
                onUpdate = { issueSubscriberDao.upsert(it) },
                onDelete = { issueSubscriberDao.deleteById(it.id) },
                onRefetch = { issueSubscriberDao.clear() },
            ),
            launchShape(
                shape = "coding_sessions", path = "/api/shapes/coding-sessions", tableName = "coding_sessions",
                serializer = CodingSessionEntity.serializer(),
                offsetDao = offsetDao, db = db, baseUrl = baseUrl, token = token,
                reporter = reporter("coding_sessions"),
                onInsert = { codingSessionDao.upsert(it) },
                onUpdate = { codingSessionDao.upsert(it) },
                onDelete = { codingSessionDao.deleteById(it.id) },
                onRefetch = { codingSessionDao.clear() },
            ),
        )
        return Pipeline(jobs = shapes.map { it.first }, clients = shapes.map { it.second })
    }

    private fun <T : Any> launchShape(
        shape: String,
        path: String,
        tableName: String,
        serializer: KSerializer<T>,
        offsetDao: com.exponential.app.data.db.ElectricOffsetDao,
        db: ExponentialDatabase,
        baseUrl: () -> String?,
        token: () -> String?,
        reporter: ShapeReporter = ShapeReporter(),
        onInsert: suspend (T) -> Unit,
        onUpdate: suspend (T) -> Unit,
        onDelete: suspend (T) -> Unit,
        onRefetch: suspend () -> Unit,
    ): Pair<Job, ShapeClient<*>> {
        val shapeClient = ShapeClient(
            client = client,
            baseUrlProvider = baseUrl,
            tokenProvider = token,
            shapeName = shape,
            urlPath = path,
            valueSerializer = serializer,
            offsetDao = offsetDao,
            json = json,
            onPhase = reporter.onPhase,
            onApplied = reporter.onApplied,
            onError = reporter.onError,
            onSuccess = reporter.onSuccess,
            onDecodeDrop = { reporter.onDecodeDrop() },
            onRecovering = reporter.onRecovering,
            // Auto-recovery: mark the shape for a refetch so the next poll
            // pulls a fresh snapshot (the same marker the 409/400 path writes).
            // The rows stay until that snapshot's batch replaces them in one
            // transaction — a local schema drift must not blank the UI first.
            onReset = {
                val saved = offsetDao.get(shape)
                offsetDao.upsert(
                    com.exponential.app.data.db.ElectricOffsetEntity(
                        shape = shape,
                        handle = saved?.handle ?: "",
                        offset = INITIAL_OFFSET,
                        isLive = false,
                        needsRefetch = true,
                    )
                )
            },
            onMessages = { messages ->
                // Apply each long-poll batch in one transaction (parity with iOS
                // applyBatch) so a batch is an atomic write and the concurrent
                // shape loops don't interleave partial writes. Covers both the
                // DAO calls and applyPartialUpdate's raw execSQL (same connection).
                db.withTransaction {
                    for (message in messages) {
                        when (message) {
                            is ShapeMessage.Insert -> onInsert(message.value)
                            is ShapeMessage.Update -> onUpdate(message.value)
                            is ShapeMessage.PartialUpdate ->
                                applyPartialUpdate(db, tableName, message.key, message.columns, reporter.onDropped)
                            // Electric delete/move-out messages carry PK-only (or
                            // partial) payloads, so the full-entity decode usually
                            // fails and `value` is null. Fall back to deleting by
                            // the PK parsed from the Electric key (iOS parity) —
                            // without this every delete was silently dropped.
                            is ShapeMessage.Delete -> message.value?.let { onDelete(it) }
                                ?: deleteByKey(db, tableName, message.key)
                            ShapeMessage.MustRefetch -> onRefetch()
                            ShapeMessage.UpToDate -> Unit
                        }
                    }
                }
            },
        )
        return scope.launch { shapeClient.run() } to shapeClient
    }
}

// One unforced kick per second at most: app-foreground, network-available and a
// burst of pushes routinely coincide, and each kick drops 14 live long-polls.
private const val KICK_DEBOUNCE_MS = 1_000L

/** Per-shape diagnostics callbacks passed from [SyncManager] into [ShapeClient]. */
private class ShapeReporter(
    val onPhase: (String) -> Unit = {},
    val onApplied: (Int) -> Unit = {},
    // (authFailure, message, schemaError): authFailure is HTTP 401/403; schema
    // is true for "no such column/table" class SQLite failures.
    val onError: (Boolean, String?, Boolean) -> Unit = { _, _, _ -> },
    val onSuccess: () -> Unit = {},
    // Wire columns a partial-update dropped because the local schema predates them.
    val onDropped: (Set<String>) -> Unit = {},
    // A full-row insert was dropped because it failed to decode.
    val onDecodeDrop: () -> Unit = {},
    // An auto-reset of this shape has begun.
    val onRecovering: () -> Unit = {},
)

// One schema cache for the whole process: every account's Room instance shares
// the same table definitions, so the PRAGMA read is done once per table.
private val schemaCache = SchemaCache()

private fun parseIdFromKey(key: String): String? {
    val last = key.split("/").lastOrNull() ?: return null
    return last.trim('"')
}

/**
 * Delete a row by its Electric key when the `delete` message carries no
 * decodable value (mirrors iOS `deleteByKey`). Resolves the table's PK columns
 * via [SchemaCache] so composite-PK tables (issue_labels) work too; pure
 * planning lives in [planDeleteByKey]. Runs on the same connection as the
 * batch transaction (like [applyPartialUpdate]).
 */
private fun deleteByKey(db: ExponentialDatabase, table: String, key: String) {
    val schema = schemaCache.of(db.openHelper.writableDatabase, table)
    val plan = planDeleteByKey(schema.pkColumns, key) ?: return
    db.openHelper.writableDatabase.execSQL(
        "DELETE FROM \"$table\" WHERE ${plan.whereClause}",
        plan.args.toTypedArray(),
    )
}

/**
 * Tolerant partial-apply: filter the wire columns to what the local schema
 * actually has (via [SchemaCache]) and skip composite-PK tables entirely, so a
 * server column the client predates (or a join table keyed on something other
 * than `id`) can never abort the batch transaction and freeze the offset. Pure
 * planning lives in [planPartialUpdate]; this method only does I/O + reporting.
 */
private fun applyPartialUpdate(
    db: ExponentialDatabase,
    table: String,
    key: String,
    columnsJson: String,
    onDropped: (Set<String>) -> Unit,
) {
    val id = parseIdFromKey(key) ?: return
    val columns = try {
        kotlinx.serialization.json.Json.parseToJsonElement(columnsJson).jsonObject
    } catch (_: Exception) { return }

    val schema = schemaCache.of(db.openHelper.writableDatabase, table)
    val plan = planPartialUpdate(schema.pkColumns, schema.columns, columns, schema.integerColumns) ?: return
    if (plan.droppedColumns.isNotEmpty()) onDropped(plan.droppedColumns)
    // Pure-unknown partial: no known columns to write, but returning cleanly
    // lets the offset advance instead of refailing the batch forever.
    if (plan.setClause.isEmpty()) return

    db.openHelper.writableDatabase.execSQL(
        "UPDATE \"$table\" SET ${plan.setClause} WHERE \"id\" = ?",
        (plan.args + id).toTypedArray(),
    )
}

/**
 * Lazily-read, per-app-run cache of each table's primary-key columns + full
 * column set + INTEGER-affinity columns, from `PRAGMA table_info`. Shared
 * across account DBs — every account's Room instance has the identical schema.
 */
internal class SchemaCache {
    data class TableSchema(
        val pkColumns: List<String>,
        val columns: Set<String>,
        val integerColumns: Set<String>,
    )

    private val lock = Any()
    private val byTable = mutableMapOf<String, TableSchema>()

    fun of(db: SupportSQLiteDatabase, table: String): TableSchema = synchronized(lock) {
        byTable.getOrPut(table) { read(db, table) }
    }

    private fun read(db: SupportSQLiteDatabase, table: String): TableSchema {
        val columns = linkedSetOf<String>()
        val integerColumns = linkedSetOf<String>()
        val pk = mutableListOf<Pair<Int, String>>()
        db.query("PRAGMA table_info(\"$table\")").use { cursor ->
            val nameIdx = cursor.getColumnIndex("name")
            val typeIdx = cursor.getColumnIndex("type")
            val pkIdx = cursor.getColumnIndex("pk")
            while (cursor.moveToNext()) {
                val name = cursor.getString(nameIdx)
                columns.add(name)
                // SQLite affinity rule 1: any declared type containing "INT" is
                // INTEGER affinity — covers Room's Boolean columns (EXP-185).
                if (cursor.getString(typeIdx).uppercase().contains("INT")) integerColumns.add(name)
                val order = cursor.getInt(pkIdx)
                if (order > 0) pk.add(order to name)
            }
        }
        return TableSchema(
            pkColumns = pk.sortedBy { it.first }.map { it.second },
            columns = columns,
            integerColumns = integerColumns,
        )
    }
}
