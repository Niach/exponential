package com.exponential.app.data.electric

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.ElectricOffsetDao
import com.exponential.app.data.db.IssueDao
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueLabelDao
import com.exponential.app.data.db.IssueLabelEntity
import com.exponential.app.data.db.LabelDao
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectDao
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.WorkspaceDao
import com.exponential.app.data.db.WorkspaceEntity
import io.ktor.client.HttpClient
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json

@Singleton
class SyncManager @Inject constructor(
    private val auth: AuthRepository,
    private val client: HttpClient,
    private val json: Json,
    private val offsetDao: ElectricOffsetDao,
    private val workspaceDao: WorkspaceDao,
    private val projectDao: ProjectDao,
    private val issueDao: IssueDao,
    private val labelDao: LabelDao,
    private val issueLabelDao: IssueLabelDao,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var shapeJobs: List<Job> = emptyList()

    fun start() {
        scope.launch {
            combine(auth.instanceUrl, auth.token) { url, token -> url to token }
                .distinctUntilChanged()
                .collect { (url, token) ->
                    cancelShapes()
                    if (url != null && token != null) launchShapes()
                }
        }
    }

    suspend fun signOut() {
        cancelShapes()
        offsetDao.clear()
        workspaceDao.clear()
        projectDao.clear()
        issueDao.clear()
        labelDao.clear()
        issueLabelDao.clear()
    }

    private fun cancelShapes() {
        shapeJobs.forEach { it.cancel() }
        shapeJobs = emptyList()
    }

    private fun launchShapes() {
        shapeJobs = listOf(
            launchShape(
                shape = "workspaces",
                path = "/api/shapes/workspaces",
                serializer = WorkspaceEntity.serializer(),
                onInsert = { workspaceDao.upsert(it) },
                onUpdate = { workspaceDao.upsert(it) },
                onDelete = { workspaceDao.deleteById(it.id) },
                onRefetch = { workspaceDao.clear() },
            ),
            launchShape(
                shape = "projects",
                path = "/api/shapes/projects",
                serializer = ProjectEntity.serializer(),
                onInsert = { projectDao.upsert(it) },
                onUpdate = { projectDao.upsert(it) },
                onDelete = { projectDao.deleteById(it.id) },
                onRefetch = { projectDao.clear() },
            ),
            launchShape(
                shape = "issues",
                path = "/api/shapes/issues",
                serializer = IssueEntity.serializer(),
                onInsert = { issueDao.upsert(it) },
                onUpdate = { issueDao.upsert(it) },
                onDelete = { issueDao.deleteById(it.id) },
                onRefetch = { issueDao.clear() },
            ),
            launchShape(
                shape = "labels",
                path = "/api/shapes/labels",
                serializer = LabelEntity.serializer(),
                onInsert = { labelDao.upsert(it) },
                onUpdate = { labelDao.upsert(it) },
                onDelete = { labelDao.deleteById(it.id) },
                onRefetch = { labelDao.clear() },
            ),
            launchShape(
                shape = "issue_labels",
                path = "/api/shapes/issue-labels",
                serializer = IssueLabelEntity.serializer(),
                onInsert = { issueLabelDao.upsert(it) },
                onUpdate = { issueLabelDao.upsert(it) },
                onDelete = { issueLabelDao.delete(it.issueId, it.labelId) },
                onRefetch = { issueLabelDao.clear() },
            ),
        )
    }

    private fun <T : Any> launchShape(
        shape: String,
        path: String,
        serializer: kotlinx.serialization.KSerializer<T>,
        onInsert: suspend (T) -> Unit,
        onUpdate: suspend (T) -> Unit,
        onDelete: suspend (T) -> Unit,
        onRefetch: suspend () -> Unit,
    ): Job {
        val client = ShapeClient(
            client = client,
            baseUrlProvider = { auth.instanceUrl.value },
            tokenProvider = { auth.token.value },
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
        return scope.launch { client.run() }
    }
}
