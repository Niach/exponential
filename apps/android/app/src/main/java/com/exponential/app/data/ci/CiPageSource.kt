package com.exponential.app.data.ci

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CiJobEntity
import com.exponential.app.data.db.CiRunEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.CiPage
import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.RunnerSettings
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

/**
 * #EXP-2172 (ci-page.json DATA): the CI page's synced inputs across every
 * member team: `ci_runs` + `ci_jobs` (ours and GitHub Actions), the pull
 * requests, the sessions (a root's starter), the runner devices
 * (`runner.enabled`) and the viewer. Repository names and the clock are the
 * screen's. [badge] = nav-badges.json destination `ci` ([NavBadges.ciBadge]),
 * the value the phones' More menu shows on its CI entry.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@Singleton
class CiPageSource @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
) {
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private data class Rows(
        val runs: List<CiRunEntity>,
        val jobs: List<CiJobEntity>,
        val prs: List<PullRequestEntity>,
        val sessions: List<SessionEntity>,
        val devices: List<DeviceEntity>,
    )

    /** Every input but `repos` and `now`. */
    val input: Flow<CiPage.Input> = combine(
        dbFlow.scopedQuery(emptyList<CiRunEntity>()) { it.ciRunDao().observeAll() },
        dbFlow.scopedQuery(emptyList<CiJobEntity>()) { it.ciJobDao().observeAll() },
        dbFlow.scopedQuery(emptyList<PullRequestEntity>()) { it.pullRequestDao().observeAll() },
        dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeAll() },
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
    ) { runs, jobs, prs, sessions, devices -> Rows(runs, jobs, prs, sessions, devices) }
        .combine(auth.userId) { rows, viewer -> toInput(rows, viewer.orEmpty()) }
        .flowOn(Dispatchers.Default)

    /** nav-badges.json destination `ci`. */
    val badge: Flow<NavBadges.Badge> = input.map { NavBadges.ciBadge(it) }.distinctUntilChanged()

    private fun toInput(rows: Rows, viewerId: String): CiPage.Input = CiPage.Input(
        runs = rows.runs.map { it.toPageRun() },
        jobs = rows.jobs.map { it.toPageJob() },
        pullRequests = rows.prs.map { pr ->
            CiPage.Pr(
                id = pr.id,
                number = pr.number ?: 0,
                title = pr.title.orEmpty(),
                repositoryId = pr.repositoryId,
                rootSessionId = pr.rootSessionId,
                base = pr.base,
                mergeState = pr.mergeState,
                mergePosition = pr.mergePosition,
                mergeTrainId = pr.mergeTrainId,
            )
        },
        // Only the starters of roots a PR names matter (`mine`).
        sessions = rows.prs.mapNotNull { it.rootSessionId }.toSet().let { roots ->
            rows.sessions.filter { it.id in roots }.map { CiPage.Session(it.id, it.userId) }
        },
        runners = rows.devices.mapNotNull { device ->
            val settings = RunnerSettings.parse(device.runner)?.takeIf { it.enabled } ?: return@mapNotNull null
            CiPage.Runner(
                id = device.id,
                label = device.label.ifBlank { device.deviceId },
                platform = device.platform,
                labels = settings.labels,
                slots = settings.slots,
                lastSeenAt = device.lastSeenAt,
            )
        },
        viewerId = viewerId,
    )
}

internal fun CiRunEntity.toPageRun(): CiPage.Run = CiPage.Run(
    id = id,
    repositoryId = repositoryId,
    number = number ?: 0,
    name = name,
    sha = sha,
    ref = ref,
    trigger = trigger,
    state = state,
    pullRequestId = pullRequestId,
    createdByUserId = createdByUserId,
    provider = provider,
    startedAt = startedAt,
    completedAt = completedAt,
    createdAt = createdAt,
)

private val lenient = Json { ignoreUnknownKeys = true }

internal fun CiJobEntity.toPageJob(): CiPage.Job = CiPage.Job(
    runId = runId,
    status = status,
    conclusion = conclusion,
    runsOn = runCatching { lenient.parseToJsonElement(runsOn ?: "[]") as? JsonArray }.getOrNull()
        ?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty(),
    runnerDeviceId = runnerDeviceId,
    provider = provider,
)
