package com.exponential.app.ui.stories

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.AnswerInput
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.thread.AskAnswer
import com.exponential.app.ui.thread.PlanAnswer
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** A ring's avatar (stories.json AVATARS): an agent's icon in its colour,
 *  else ([color] null, a chat's root) the chat row's bare brand mark of the
 *  root's [cli]. */
data class StoryAvatar(val icon: String?, val color: String?, val cli: String? = null)

/** What the ring row + the viewer draw: the active team's rings, each
 *  story's card as THE thread row (the viewer's body + footer read it) and
 *  the rings' avatars. */
data class StoriesState(
    val rings: List<Stories.StoryRing> = emptyList(),
    val rows: Map<String, SessionTree.ThreadRow> = emptyMap(),
    val avatars: Map<String, StoryAvatar> = emptyMap(),
) {
    fun avatar(ring: Stories.StoryRing): StoryAvatar = avatars[ring.key] ?: StoryAvatar(null, null)
}

/** STORIES TAB: the root's ONE ring ([Stories.storyTab]; null = the empty
 *  state) + what the viewer draws it with; [loading] until the first fold. */
data class StoryTabState(
    val ring: Stories.StoryRing? = null,
    val draw: StoriesState = StoriesState(),
    val loading: Boolean = true,
)

/**
 * S-3 STORIES (stories.json): the fold's input off the synced rows — the
 * `sessions` (named by THE session title, the chat list's rung over the
 * conversations' digests), the `agents` definitions (no role column synced:
 * `assistant` = false), every session's OWN cards (`messages`, the native
 * shape syncs every visible one) and the viewer's `session_reads`
 * (`story_seen_at` included); the ACTIVE team narrows it, a minute clock
 * re-ages it. Seen = `sessions.markStoriesSeen` on show; answers go through
 * THE card-answer path (`sessions.answer`, the inline card's call).
 */
@HiltViewModel
class StoriesViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val sessionsApi: SessionsApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val sessionRows = dbFlow.scopedQuery(emptyList()) { it.sessionDao().observeAll() }

    private val sessions = sessionRows.map { rows -> rows.map(SessionTree::session) }

    /** Session id → its CLI (a root chat ring's brand mark). */
    private val clis: Flow<Map<String, String?>> = sessionRows.map { rows -> rows.associate { it.id to it.cli } }

    /** Session id → its title (THE session title over the digests). */
    private val titles: Flow<Map<String, String>> = combine(
        sessions,
        dbFlow.scopedQuery(emptyList()) { it.sessionIssueDao().observeAll() }.map { it.map(SessionTree::sessionIssue) },
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() }.map { it.map(SessionTree::issue) },
        dbFlow.scopedQuery(emptyList()) { it.conversationDao().observeAll() }.map { rows ->
            rows.flatMap { row ->
                val session = row.sessionId ?: return@flatMap emptyList()
                Chats.digestMessages(session, Chats.parseDigest(row.digest))
            }
        },
    ) { s, links, issues, digests -> SessionTree.sessionTitles(s, links, issues, digests) }

    private val cards: Flow<List<SessionTree.Message>> =
        dbFlow.scopedQuery(emptyList()) { it.messageDao().observeOwnCards() }.map { rows ->
            rows.mapNotNull { row ->
                val author = row.authorSessionId ?: return@mapNotNull null
                SessionTree.message(row, author).takeIf { Stories.isStoryCard(it.card) }
            }
        }

    private val agents = dbFlow.scopedQuery(emptyList()) { it.agentDefinitionDao().observeAll() }

    private val reads = dbFlow.scopedQuery(emptyList()) { it.sessionReadDao().observeAll() }

    /** The fold's clock: ages re-read once a minute. */
    private val minuteClock: Flow<Long> = flow {
        while (true) {
            emit(System.currentTimeMillis())
            delay(60_000)
        }
    }

    /** The fold's input off the synced rows (every team), plus what the
     *  hosts draw with it (the cards, the agents, the CLIs). */
    private class Inputs(
        val now: Long,
        val sessions: List<SessionTree.Session>,
        val names: Map<String, String>,
        val clis: Map<String, String?>,
        val messages: List<SessionTree.Message>,
        val defs: List<com.exponential.app.data.db.AgentDefinitionEntity>,
        val reads: List<Stories.ReadInput>,
        val team: String?,
    ) {
        fun sessionInputs(filter: (SessionTree.Session) -> Boolean) = sessions.filter(filter).map {
            Stories.SessionInput(
                id = it.id,
                rootId = it.rootId,
                name = names[it.id].orEmpty(),
                agentId = it.agentId,
                startedByAgentId = it.startedByAgentId,
            )
        }

        fun messageInputs(ids: Set<String>?) = messages.filter { ids == null || it.sessionId in ids }.mapNotNull { m ->
            val created = WireTimestamps.parseEpochMs(m.createdAt) ?: return@mapNotNull null
            Stories.MessageInput(
                id = m.id,
                sessionId = m.sessionId,
                authorSessionId = m.authorSessionId,
                createdAt = created,
                answeredAt = m.answeredAt?.let(WireTimestamps::parseEpochMs),
                card = m.card,
            )
        }

        val agents get() = defs.map { Stories.AgentInput(it.id, it.name, assistant = false) }

        /** The rings' cards as THE thread rows + the rings' avatars. */
        fun draw(rings: List<Stories.StoryRing>): StoriesState {
            val shown = rings.flatMapTo(HashSet()) { ring -> ring.stories.map { it.id } }
            val rows = messages.filter { it.id in shown }.mapNotNull { m ->
                (SessionTree.surfaceRow(m) ?: SessionTree.askRow(m))?.let { m.id to it }
            }.toMap()
            val byAgent = defs.associateBy { it.id }
            val avatars = rings.associate { ring ->
                val agent = ring.agentId?.let { byAgent[it] }
                ring.key to if (ring.agentId != null) {
                    StoryAvatar(
                        agent?.icon ?: SessionTree.ListCopy.AGENT_DEFAULT_ICON,
                        agent?.color ?: SessionTree.ListCopy.AGENT_DEFAULT_COLOR,
                    )
                } else {
                    StoryAvatar(null, null, ring.rootId?.let { clis[it] })
                }
            }
            return StoriesState(rings, rows, avatars)
        }
    }

    private val inputs: Flow<Inputs> = combine(
        combine(sessions, titles, clis) { s, t, c -> Triple(s, t, c) },
        cards,
        combine(agents, reads) { a, r -> a to r },
        selection.selectedId,
        minuteClock,
    ) { (s, names, cliOf), messages, (defs, readRows), team, now ->
        Inputs(
            now = now,
            sessions = s,
            names = names,
            clis = cliOf,
            messages = messages,
            defs = defs,
            reads = readRows.map {
                Stories.ReadInput(
                    sessionId = it.sessionId,
                    readAt = WireTimestamps.parseEpochMs(it.readAt),
                    storySeenAt = it.storySeenAt?.let(WireTimestamps::parseEpochMs),
                )
            },
            team = team,
        )
    }

    val state: StateFlow<StoriesState> = inputs.map { input ->
        val inTeam = input.sessionInputs { input.team == null || it.teamId == input.team }
        val ids = inTeam.mapTo(HashSet()) { it.id }
        input.draw(
            Stories.rings(
                now = input.now,
                sessions = inTeam,
                agents = input.agents,
                messages = input.messageInputs(ids),
                reads = input.reads,
            ),
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), StoriesState())

    private val tabs = HashMap<String, StateFlow<StoryTabState>>()

    /** STORIES TAB: [Stories.storyTab] of [rootId] (the root's whole tree,
     *  never expiring) as ONE ring the viewer opens on. */
    fun tab(rootId: String): StateFlow<StoryTabState> = tabs.getOrPut(rootId) {
        inputs.map { input ->
            val tree = input.sessionInputs { it.rootId == rootId }
            val ring = Stories.storyTab(
                now = input.now,
                sessions = tree,
                agents = input.agents,
                messages = input.messageInputs(tree.mapTo(HashSet()) { it.id }),
                reads = input.reads,
                rootId = rootId,
            )
            StoryTabState(ring = ring, draw = input.draw(listOfNotNull(ring)), loading = false)
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), StoryTabState())
    }

    private val _failure = MutableStateFlow<FailedAction?>(null)
    /** A refused answer, raised once as the fixture's toast. */
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    /** A story was SHOWN: the root's `story_seen_at` keeps the greatest
     *  createdAt. Background, so a failure stays silent. */
    fun markSeen(story: Stories.Story) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value ?: return@launch
            try {
                sessionsApi.markStoriesSeen(accountId, story.rootId, story.createdAt)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
            }
        }
    }

    /** A question story's answer (THE inline card's `sessions.answer`). */
    fun answer(messageId: String, answer: AskAnswer, settled: (Boolean) -> Unit) =
        run("answer", "the question", settled) { accountId ->
            sessionsApi.answer(
                accountId,
                AnswerInput(
                    messageId = messageId,
                    answer = answer.answer,
                    answers = answer.answers?.let(AnswerInput::questionAnswers),
                    surfaceData = answer.surfaceData,
                ),
            )
        }

    /** A plan story's decision (THE inline card's `sessions.answer`). */
    fun decidePlan(messageId: String, decision: PlanAnswer, settled: (Boolean) -> Unit) =
        run("answer", "the plan", settled) { accountId ->
            sessionsApi.answer(
                accountId,
                AnswerInput(
                    messageId = messageId,
                    decision = if (decision.approved) "approved" else "rejected",
                    fresh = if (decision.approved && decision.fresh) true else null,
                    feedback = decision.feedback,
                    answers = decision.answers?.let(AnswerInput::planAnswers),
                    surfaceData = decision.surfaceData,
                ),
            )
        }

    private fun run(verb: String, thing: String, settled: (Boolean) -> Unit, call: suspend (String) -> Unit) {
        viewModelScope.launch {
            val accountId = auth.activeAccountId.value
            if (accountId == null) {
                settled(false)
                return@launch
            }
            try {
                call(accountId)
                settled(true)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                settled(false)
                _failure.value = FailedAction(verb, thing, trpcErrorMessage(t, ""))
            }
        }
    }
}
