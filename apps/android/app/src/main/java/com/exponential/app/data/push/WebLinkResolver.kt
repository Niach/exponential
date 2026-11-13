package com.exponential.app.data.push

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.domain.AppLink
import com.exponential.app.domain.ChatRowAction
import com.exponential.app.domain.Chats
import com.exponential.app.domain.InboxRoute
import com.exponential.app.domain.PlanPage
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.SettingsNav
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.domain.appLinkTeam
import com.exponential.app.domain.isAppsListPath
import java.net.URI
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first

/** #EXP-1674: the native screen an in-app link opens. */
sealed interface LinkDestination {
    data class Issue(val id: String) : LinkDestination

    /** A synced session: a root opens its chat (Guide on a `/sessions/` link's
     *  `?view=guide`), a child its session view. */
    data class Conversation(val id: String, val isRoot: Boolean, val sessionPath: Boolean) : LinkDestination
    data class Plan(val sessionId: String, val messageId: String) : LinkDestination
    data class Board(val id: String) : LinkDestination
    data object Chats : LinkDestination
    data class AgentDm(val agentId: String) : LinkDestination
    data object AgentHome : LinkDestination
    data object Inbox : LinkDestination
    data object Devices : LinkDestination
    data class Settings(val section: String) : LinkDestination
    data object TeamHome : LinkDestination
    data object AppsList : LinkDestination
    data class AppRun(val appId: String) : LinkDestination
    data class AppScreen(val appId: String, val screen: String, val params: Map<String, String>) : LinkDestination
}

/**
 * #EXP-1674: the facts `linkOpenOutcome` decides on. [destination] on a miss =
 * a screen with its own not-found state (the plan page, an agent's chat);
 * [accountId] + [teamId] = where the link's team lives for the viewer.
 */
data class LinkResolution(
    val member: Boolean,
    val found: Boolean,
    val webOnly: Boolean,
    val accountId: String? = null,
    val teamId: String? = null,
    val destination: LinkDestination? = null,
)

/**
 * Resolves an in-app link (EXP-92, #EXP-1674) under the signed-in accounts whose
 * instanceUrl host matches it. The row may simply not have synced yet (cold
 * launch / brand-new issue), so a miss polls the DB briefly while sync runs
 * ([SyncManager] has no awaitable hook) before it counts as missing.
 */
@Singleton
class WebLinkResolver @Inject constructor(
    private val auth: AuthRepository,
    private val dbHolder: DatabaseHolder,
) {
    /**
     * Teams and team-scoped rows are member-only shapes, so "not a member" =
     * no such team among the host's synced teams. It waits only while no team
     * has synced at all. A link no team governs (a path off the team tree) or
     * on an instance with no signed-in account here can only open on the web.
     */
    suspend fun resolveLink(link: AppLink, host: String): LinkResolution {
        val browserOnly = LinkResolution(member = true, found = true, webOnly = true)
        val slug = appLinkTeam(link) ?: return browserOnly
        val accounts = accountsOn(host)
        if (accounts.isEmpty()) return browserOnly

        var memberAccountId: String? = null
        var memberTeamId: String? = null
        for (attempt in 0 until POLL_ATTEMPTS) {
            var teamsSynced = false
            for (account in accounts) {
                val db = runCatching { dbHolder.database(account.id) }.getOrNull() ?: continue
                val teams = runCatching { db.teamDao().observeAll().first() }.getOrNull().orEmpty()
                if (teams.isNotEmpty()) teamsSynced = true
                val team = teams.firstOrNull { it.slug == slug } ?: continue
                if (memberTeamId == null) {
                    memberAccountId = account.id
                    memberTeamId = team.id
                }
                if (isWebOnly(link)) {
                    return LinkResolution(true, found = true, webOnly = true, accountId = account.id, teamId = team.id)
                }
                val destination = runCatching { linkDestination(db, team.id, link, account.userId) }.getOrNull()
                if (destination != null) {
                    return LinkResolution(true, found = true, webOnly = false, accountId = account.id, teamId = team.id, destination = destination)
                }
            }
            if (memberTeamId == null && teamsSynced) break
            if (attempt < POLL_ATTEMPTS - 1) delay(POLL_INTERVAL_MS)
        }
        if (memberTeamId == null) return LinkResolution(member = false, found = false, webOnly = false)
        val notFoundScreen = when (link) {
            is AppLink.Plan -> LinkDestination.Plan(link.sessionId, link.messageId)
            is AppLink.Agent -> LinkDestination.AgentDm(link.agentId)
            else -> null
        }
        return LinkResolution(true, found = false, webOnly = false, accountId = memberAccountId, teamId = memberTeamId, destination = notFoundScreen)
    }

    /** A page this client has no screen for: any `app` path but the apps list,
     *  a settings section with no phone page. */
    private fun isWebOnly(link: AppLink): Boolean = when (link) {
        is AppLink.App -> !isAppsListPath(link.path)
        is AppLink.Settings -> link.section.isNotEmpty() && link.section !in SettingsNav.PHONE_ROWS
        else -> false
    }

    private suspend fun isOwner(db: com.exponential.app.data.db.ExponentialDatabase, teamId: String, userId: String?): Boolean =
        db.teamMemberDao().observeByTeam(teamId).first().any {
            it.userId == userId && it.role == com.exponential.app.domain.DomainContract.teamRoleOwner
        }

    /** The screen of [link] on one account's DB; null = the row is not there. */
    private suspend fun linkDestination(
        db: com.exponential.app.data.db.ExponentialDatabase,
        teamId: String,
        link: AppLink,
        userId: String?,
    ): LinkDestination? = when (link) {
        is AppLink.Issue -> db.issueDao().findIdByTeamRef(link.teamSlug, link.identifier)?.let { LinkDestination.Issue(it) }
        is AppLink.IssueRef -> db.issueDao().findIdByTeamRef(link.teamSlug, link.identifier)?.let { LinkDestination.Issue(it) }
        is AppLink.Board -> db.boardDao().observeBySlug(teamId, link.boardSlug).first()?.let { LinkDestination.Board(it.id) }
        is AppLink.Session -> conversation(db, link.teamSlug, link.sessionId, sessionPath = true)
        // #EXP-1662: a plan is a tab of its session's root.
        is AppLink.Plan -> db.sessionDao().findIdByTeamRef(link.teamSlug, link.sessionId)?.let { id ->
            val root = db.sessionDao().getById(id)?.rootId?.takeIf { it.isNotEmpty() } ?: id
            LinkDestination.Plan(root, link.messageId)
        }
        is AppLink.Chats -> LinkDestination.Chats
        is AppLink.Agent -> db.agentDefinitionDao().observeById(link.agentId).first()
            ?.takeIf { it.teamId == teamId }
            ?.let { LinkDestination.AgentDm(it.id) }
        is AppLink.AgentHome -> LinkDestination.AgentHome
        is AppLink.Inbox -> LinkDestination.Inbox
        is AppLink.Devices -> LinkDestination.Devices
        is AppLink.Settings -> when {
            link.section.isEmpty() -> LinkDestination.TeamHome
            // `settings-nav.json` `visible`: a row this viewer does not see reads as missing.
            link.section !in SettingsNav.visiblePhoneRows(isOwner(db, teamId, userId)) -> null
            else -> LinkDestination.Settings(link.section)
        }
        is AppLink.AppHome -> appIdBySlug(db, teamId, link.appSlug)?.let { LinkDestination.AppRun(it) }
        is AppLink.AppScreen -> appIdBySlug(db, teamId, link.appSlug)?.let { LinkDestination.AppScreen(it, link.screen, link.params) }
        // Every other `app` path is web-only ([isWebOnly]).
        is AppLink.App -> LinkDestination.AppsList
        is AppLink.External, AppLink.Ignore -> null
    }

    private suspend fun conversation(
        db: com.exponential.app.data.db.ExponentialDatabase,
        teamSlug: String,
        sessionId: String,
        sessionPath: Boolean,
    ): LinkDestination? {
        val id = db.sessionDao().findIdByTeamRef(teamSlug, sessionId) ?: return null
        return LinkDestination.Conversation(id, db.sessionDao().getById(id)?.isRoot ?: true, sessionPath)
    }

    private suspend fun appIdBySlug(
        db: com.exponential.app.data.db.ExponentialDatabase,
        teamId: String,
        slug: String,
    ): String? = db.appDao().observeByTeam(teamId).first().firstOrNull { it.slug == slug }?.id

    /**
     * #EXP-1456 (fixtures/inbox-route.json `sessionCases`, the S-3 rule):
     * where the synced session [sessionId] opens for the signed-in viewer —
     * the agent's channel for my own agent conversation root, the chat for
     * a chat's root or child, else the session itself. A child resolves
     * its agent + owner through its root. Null when the row is not synced
     * (yet): the caller opens the session route's not-found state.
     */
    suspend fun sessionTarget(sessionId: String): InboxRoute.SessionTarget? = activeDb(null) { db ->
        val session = db.sessionDao().getById(sessionId) ?: return@activeDb null
        val rootId = session.rootId.takeIf { it.isNotEmpty() } ?: session.id
        val root = if (rootId == session.id) session else db.sessionDao().getById(rootId)
        InboxRoute.sessionTarget(
            id = session.id,
            rootId = rootId,
            agentId = root?.agentId ?: session.agentId,
            userId = root?.userId ?: session.userId,
            viewerId = auth.userId.value,
        )
    }

    /** #EXP-1662 (plan-page.json WAYS IN): the chat whose plan tab holds
     *  the plans of [sessionId] (its root); null = not synced (yet). */
    suspend fun planChat(sessionId: String): String? = activeDb(null) { db ->
        db.sessionDao().getById(sessionId)?.let { session -> session.rootId.takeIf { it.isNotEmpty() } ?: session.id }
    }

    /** #EXP-1493 (plan-page.json WAYS IN): the session's own waiting plan
     *  message ([PlanPage.planPageFor]) an inbox row / push opens its chat's
     *  plan tab on; null = none (the session's own route). */
    suspend fun planPageTarget(sessionId: String): String? = activeDb(null) { db ->
        val messages = db.messageDao().observeBySession(sessionId).first().mapNotNull { row ->
            val message = SessionTree.message(row, sessionId)
            if (message.card?.get("kind")?.toString() != "\"plan\"") return@mapNotNull null
            PlanPage.Message(
                id = message.id,
                sessionId = sessionId,
                authorSessionId = message.authorSessionId,
                createdAt = WireTimestamps.parseEpochMs(message.createdAt) ?: 0L,
                card = message.card,
            )
        }
        PlanPage.planPageFor(sessionId, messages)
    }

    /** waiting-cards.json DEEP LINKS: the session's own NEWEST waiting card
     *  ([ChatRowAction.sessionWaitingCard] over ALL its synced messages, never
     *  the chat's newest-200 window, + its digest's `waits`: the messages
     *  sync on demand, the digest always) an inbox row lands on; null = none. */
    suspend fun waitingCard(sessionId: String): String? = activeDb(null) { db ->
        val messages = db.messageDao().observeBySession(sessionId).first().map { SessionTree.message(it, sessionId) }
        val digest = db.sessionDao().getById(sessionId)?.digest
        ChatRowAction.sessionWaitingCard(sessionId, messages + Chats.digestMessages(sessionId, Chats.parseDigest(digest)))
    }

    /**
     * Session 58cb32df (chats.json): a notification / deep link that names
     * another team SWITCHES first, then opens the target. The team of the
     * synced issue / session [id] on the ACTIVE account (an issue's = its
     * board's); null when the row is not synced (yet).
     */
    suspend fun teamOfIssue(id: String, accountId: String? = null): String? = activeDb(accountId) { db ->
        val issue = db.issueDao().observeById(id).first() ?: return@activeDb null
        db.boardDao().observeAll().first().firstOrNull { it.id == issue.boardId }?.teamId
    }

    suspend fun teamOfSession(id: String, accountId: String? = null): String? = activeDb(accountId) { db ->
        db.sessionDao().getById(id)?.teamId?.takeIf { it.isNotEmpty() }
    }

    private suspend fun <T> activeDb(
        account: String?,
        block: suspend (com.exponential.app.data.db.ExponentialDatabase) -> T?,
    ): T? {
        val accountId = account ?: auth.activeAccountId.value ?: return null
        return runCatching { block(dbHolder.database(accountId)) }.getOrNull()
    }

    /** Signed-in accounts on the link's instance — active account first, then
     *  most recently used (several accounts can share a host). */
    private fun accountsOn(host: String): List<com.exponential.app.data.auth.ServerAccount> {
        val activeId = auth.activeAccountId.value
        return auth.accounts.value
            .filter { it.token != null && instanceHost(it.instanceUrl) == host }
            .sortedWith(
                compareByDescending<com.exponential.app.data.auth.ServerAccount> { it.id == activeId }
                    .thenByDescending { it.lastUsedAt }
            )
    }

    private fun instanceHost(instanceUrl: String): String? =
        runCatching { URI(instanceUrl.trim()).host }.getOrNull()

    private companion object {
        // ~10s total: enough for the post-launch sync to land a fresh issue
        // without holding an unresolvable link hostage forever.
        const val POLL_ATTEMPTS = 20
        const val POLL_INTERVAL_MS = 500L
    }
}
