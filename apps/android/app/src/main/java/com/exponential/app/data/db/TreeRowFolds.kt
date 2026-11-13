package com.exponential.app.data.db

import androidx.room.ColumnInfo
import com.exponential.app.data.sealed.SealedContent
import com.exponential.app.data.sealed.SealedKeys
import com.exponential.app.data.sealed.TreeKeys
import com.exponential.app.domain.ResultsTab
import com.exponential.app.domain.SessionTitle
import com.exponential.app.domain.SessionTree
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map

/**
 * #ERR-19: what makes one message row differ, read WITHOUT its body or card
 * (a chat's tree-wide reads re-run on every write to `messages`: they compare
 * these cheap stamps and load only the rows that changed).
 */
data class RowStamp(
    val id: String,
    val stamp: String,
)

/** #ERR-19: one session's title-eligible rows, summarized (count + newest stamp). */
data class TitleStamp(
    @ColumnInfo(name = "session_id") val sessionId: String,
    val stamp: String,
)

/**
 * #ERR-19 (results-tab.json): the Results tab's input, folded INCREMENTALLY.
 * The tab reads a session's OWN result cards over the whole tree (never the
 * loaded window); a trickle of new messages used to re-read, re-open and
 * re-parse every session-authored card of the tree per write (a busy
 * Assistant chat: thousands of plans, asks and reports, 5–10 KB each), and
 * kept them all. Now: a row is loaded only when its stamp is new or changed,
 * in pages, and only a RESULT card ([ResultsTab.isResultCard]) is kept.
 */
internal class ResultCardsFold(page: Int = LOAD_PAGE) : CardRowsFold(page, { ResultsTab.isResultCard(it.card) })

/**
 * #ERR-19: card rows folded INCREMENTALLY: rows are read as [RowStamp]s, a row
 * is loaded (in pages) and opened only when its stamp is new or changed, and
 * only the messages [keep] accepts are kept. An unchanged kept message keeps
 * its instance (no re-parse, no recomposition).
 */
internal open class CardRowsFold(
    private val page: Int = LOAD_PAGE,
    private val keep: (SessionTree.Message) -> Boolean,
) {
    private var memo: Map<String, Pair<String, SessionTree.Message?>> = emptyMap()
    private var memoKeys: TreeKeys? = null

    /** How many rows the last [update] loaded (tests). */
    var lastLoaded = 0
        private set

    suspend fun update(
        stamps: List<RowStamp>,
        keys: TreeKeys,
        load: suspend (ids: List<String>) -> List<MessageEntity>,
    ): List<SessionTree.Message> {
        // New keys can open rows that stayed locked: every row again.
        if (keys !== memoKeys) {
            memo = emptyMap()
            memoKeys = keys
        }
        val fresh = HashMap<String, Pair<String, SessionTree.Message?>>()
        val missing = stamps.filter { memo[it.id]?.first != it.stamp }
        val stampById = missing.associate { it.id to it.stamp }
        lastLoaded = 0
        for (chunk in missing.chunked(page)) {
            for (row in load(chunk.map { it.id })) {
                lastLoaded++
                val stamp = stampById[row.id] ?: continue
                fresh[row.id] = stamp to keptMessage(row, keys)
            }
        }
        val next = HashMap<String, Pair<String, SessionTree.Message?>>(stamps.size)
        for (s in stamps) next[s.id] = fresh[s.id] ?: memo[s.id]?.takeIf { it.first == s.stamp } ?: (s.stamp to null)
        memo = next
        return stamps.mapNotNull { next[it.id]?.second }
    }

    private fun keptMessage(row: MessageEntity, keys: TreeKeys): SessionTree.Message? {
        val sessionId = row.sessionId?.takeIf { it.isNotEmpty() } ?: return null
        val message = SessionTree.message(SealedContent.message(row, keys), sessionId)
        return message.takeIf(keep)
    }

    companion object {
        const val LOAD_PAGE = 100
    }
}

/**
 * #ERR-19 (session-title.json `firstPrompt`): a session's title reads its
 * FIRST usable prompt, so only a session's oldest title-eligible rows up to
 * and including that one are ever loaded (paged, [FIRST_PAGE] first), never
 * whole transcripts of every session a chat's cards name. Re-read per session
 * only when its [TitleStamp] changes or keys arrive.
 */
internal class FirstTitleRows {
    private var memo: Map<String, Pair<String, List<MessageEntity>>> = emptyMap()
    private var memoKeys: TreeKeys? = null

    suspend fun update(
        stamps: List<TitleStamp>,
        keys: TreeKeys,
        page: suspend (sessionId: String, limit: Int, offset: Int) -> List<MessageEntity>,
    ): List<MessageEntity> {
        if (keys !== memoKeys) {
            memo = emptyMap()
            memoKeys = keys
        }
        val next = HashMap<String, Pair<String, List<MessageEntity>>>(stamps.size)
        for (s in stamps) {
            val have = memo[s.sessionId]
            next[s.sessionId] = if (have != null && have.first == s.stamp) have else s.stamp to firstRows(s.sessionId, keys, page)
        }
        memo = next
        return next.values.flatMap { it.second }.sortedWith(compareBy<MessageEntity>({ it.createdAt }, { it.id }))
    }

    private suspend fun firstRows(
        sessionId: String,
        keys: TreeKeys,
        page: suspend (String, Int, Int) -> List<MessageEntity>,
    ): List<MessageEntity> {
        val out = ArrayList<MessageEntity>()
        var offset = 0
        var limit = FIRST_PAGE
        while (true) {
            val rows = page(sessionId, limit, offset)
            for (row in rows) {
                val opened = SealedContent.message(row, keys)
                out += opened
                if (SessionTitle.isUsablePrompt(SealedContent.openMessage(opened, keys).let { if (it.locked != null) "" else it.body })) return out
            }
            if (rows.size < limit) return out
            offset += rows.size
            limit = minOf(limit * 4, MAX_PAGE)
        }
    }

    companion object {
        const val FIRST_PAGE = 4
        const val MAX_PAGE = 256
    }
}

/** #ERR-19: [ResultCardsFold] over [MessageDao]'s stamps of a tree. */
fun MessageDao.observeResultCards(rootId: String): Flow<List<SessionTree.Message>> = flow {
    val fold = ResultCardsFold()
    emitAll(
        combine(observeResultCardStamps(rootId).distinctUntilChanged(), SealedKeys.keys) { stamps, keys -> stamps to keys }
            .map { (stamps, keys) -> fold.update(stamps, keys) { ids -> cardRowsByIds(ids) } },
    )
}.distinctUntilChanged().flowOn(Dispatchers.Default)

/**
 * #ERR-19: one session's card rows of the given [kinds] (`card.kind`),
 * opened, oldest first, folded incrementally ([CardRowsFold]): the session
 * view's quick replies and tool cards. A busy root (an Assistant chat) holds
 * thousands of report / ask / `thread_started` cards nobody of these reads.
 */
fun MessageDao.observeCardRowsBySession(sessionId: String, kinds: Set<String>): Flow<List<SessionTree.Message>> = flow {
    val fold = CardRowsFold { message ->
        val kind = (message.card?.get("kind") as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content
        kind != null && kind in kinds
    }
    emitAll(
        combine(observeCardRowStampsBySession(sessionId).distinctUntilChanged(), SealedKeys.keys) { stamps, keys -> stamps to keys }
            .map { (stamps, keys) -> fold.update(stamps, keys) { ids -> cardRowsByIds(ids) } },
    )
}.distinctUntilChanged().flowOn(Dispatchers.Default)

/** #ERR-19: the title rows ([FirstTitleRows]) of the given sessions. */
fun MessageDao.observeFirstTitleRows(sessionIds: List<String>): Flow<List<MessageEntity>> =
    firstTitleRows(observeTitleStamps(sessionIds))

/** #ERR-19: the title rows ([FirstTitleRows]) of every session of one tree. */
fun MessageDao.observeFirstTitleRowsByRoot(rootId: String): Flow<List<MessageEntity>> =
    firstTitleRows(observeTitleStampsByRoot(rootId))

private fun MessageDao.firstTitleRows(stamps: Flow<List<TitleStamp>>): Flow<List<MessageEntity>> = flow {
    val fold = FirstTitleRows()
    emitAll(
        combine(stamps.distinctUntilChanged(), SealedKeys.keys) { s, keys -> s to keys }
            .map { (s, keys) -> fold.update(s, keys) { id, limit, offset -> titleRowsPage(id, limit, offset) } },
    )
}.distinctUntilChanged().flowOn(Dispatchers.Default)
