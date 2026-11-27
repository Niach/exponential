import Foundation
import GRDB

// MARK: - CiRun / CiJob (#EXP-1617 / #EXP-1804)

// eCode's own CI: the synced `ci_runs` + `ci_jobs` rows (shapes `ci-runs` /
// `ci-jobs`, member-only by team), mirroring the shape allowlists
// `CI_RUN_COLUMNS` / `CI_JOB_COLUMNS`. jsonb columns are stored as their JSON
// text; logs never sync (tRPC `ci.log`). Every non-PK field decodes
// permissively: a required field absent on the wire drops the row forever.

public struct CiRunEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "ci_runs"

    public let id: String
    public let teamId: String
    public let repositoryId: String
    public let number: Int
    public let name: String
    public let sha: String
    public let ref: String?
    /// push | pull_request | tag | merge_train | manual.
    public let trigger: String
    public let pullRequestId: String?
    /// pending | success | failure.
    public let state: String
    /// The pipeline file did not plan (its message).
    public let error: String?
    public let rerunOf: String?
    public let createdByUserId: String?
    public let startedAt: String?
    public let completedAt: String?
    public let createdAt: String
    public let updatedAt: String
    /// #EXP-2172: `exponential` (ours) | `github` (a GitHub Actions run).
    public let provider: String
    /// GitHub's run id, its page, the attempt shown, the installation's
    /// Actions permission (`read` | `write`; nil = unknown).
    public let externalId: String?
    public let externalUrl: String?
    public let externalAttempt: Int?
    public let externalAccess: String?
    /// #EXP-2172 redesign: what the run is about (nil = unknown).
    public let title: String?

    public init(
        id: String, teamId: String, repositoryId: String = "", number: Int, name: String = "", sha: String = "",
        ref: String? = nil, trigger: String = "push", pullRequestId: String? = nil, state: String = "pending",
        error: String? = nil, rerunOf: String? = nil, createdByUserId: String? = nil, startedAt: String? = nil,
        completedAt: String? = nil, createdAt: String, updatedAt: String, provider: String = "exponential",
        externalId: String? = nil, externalUrl: String? = nil, externalAttempt: Int? = nil, externalAccess: String? = nil,
        title: String? = nil
    ) {
        self.id = id
        self.title = title
        self.teamId = teamId
        self.repositoryId = repositoryId
        self.number = number
        self.provider = provider
        self.externalId = externalId
        self.externalUrl = externalUrl
        self.externalAttempt = externalAttempt
        self.externalAccess = externalAccess
        self.name = name
        self.sha = sha
        self.ref = ref
        self.trigger = trigger
        self.pullRequestId = pullRequestId
        self.state = state
        self.error = error
        self.rerunOf = rerunOf
        self.createdByUserId = createdByUserId
        self.startedAt = startedAt
        self.completedAt = completedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, number, name, sha, ref, trigger, state, error, provider, title
        case teamId = "team_id"
        case externalId = "external_id"
        case externalUrl = "external_url"
        case externalAttempt = "external_attempt"
        case externalAccess = "external_access"
        case repositoryId = "repository_id"
        case pullRequestId = "pull_request_id"
        case rerunOf = "rerun_of"
        case createdByUserId = "created_by_user_id"
        case startedAt = "started_at"
        case completedAt = "completed_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    /// The run page's pure input.
    public var run: CiRun.Run {
        CiRun.Run(
            id: id, number: number, name: name, state: state, error: error, sha: sha, startedAt: startedAt,
            completedAt: completedAt, ref: ref, provider: provider, externalUrl: externalUrl, externalAccess: externalAccess,
            title: title, createdAt: createdAt.isEmpty ? nil : createdAt
        )
    }
}

extension CiRunEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        repositoryId = try c.decodeIfPresent(String.self, forKey: .repositoryId) ?? ""
        number = (try? c.decodeWireInt(forKey: .number)) ?? 0
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        sha = try c.decodeIfPresent(String.self, forKey: .sha) ?? ""
        ref = try c.decodeIfPresent(String.self, forKey: .ref)
        trigger = try c.decodeIfPresent(String.self, forKey: .trigger) ?? "push"
        pullRequestId = try c.decodeIfPresent(String.self, forKey: .pullRequestId)
        state = try c.decodeIfPresent(String.self, forKey: .state) ?? "pending"
        error = try c.decodeIfPresent(String.self, forKey: .error)
        rerunOf = try c.decodeIfPresent(String.self, forKey: .rerunOf)
        createdByUserId = try c.decodeIfPresent(String.self, forKey: .createdByUserId)
        startedAt = try c.decodeIfPresent(String.self, forKey: .startedAt)
        completedAt = try c.decodeIfPresent(String.self, forKey: .completedAt)
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt) ?? ""
        updatedAt = try c.decodeIfPresent(String.self, forKey: .updatedAt) ?? ""
        provider = try c.decodeIfPresent(String.self, forKey: .provider) ?? "exponential"
        externalId = try c.decodeIfPresent(String.self, forKey: .externalId)
        externalUrl = try c.decodeIfPresent(String.self, forKey: .externalUrl)
        externalAttempt = try? c.decodeWireInt(forKey: .externalAttempt)
        externalAccess = try c.decodeIfPresent(String.self, forKey: .externalAccess)
        title = try c.decodeIfPresent(String.self, forKey: .title)
    }
}

public struct CiJobEntity: FetchableRecord, PersistableRecord, Identifiable, Equatable, Sendable {
    public static let databaseTableName = "ci_jobs"

    public let id: String
    public let teamId: String
    public let runId: String
    public let key: String
    public let check: String
    /// The planner order on the run page.
    public let position: Int
    /// jsonb string[] / string[] / object, stored as JSON text.
    public let runsOn: String?
    public let needs: String?
    public let matrix: String?
    public let timeoutMin: Int
    /// contract prCheckStatus.
    public let status: String
    /// contract prCheckConclusion; nil while running.
    public let conclusion: String?
    /// A planned skip (`fork`, `paths`, `needs`) or `runner_lost` / `reused`.
    public let reason: String?
    /// The devices ROW id that ran it.
    public let runnerDeviceId: String?
    /// jsonb `[{name, status, conclusion, startedAt, completedAt}]` (epoch ms).
    public let steps: String?
    public let logKey: String?
    public let logBytes: Int
    public let queuedAt: String?
    public let startedAt: String?
    public let completedAt: String?
    public let createdAt: String
    public let updatedAt: String
    /// #EXP-2172: `exponential` | `github`; GitHub's job id and page.
    public let provider: String
    public let externalId: String?
    public let externalUrl: String?

    public init(
        id: String, teamId: String, runId: String, key: String = "", check: String, position: Int = 0,
        runsOn: String? = nil, needs: String? = nil, matrix: String? = nil, timeoutMin: Int = 60,
        status: String = "queued", conclusion: String? = nil, reason: String? = nil, runnerDeviceId: String? = nil,
        steps: String? = nil, logKey: String? = nil, logBytes: Int = 0, queuedAt: String? = nil,
        startedAt: String? = nil, completedAt: String? = nil, createdAt: String, updatedAt: String,
        provider: String = "exponential", externalId: String? = nil, externalUrl: String? = nil
    ) {
        self.id = id
        self.teamId = teamId
        self.runId = runId
        self.provider = provider
        self.externalId = externalId
        self.externalUrl = externalUrl
        self.key = key
        self.check = check
        self.position = position
        self.runsOn = runsOn
        self.needs = needs
        self.matrix = matrix
        self.timeoutMin = timeoutMin
        self.status = status
        self.conclusion = conclusion
        self.reason = reason
        self.runnerDeviceId = runnerDeviceId
        self.steps = steps
        self.logKey = logKey
        self.logBytes = logBytes
        self.queuedAt = queuedAt
        self.startedAt = startedAt
        self.completedAt = completedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, key, check, position, needs, matrix, status, conclusion, reason, steps, provider
        case teamId = "team_id"
        case externalId = "external_id"
        case externalUrl = "external_url"
        case runId = "run_id"
        case runsOn = "runs_on"
        case timeoutMin = "timeout_min"
        case runnerDeviceId = "runner_device_id"
        case logKey = "log_key"
        case logBytes = "log_bytes"
        case queuedAt = "queued_at"
        case startedAt = "started_at"
        case completedAt = "completed_at"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    /// The synced steps; anything unreadable = none.
    public var parsedSteps: [CiRun.Step] {
        guard let steps, let data = steps.data(using: .utf8),
              let array = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]]
        else { return [] }
        return array.compactMap { o in
            guard let name = o["name"] as? String else { return nil }
            return CiRun.Step(
                name: name,
                status: (o["status"] as? String) ?? DomainContract.prCheckStatusQueued,
                conclusion: o["conclusion"] as? String,
                startedAt: (o["startedAt"] as? NSNumber)?.doubleValue,
                completedAt: (o["completedAt"] as? NSNumber)?.doubleValue
            )
        }
    }

    /// The run page's pure input; `runnerLabel` = the runner device's label.
    public func job(runnerLabel: String?) -> CiRun.Job {
        CiRun.Job(
            id: id, check: check, position: position, status: status, conclusion: conclusion, reason: reason,
            runnerLabel: runnerLabel, startedAt: startedAt, completedAt: completedAt, steps: parsedSteps
        )
    }
}

extension CiJobEntity: Codable {
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        runId = try c.decode(String.self, forKey: .runId)
        key = try c.decodeIfPresent(String.self, forKey: .key) ?? ""
        check = try c.decodeIfPresent(String.self, forKey: .check) ?? ""
        position = (try? c.decodeWireInt(forKey: .position)) ?? 0
        runsOn = c.decodeWireJsonString(forKey: .runsOn)
        needs = c.decodeWireJsonString(forKey: .needs)
        matrix = c.decodeWireJsonString(forKey: .matrix)
        timeoutMin = (try? c.decodeWireInt(forKey: .timeoutMin)) ?? 60
        status = try c.decodeIfPresent(String.self, forKey: .status) ?? DomainContract.prCheckStatusQueued
        conclusion = try c.decodeIfPresent(String.self, forKey: .conclusion)
        reason = try c.decodeIfPresent(String.self, forKey: .reason)
        runnerDeviceId = try c.decodeIfPresent(String.self, forKey: .runnerDeviceId)
        steps = c.decodeWireJsonString(forKey: .steps)
        logKey = try c.decodeIfPresent(String.self, forKey: .logKey)
        logBytes = (try? c.decodeWireInt(forKey: .logBytes)) ?? 0
        queuedAt = try c.decodeIfPresent(String.self, forKey: .queuedAt)
        startedAt = try c.decodeIfPresent(String.self, forKey: .startedAt)
        completedAt = try c.decodeIfPresent(String.self, forKey: .completedAt)
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt) ?? ""
        updatedAt = try c.decodeIfPresent(String.self, forKey: .updatedAt) ?? ""
        provider = try c.decodeIfPresent(String.self, forKey: .provider) ?? "exponential"
        externalId = try c.decodeIfPresent(String.self, forKey: .externalId)
        externalUrl = try c.decodeIfPresent(String.self, forKey: .externalUrl)
    }
}

// MARK: - The CI page's input (#EXP-2172, ci-page.json DATA)

extension CiPage {
    /// The page's synced input, every member team at once: runs, jobs, pull
    /// requests, root sessions (their starters), the runs' authors (users),
    /// the runner devices (runner enabled, never a member space). `repos`,
    /// `now` and `utcOffsetMinutes` are the caller's
    /// (repository names come from `repositories.list` per team until the
    /// repositories shape syncs).
    public static func fetchRows(_ db: Database, viewerId: String) throws -> Input {
        let runs = try CiRunEntity.fetchAll(db)
        let jobs = try CiJobEntity.fetchAll(db)
        let prs = try PullRequestEntity.fetchAll(db)
        let roots = Set(prs.compactMap(\.rootSessionId))
        let sessions = roots.isEmpty ? [] : try SessionEntity.filter(keys: Array(roots)).fetchAll(db)
        let devices = try DeviceEntity.fetchAll(db)
        let authors = Set(runs.compactMap(\.createdByUserId))
        let users = authors.isEmpty ? [] : try UserEntity.filter(keys: Array(authors)).fetchAll(db)
        return Input(
            runs: runs.map(\.pageRun),
            jobs: jobs.map(\.pageJob),
            repos: [],
            pullRequests: prs.map { pr in
                Pr(
                    id: pr.id, number: pr.number, title: pr.title ?? "", repositoryId: pr.repositoryId,
                    rootSessionId: pr.rootSessionId, base: pr.base, mergeState: pr.mergeState.isEmpty ? nil : pr.mergeState,
                    mergePosition: pr.mergePosition, mergeTrainId: pr.mergeTrainId
                )
            },
            sessions: sessions.filter { $0.rootId == $0.id }.map { Session(id: $0.id, userId: $0.userId) },
            runners: devices.compactMap { device in
                let settings = RunnerSettings.Settings(json: device.runner)
                guard settings.enabled, device.spaceHostId == nil else { return nil }
                return Runner(
                    id: device.id, label: device.label.isEmpty ? device.deviceId : device.label, platform: device.platform,
                    labels: settings.labels, slots: settings.slots, lastSeenAt: device.lastSeenAt
                )
            },
            users: users.map { User(id: $0.id, name: ($0.name ?? "").isEmpty ? $0.email : $0.name ?? "", image: $0.image) },
            viewerId: viewerId,
            now: ""
        )
    }
}

extension CiRunEntity {
    public var pageRun: CiPage.Run {
        CiPage.Run(
            id: id, repositoryId: repositoryId, number: number, name: name, sha: sha, ref: ref, trigger: trigger,
            state: state, pullRequestId: pullRequestId, createdByUserId: createdByUserId, provider: provider,
            startedAt: startedAt, completedAt: completedAt, createdAt: createdAt, title: title
        )
    }
}

extension CiJobEntity {
    /// `runs_on` (JSON text) as labels; anything unreadable = none.
    public var runsOnLabels: [String] {
        guard let runsOn, let data = runsOn.data(using: .utf8) else { return [] }
        return ((try? JSONSerialization.jsonObject(with: data)) as? [String]) ?? []
    }

    public var pageJob: CiPage.Job {
        CiPage.Job(runId: runId, status: status, conclusion: conclusion, runsOn: runsOnLabels, runnerDeviceId: runnerDeviceId, provider: provider)
    }
}

extension CiPage {
    /// nav-badges.json destination `ci` live (the iPad sidebar's CI row, the
    /// phones' More entry): `ciBadge` over the synced rows.
    public static func badgeObservation(viewerId: String) -> ValueObservation<ValueReducers.Fetch<NavBadges.Value>> {
        ValueObservation.tracking { db in ciBadge(try fetchRows(db, viewerId: viewerId)) }
    }
}
