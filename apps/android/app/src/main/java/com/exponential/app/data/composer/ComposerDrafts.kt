package com.exponential.app.data.composer

import com.exponential.app.data.api.ComposerDraftsApi
import com.exponential.app.data.api.UpsertComposerDraftInput
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.domain.ComposerDraft
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map

/**
 * #FEED-219: the app's one composer-draft writer ([ComposerDraftWriter] over
 * tRPC + Room) and the synced/local reads every [ComposerDraftBinding] follows.
 * App-scoped: a write outlives the screen that typed it.
 */
@Singleton
class ComposerDrafts @Inject constructor(
    private val api: ComposerDraftsApi,
    private val holder: DatabaseHolder,
) {
    val scope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    val writer = ComposerDraftWriter(
        scope,
        object : ComposerDraftWriter.Transport {
            override suspend fun upsert(accountId: String, teamId: String, key: String, text: String) {
                val row = api.upsert(accountId, UpsertComposerDraftInput(teamId = teamId, key = key, text = text))
                // The shape echoes it too; mirroring now keeps `synced` equal
                // to what was written the moment the local copy goes.
                holder.database(forAccountId = accountId).composerDraftDao().upsert(row)
            }

            override suspend fun delete(accountId: String, key: String) {
                api.delete(accountId, key)
                holder.database(forAccountId = accountId).composerDraftDao().deleteByKey(key)
            }
        },
    )

    /** What an untouched field shows for [id]: local, else synced, else empty. */
    fun shown(id: ComposerDraftWriter.Id): Flow<String> =
        combine(
            writer.localFlow(id),
            holder.database(forAccountId = id.accountId).composerDraftDao().observeByKey(id.key).map { it?.text },
        ) { local, synced -> ComposerDraft.field(field = "", edited = false, local = local, synced = synced) }

    fun flushAll() = writer.flushAll()
}
