package com.exponential.app.data.search

import android.content.Context
import android.os.Build
import android.util.Log
import androidx.appsearch.app.AppSearchSchema
import androidx.appsearch.app.AppSearchSession
import androidx.appsearch.app.GenericDocument
import androidx.appsearch.app.PutDocumentsRequest
import androidx.appsearch.app.SearchSpec
import androidx.appsearch.app.SetSchemaRequest
import androidx.appsearch.platformstorage.PlatformStorage
import androidx.concurrent.futures.await
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.push.PushDeepLinks
import com.exponential.app.domain.SessionTitle
import com.exponential.app.domain.ShareTargets
import com.exponential.app.domain.SystemSearch
import dagger.hilt.android.qualifiers.ApplicationContext
import java.util.concurrent.Executors
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/**
 * #EXP-1642 (system-search.json): every signed-in account's issues and plain
 * chats in the platform AppSearch index, one database per account (its
 * domain), the schema type displayed by system UI. Re-synced at most once per
 * `debounceMs` after the store changes: the current items upserted, the ids
 * that left removed. A signed-out account's database is emptied. Never a
 * description, comment or message; never an encrypted chat. A document's
 * `intentUri` is the push-tap link (`exponential://issue|session/<id>?userId=`),
 * so a result opens like a push tap under the domain's account.
 *
 * Platform storage = API 31+; below that the phone has no system AppSearch.
 */
@OptIn(FlowPreview::class)
@Singleton
class SystemSearchIndexer @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    private val dbs: DatabaseHolder,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val executor = Executors.newSingleThreadExecutor()
    private val prefs by lazy { context.getSharedPreferences(PREFS, Context.MODE_PRIVATE) }

    /** The ids each account's database holds (this process; a first pass rewrites it whole). */
    private val indexed = mutableMapOf<String, Set<String>>()

    fun start() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return
        scope.launch {
            auth.accounts
                .map { accounts -> accounts.filter { it.token != null }.associate { it.id to it.userId } }
                .distinctUntilChanged()
                .collectLatest { signedIn ->
                    // A sign-out (or a removed account): its domain goes, nothing else.
                    val known = prefs.getStringSet(KEY_ACCOUNTS, emptySet()).orEmpty()
                    for (gone in known - signedIn.keys) clear(gone)
                    prefs.edit().putStringSet(KEY_ACCOUNTS, signedIn.keys).apply()
                    coroutineScope {
                        for ((accountId, userId) in signedIn) launch { follow(accountId, userId) }
                    }
                }
        }
    }

    private suspend fun follow(accountId: String, userId: String?) {
        val db = dbs.database(accountId)
        val caps = SystemSearch.PRODUCTION_CAPS
        combine(
            db.boardDao().observeAll(),
            db.issueDao().observeAll(),
            db.sessionDao().observeSearchableChats(caps.maxChats),
        ) { boards, issues, sessions ->
            val live = boards.filter { it.deletedAt == null }
            val liveIds = live.map { it.id }.toSet()
            SystemSearch.systemSearchItems(
                boards = live.map { SystemSearch.Board(it.id, it.name) },
                issues = issues.map {
                    SystemSearch.Issue(it.id, it.identifier, it.title, it.boardId, it.updatedAt, trashed = it.boardId !in liveIds)
                },
                chats = sessions.map {
                    ShareTargets.Chat(
                        it.id,
                        it.title?.trim()?.takeIf(String::isNotEmpty) ?: SessionTitle.UNTITLED,
                        it.updatedAt,
                        it.agentId,
                        it.sealPublicKey != null || it.sealedTitle != null,
                    )
                },
                caps = caps,
            )
        }
            .debounce(SystemSearch.DEBOUNCE_MS)
            .distinctUntilChanged()
            .collect { items -> runCatchingIndex { index(accountId, userId, items) } }
    }

    private suspend fun index(accountId: String, userId: String?, items: List<SystemSearch.Item>) {
        withSession(accountId) { session ->
            ensureSchema(session)
            val previous = indexed[accountId]
            if (previous == null) {
                // First pass in this process: whatever an earlier run left goes.
                session.removeAsync("", SearchSpec.Builder().build()).await()
            }
            val next = items.map { it.id }.toSet()
            val left = previous.orEmpty() - next
            if (left.isNotEmpty()) {
                session.removeAsync(
                    androidx.appsearch.app.RemoveByDocumentIdRequest.Builder(NAMESPACE).addIds(left).build(),
                ).await()
            }
            items.chunked(PUT_BATCH).forEach { batch ->
                val docs = batch.map { document(it, userId) }
                session.putAsync(PutDocumentsRequest.Builder().addGenericDocuments(docs).build()).await()
            }
            indexed[accountId] = next
        }
    }

    /** Empties the signed-out account's database (an empty, forced schema). */
    private suspend fun clear(accountId: String) {
        runCatchingIndex {
            withSession(accountId) { session ->
                session.setSchemaAsync(SetSchemaRequest.Builder().setForceOverride(true).build()).await()
            }
        }
        indexed.remove(accountId)
    }

    private suspend fun ensureSchema(session: AppSearchSession) {
        val request = SetSchemaRequest.Builder()
            .addSchemas(SCHEMA)
            .setSchemaTypeDisplayedBySystem(SCHEMA_TYPE, true)
            .setForceOverride(true)
            .build()
        session.setSchemaAsync(request).await()
    }

    private fun document(item: SystemSearch.Item, userId: String?): GenericDocument {
        val target = when (SystemSearch.parseSystemSearchId(item.id)?.kind) {
            SystemSearch.Kind.Issue -> PushDeepLinks.Target.Issue(item.id.removePrefix(SystemSearch.ISSUE_PREFIX))
            else -> PushDeepLinks.Target.Session(item.id.removePrefix(SystemSearch.CHAT_PREFIX))
        }
        return GenericDocument.Builder<GenericDocument.Builder<*>>(NAMESPACE, item.id, SCHEMA_TYPE)
            .setPropertyString(PROP_TITLE, item.title)
            .setPropertyString(PROP_SUBTITLE, item.subtitle)
            .setPropertyString(PROP_KEYWORDS, *item.keywords.toTypedArray())
            .setPropertyString(PROP_INTENT_URI, PushDeepLinks.uri(target, userId))
            .build()
    }

    /** A session on the account's database; nothing below API 31 (platform
     *  AppSearch storage: [start] never runs there, the guard keeps lint and
     *  any other caller honest). */
    private suspend fun withSession(accountId: String, block: suspend (AppSearchSession) -> Unit) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return
        val session = PlatformStorage.createSearchSessionAsync(
            PlatformStorage.SearchContext.Builder(context, SystemSearch.domain(accountId).replace('/', '_'))
                .setWorkerExecutor(executor)
                .build(),
        ).await()
        try {
            block(session)
        } finally {
            session.close()
        }
    }

    private suspend fun runCatchingIndex(block: suspend () -> Unit) {
        try {
            block()
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            // Best effort: the next store change retries; titles never reach logcat.
            Log.w(TAG, "system search index failed: ${err.javaClass.simpleName}")
        }
    }

    private companion object {
        const val TAG = "SystemSearch"
        const val PREFS = "system_search"
        const val KEY_ACCOUNTS = "indexed_accounts"
        const val NAMESPACE = "items"
        const val SCHEMA_TYPE = "ExponentialItem"
        const val PROP_TITLE = "title"
        const val PROP_SUBTITLE = "subtitle"
        const val PROP_KEYWORDS = "keywords"
        const val PROP_INTENT_URI = "intentUri"
        const val PUT_BATCH = 100

        private fun indexed(name: String) = AppSearchSchema.StringPropertyConfig.Builder(name)
            .setCardinality(AppSearchSchema.PropertyConfig.CARDINALITY_OPTIONAL)
            .setIndexingType(AppSearchSchema.StringPropertyConfig.INDEXING_TYPE_PREFIXES)
            .setTokenizerType(AppSearchSchema.StringPropertyConfig.TOKENIZER_TYPE_PLAIN)
            .build()

        val SCHEMA: AppSearchSchema = AppSearchSchema.Builder(SCHEMA_TYPE)
            .addProperty(indexed(PROP_TITLE))
            .addProperty(indexed(PROP_SUBTITLE))
            .addProperty(
                AppSearchSchema.StringPropertyConfig.Builder(PROP_KEYWORDS)
                    .setCardinality(AppSearchSchema.PropertyConfig.CARDINALITY_REPEATED)
                    .setIndexingType(AppSearchSchema.StringPropertyConfig.INDEXING_TYPE_EXACT_TERMS)
                    .setTokenizerType(AppSearchSchema.StringPropertyConfig.TOKENIZER_TYPE_PLAIN)
                    .build(),
            )
            .addProperty(
                AppSearchSchema.StringPropertyConfig.Builder(PROP_INTENT_URI)
                    .setCardinality(AppSearchSchema.PropertyConfig.CARDINALITY_OPTIONAL)
                    .setIndexingType(AppSearchSchema.StringPropertyConfig.INDEXING_TYPE_NONE)
                    .setTokenizerType(AppSearchSchema.StringPropertyConfig.TOKENIZER_TYPE_NONE)
                    .build(),
            )
            .build()
    }
}
