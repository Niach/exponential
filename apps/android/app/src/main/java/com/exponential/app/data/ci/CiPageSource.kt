package com.exponential.app.data.ci

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.CiJobEntity
import com.exponential.app.data.db.CiRunEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.MergeTrainEntity
import com.exponential.app.data.db.MergeTrainEventEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.RepositoryEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.UserEntity
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
 * (`runner.enabled`), the viewer and (#EXP-2214) the repositories shape (names,
 * default branches), the merge queue's runs and its history. The clock is
 * the screen's. [badge] = nav-badges.json destination `ci` ([NavBadges.ciBadge]),
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
        val users: List<UserEntity>,
        val repos: List<RepositoryEntity> = emptyList(),
        val trains: List<MergeTrainEntity> = emptyList(),
        val events: List<MergeTrainEventEntity> = emptyList(),
    )

    /** Every input but `now`. */
    val input: Flow<CiPage.Input> = combine(
        dbFlow.scopedQuery(emptyList<CiRunEntity>()) { it.ciRunDao().observeAll() },
        dbFlow.scopedQuery(emptyList<CiJobEntity>()) { it.ciJobDao().observeAll() },
        dbFlow.scopedQuery(emptyList<PullRequestEntity>()) { it.pullRequestDao().observeAll() },
        dbFlow.scopedQuery(emptyList<SessionEntity>()) { it.sessionDao().observeAll() },
        dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
    ) { runs, jobs, prs, sessions, devices -> Rows(runs, jobs, prs, sessions, devices, emptyList()) }
        .combine(dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeAll() }) { rows, users -> rows.copy(users = users) }
        .combine(dbFlow.scopedQuery(emptyList<RepositoryEntity>()) { it.repositoryDao().observeAll() }) { rows, repos -> rows.copy(repos = repos) }
        .combine(dbFlow.scopedQuery(emptyList<MergeTrainEntity>()) { it.mergeTrainDao().observeAll() }) { rows, trains -> rows.copy(trains = trains) }
        .combine(dbFlow.scopedQuery(emptyList<MergeTrainEventEntity>()) { it.mergeTrainEventDao().observeAll() }) { rows, events -> rows.copy(events = events) }
        .combine(auth.userId) { rows, viewer -> toInput(rows, viewer.orEmpty()) }
        .flowOn(Dispatchers.Default)

    /** nav-badges.json destination `ci`. */
    val badge: Flow<NavBadges.Badge> = input.map { NavBadges.ciBadge(it) }.distinctUntilChanged()

    private fun toInput(rows: Rows, viewerId: String): CiPage.Input = CiPage.Input(
        runs = rows.runs.map { it.toPageRun() },
        jobs = rows.jobs.map { it.toPageJob() },
        repos = rows.repos.map { it.toPageRepo() },
        pullRequests = rows.prs.map { it.toPagePr() },
        trains = rows.trains.map { it.toPageTrain() },
        trainEvents = rows.events.map { it.toPageTrainEvent() },
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
        // Only the people a run names (its author) and (#EXP-2214) the
        // starters of the PRs' chats (the merge queue's members).
        users = rows.prs.mapNotNull { it.rootSessionId }.toSet().let { roots ->
            rows.runs.mapNotNull { it.createdByUserId } + rows.sessions.filter { it.id in roots }.map { it.userId }
        }.toSet().let { ids ->
            rows.users.filter { it.id in ids }.map { CiPage.User(it.id, it.name?.takeIf(String::isNotBlank) ?: it.email, it.image) }
        },
        viewerId = viewerId,
        utcOffsetMinutes = java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 60_000,
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
    title = title,
    mergeTrainId = mergeTrainId,
)

/** #EXP-2214: the synced repositories row (the default branch = its override, else GitHub's). */
internal fun RepositoryEntity.toPageRepo(): CiPage.Repo =
    CiPage.Repo(id, fullName, defaultBranchOverride?.takeIf { it.isNotBlank() } ?: defaultBranch.takeIf { it.isNotBlank() })

internal fun PullRequestEntity.toPagePr(): CiPage.Pr = CiPage.Pr(
    id = id,
    number = number ?: 0,
    title = title.orEmpty(),
    repositoryId = repositoryId,
    rootSessionId = rootSessionId,
    base = base,
    state = state,
    mergeState = mergeState,
    mergeWait = mergeWait,
    mergeHold = mergeHold,
    mergeNow = mergeNow,
    mergeAttempts = mergeAttempts ?: 0,
    mergePosition = mergePosition,
    mergeTrainId = mergeTrainId,
)

private val lenient = Json { ignoreUnknownKeys = true }

/** A jsonb array of strings (JSON text), lenient: anything else = empty. */
private fun strings(text: String?): List<String> =
    runCatching { lenient.parseToJsonElement(text ?: "[]") as? JsonArray }.getOrNull()
        ?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty()

private fun nested(text: String?): JsonArray? = runCatching { lenient.parseToJsonElement(text ?: "[]") as? JsonArray }.getOrNull()

internal fun MergeTrainEntity.toPageTrain(): CiPage.Train = CiPage.Train(
    id = id,
    repositoryId = repositoryId,
    base = base,
    baseSha = baseSha,
    units = nested(units).orEmpty().map { unit -> (unit as? JsonArray).orEmpty().mapNotNull { (it as? JsonPrimitive)?.contentOrNull } },
    state = state,
    attempt = attempt ?: 0,
    bisectOf = bisectOf,
    pending = nested(pending).orEmpty().map { half ->
        (half as? JsonArray).orEmpty().map { unit -> (unit as? JsonArray).orEmpty().mapNotNull { (it as? JsonPrimitive)?.contentOrNull } }
    },
    untested = untested,
    testedSha = testedSha,
    createdAt = createdAt,
    finishedAt = finishedAt,
)

internal fun MergeTrainEventEntity.toPageTrainEvent(): CiPage.TrainEvent = CiPage.TrainEvent(
    id = id,
    repositoryId = repositoryId,
    pullRequestId = pullRequestId,
    mergeTrainId = mergeTrainId,
    kind = kind,
    base = base,
    attempt = attempt ?: 0,
    reason = reason,
    checks = strings(checks),
    ciJobId = ciJobId,
    jobUrl = jobUrl,
    files = strings(files),
    sha = sha,
    byUserId = byUserId,
    createdAt = createdAt,
)

internal fun CiJobEntity.toPageJob(): CiPage.Job = CiPage.Job(
    id = id,
    runId = runId,
    check = check,
    status = status,
    conclusion = conclusion,
    runsOn = strings(runsOn),
    runnerDeviceId = runnerDeviceId,
    provider = provider,
)
