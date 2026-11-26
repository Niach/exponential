package com.exponential.app.data.push

import android.content.Context
import android.util.Log
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.domain.Chats
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.chats.needsYouRead
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * nav-badges.json THE OS APP ICON: the launcher's number = every signed-in
 * account's Needs you total ([needsYouRead], the Chats page's read), carried by
 * the app's summary notification ([PushNotifier]); never a notification of its own.
 */
@Singleton
class AppIconBadge @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
    private val nodes: EnetNodeManager,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /** Re-reads the count into the notifications that stand. */
    fun refresh() {
        scope.launch {
            if (PushNotifier.hasSummary(context)) PushNotifier.setNumber(context, count())
        }
    }

    suspend fun count(): Int = NavBadges.appIconBadge(
        auth.accounts.value.filter { it.token != null }.map { account ->
            try {
                needsYou(account)
            } catch (err: CancellationException) {
                throw err
            } catch (err: Exception) {
                Log.w(TAG, "needs you not read: ${err.javaClass.simpleName}")
                0
            }
        },
    )

    private suspend fun needsYou(account: ServerAccount): Int {
        val db = holder.database(forAccountId = account.id)
        suspend fun <T> read(flow: Flow<T>): T = flow.first()
        val sessions = read(db.sessionDao().observeAll())
        val owned = account.userId?.let { read(db.teamMemberDao().observeByUser(it)) }.orEmpty()
            .filter { it.role == DomainContract.teamRoleOwner }.mapTo(HashSet()) { it.teamId }
        return needsYouRead(
            sessions = sessions.map(SessionTree::session),
            links = emptyList(),
            issues = emptyList(),
            messages = sessions.flatMap { Chats.digestMessages(it.id, Chats.parseDigest(it.digest)) },
            definitions = read(db.agentDefinitionDao().observeAll()).map(SessionTree::agentDefinition),
            pullRequests = read(db.pullRequestDao().observeAll()),
            teams = read(db.teamDao().observeAll()),
            owned = owned,
            devices = read(db.deviceDao().observeAll()),
            me = account.userId,
            now = System.currentTimeMillis(),
        ).counts(nodes.trust.snapshot(account.id).installs).total
    }

    private companion object {
        const val TAG = "AppIconBadge"
    }
}
