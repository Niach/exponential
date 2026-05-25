package com.exponential.app.data.electric

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.WorkspaceEntity
import com.exponential.app.data.db.WorkspaceInviteEntity
import com.exponential.app.data.db.WorkspaceMemberEntity
import io.ktor.client.HttpClient
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json

/// Multi-account sync orchestrator. Maintains one set of 10 shape jobs per
/// signed-in account; each pipeline writes to that account's per-account Room
/// instance (`exponential-<accountId>-v2.db`). Sign-out on one account cancels
/// just that pipeline; other accounts keep syncing.
@Singleton
class SyncManager @Inject constructor(
    private val auth: AuthRepository,
    private val databaseHolder: DatabaseHolder,
    private val client: HttpClient,
    private val json: Json,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val lock = Any()
    private val pipelines = mutableMapOf<String, List<Job>>()

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
    }

    /// Sign out a specific account: cancel its pipeline. The Room cache stays
    /// so the user can resume offline browsing if they sign back in. Full
    /// deletion happens via `DatabaseHolder.deleteFiles(accountId)` from
    /// Settings.
    suspend fun signOut(accountId: String) {
        cancelPipeline(accountId)
    }

    /// **Transitional**: signs out whichever account is currently the
    /// most-recently-used one. Existing UI callers (Settings "Sign out",
    /// HomeScreen avatar menu) still go through this. Replaced once the auth
    /// UI is reworked per-server in Phase C.
    suspend fun signOut() {
        auth.activeAccountId.value?.let { signOut(it) }
    }

    // MARK: - Reconciliation

    private fun reconcile(signedIn: Set<String>) {
        synchronized(lock) {
            val running = pipelines.keys.toSet()

            // Cancel pipelines for accounts no longer signed in.
            for (accountId in running - signedIn) {
                pipelines.remove(accountId)?.forEach { it.cancel() }
                android.util.Log.i("SyncManager", "Cancelled shape pipeline for $accountId")
            }

            // Launch pipelines for newly signed-in accounts.
            for (accountId in signedIn - running) {
                val db = databaseHolder.database(forAccountId = accountId)
                pipelines[accountId] = launchPipeline(accountId, db)
                android.util.Log.i("SyncManager", "Launched shape pipeline (10 shapes) for $accountId")
            }
        }
    }

    private suspend fun cancelPipeline(accountId: String) {
        val jobs = synchronized(lock) { pipelines.remove(accountId) ?: emptyList() }
        jobs.forEach { it.cancel() }
    }

    // MARK: - Per-account shape launch

    private fun launchPipeline(accountId: String, db: ExponentialDatabase): List<Job> {
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
        val workspaceDao = db.workspaceDao()
        val projectDao = db.projectDao()
        val issueDao = db.issueDao()
        val labelDao = db.labelDao()
        val issueLabelDao = db.issueLabelDao()
        val userDao = db.userDao()
        val workspaceMemberDao = db.workspaceMemberDao()
        val workspaceInviteDao = db.workspaceInviteDao()
        val commentDao = db.commentDao()
        val attachmentDao = db.attachmentDao()

        return listOf(
            launchShape(
                shape = "workspaces", path = "/api/shapes/workspaces",
                serializer = WorkspaceEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { workspaceDao.upsert(it) },
                onUpdate = { workspaceDao.upsert(it) },
                onDelete = { workspaceDao.deleteById(it.id) },
                onRefetch = { workspaceDao.clear() },
            ),
            launchShape(
                shape = "projects", path = "/api/shapes/projects",
                serializer = ProjectEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { projectDao.upsert(it) },
                onUpdate = { projectDao.upsert(it) },
                onDelete = { projectDao.deleteById(it.id) },
                onRefetch = { projectDao.clear() },
            ),
            launchShape(
                shape = "issues", path = "/api/shapes/issues",
                serializer = IssueEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { issueDao.upsert(it) },
                onUpdate = { issueDao.upsert(it) },
                onDelete = { issueDao.deleteById(it.id) },
                onRefetch = { issueDao.clear() },
            ),
            launchShape(
                shape = "labels", path = "/api/shapes/labels",
                serializer = LabelEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { labelDao.upsert(it) },
                onUpdate = { labelDao.upsert(it) },
                onDelete = { labelDao.deleteById(it.id) },
                onRefetch = { labelDao.clear() },
            ),
            launchShape(
                shape = "issue_labels", path = "/api/shapes/issue-labels",
                serializer = IssueLabelEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { issueLabelDao.upsert(it) },
                onUpdate = { issueLabelDao.upsert(it) },
                onDelete = { issueLabelDao.delete(it.issueId, it.labelId) },
                onRefetch = { issueLabelDao.clear() },
            ),
            launchShape(
                shape = "users", path = "/api/shapes/users",
                serializer = UserEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { userDao.upsert(it) },
                onUpdate = { userDao.upsert(it) },
                onDelete = { userDao.deleteById(it.id) },
                onRefetch = { userDao.clear() },
            ),
            launchShape(
                shape = "workspace_members", path = "/api/shapes/workspace-members",
                serializer = WorkspaceMemberEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { workspaceMemberDao.upsert(it) },
                onUpdate = { workspaceMemberDao.upsert(it) },
                onDelete = { workspaceMemberDao.deleteById(it.id) },
                onRefetch = { workspaceMemberDao.clear() },
            ),
            launchShape(
                shape = "workspace_invites", path = "/api/shapes/workspace-invites",
                serializer = WorkspaceInviteEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { workspaceInviteDao.upsert(it) },
                onUpdate = { workspaceInviteDao.upsert(it) },
                onDelete = { workspaceInviteDao.deleteById(it.id) },
                onRefetch = { workspaceInviteDao.clear() },
            ),
            launchShape(
                shape = "comments", path = "/api/shapes/comments",
                serializer = CommentEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { commentDao.upsert(it) },
                onUpdate = { commentDao.upsert(it) },
                onDelete = { commentDao.deleteById(it.id) },
                onRefetch = { commentDao.clear() },
            ),
            launchShape(
                shape = "attachments", path = "/api/shapes/attachments",
                serializer = AttachmentEntity.serializer(),
                offsetDao = offsetDao, baseUrl = baseUrl, token = token,
                onInsert = { attachmentDao.upsert(it) },
                onUpdate = { attachmentDao.upsert(it) },
                onDelete = { attachmentDao.deleteById(it.id) },
                onRefetch = { attachmentDao.clear() },
            ),
        )
    }

    private fun <T : Any> launchShape(
        shape: String,
        path: String,
        serializer: KSerializer<T>,
        offsetDao: com.exponential.app.data.db.ElectricOffsetDao,
        baseUrl: () -> String?,
        token: () -> String?,
        onInsert: suspend (T) -> Unit,
        onUpdate: suspend (T) -> Unit,
        onDelete: suspend (T) -> Unit,
        onRefetch: suspend () -> Unit,
    ): Job {
        val shapeClient = ShapeClient(
            client = client,
            baseUrlProvider = baseUrl,
            tokenProvider = token,
            shapeName = shape,
            urlPath = path,
            valueSerializer = serializer,
            offsetDao = offsetDao,
            json = json,
            onMessages = { messages ->
                for (message in messages) {
                    when (message) {
                        is ShapeMessage.Insert -> onInsert(message.value)
                        is ShapeMessage.Update -> onUpdate(message.value)
                        is ShapeMessage.Delete -> message.value?.let { onDelete(it) }
                        ShapeMessage.MustRefetch -> onRefetch()
                        ShapeMessage.UpToDate -> Unit
                    }
                }
            },
        )
        return scope.launch { shapeClient.run() }
    }
}
