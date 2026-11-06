package com.exponential.app.data.intents

import android.content.Context
import android.widget.Toast
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.CreateIssueInput
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.api.SessionsSendInput
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.push.DeepLinkBus
import com.exponential.app.domain.AppIntents
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.SessionTree
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * #EXP-1640 (app-intents.json): an App Action (Gemini / Assistant) on the
 * ACTIVE account. `create-issue` files the title through THE issues.create
 * path the New issue page uses (no other field: status = the board default)
 * on [AppIntents.intentBoard], answers `copy.created` and opens THE issue
 * page. `ask-agent` sends the text to the matched agent's conversation
 * through THE composer's send path ([SessionsApi.send]: sealed on the phone
 * when the tree has a key) and opens that conversation. Signed out =
 * `copy.signedOut`, nothing runs. A failed call = the toast the app's
 * failed actions use, nothing else.
 */
@Singleton
class AppIntentRunner @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    private val dbs: DatabaseHolder,
    private val selection: TeamSelection,
    private val issuesApi: IssuesApi,
    private val sessionsApi: SessionsApi,
    private val deepLinks: DeepLinkBus,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    fun createIssue(spokenTitle: String?) = run("create", "the issue") {
        val accountId = signedInAccount() ?: return@run toast(AppIntents.SIGNED_OUT)
        val title = AppIntents.intentIssueTitle(spokenTitle.orEmpty()) ?: return@run toast(AppIntents.ASK_TITLE)
        val db = dbs.database(accountId)
        val team = selection.selectedId.value
        val boards = (if (team != null) db.boardDao().observeByTeam(team).first() else db.boardDao().observeAll().first())
            .filter { it.deletedAt == null }
        val boardId = AppIntents.intentBoard(
            boards.map { AppIntents.Board(it.id, it.sortOrder) },
            lastBoardId = selection.lastBoard(accountId),
            pickedId = null,
        ) ?: return@run toast(AppIntents.NO_BOARD)
        val created = issuesApi.create(accountId, CreateIssueInput(boardId = boardId, title = title))
        runCatching { db.issueDao().upsert(created) }
        toast(AppIntents.created(created.identifier))
        deepLinks.openIssue(created.id)
    }

    fun askAgent(spokenAgent: String?, text: String?) = run("send", "the message") {
        val accountId = signedInAccount() ?: return@run toast(AppIntents.SIGNED_OUT)
        val message = text?.trim()?.takeIf { it.isNotEmpty() } ?: return@run
        val me = auth.userId.value
        val db = dbs.database(accountId)
        val team = selection.selectedId.value ?: return@run
        val listed = SessionTree.listAgents(
            db.agentDefinitionDao().observeAll().first().map(SessionTree::agentDefinition),
            emptyList(),
            team,
            me,
        )
        val agents = listed.mapIndexed { index, a ->
            AppIntents.Agent(
                id = a.agentId,
                name = a.name,
                sortOrder = index,
                role = if (a.assistant) DomainContract.agentRoleAssistant else DomainContract.agentRoleAgent,
                ownerId = if (a.assistant) me else null,
            )
        }
        val spoken = spokenAgent.orEmpty()
        val agentId = AppIntents.matchIntentAgent(agents, me, spoken)
            ?: return@run toast(AppIntents.noAgent(spoken.trim()))
        val result = sessionsApi.send(accountId, SessionsSendInput.toAgent(agentId, message))
        toast(AppIntents.sent(agents.first { it.id == agentId }.name))
        deepLinks.openSession(result.sessionId)
    }

    /** The active account when it holds a session, else null (signed out). */
    private fun signedInAccount(): String? {
        val id = auth.activeAccountId.value ?: return null
        return id.takeIf { auth.token.value != null && auth.accounts.value.any { it.id == id && it.token != null } }
    }

    /** feedback-states.json: a failed action = the `Couldn't {verb} {thing}` toast. */
    private fun run(verb: String, thing: String, block: suspend () -> Unit) {
        scope.launch {
            try {
                block()
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                toast(FeedbackStates.errorTitle(verb, thing))
            }
        }
    }

    private suspend fun toast(text: String) = withContext(Dispatchers.Main) {
        Toast.makeText(context, text, Toast.LENGTH_LONG).show()
    }

    companion object {
        /** The capability intents' actions (res/xml/shortcuts.xml). */
        const val ACTION_CREATE_ISSUE = "com.exponential.app.action.CREATE_ISSUE"
        const val ACTION_ASK_AGENT = "com.exponential.app.action.ASK_AGENT"
        const val EXTRA_TITLE = "title"
        const val EXTRA_AGENT = "agent"
        const val EXTRA_TEXT = "text"
    }
}
