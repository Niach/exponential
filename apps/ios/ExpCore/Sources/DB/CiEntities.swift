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

    public init(
        id: String, teamId: String, repositoryId: String = "", number: Int, name: String = "", sha: String = "",
        ref: String? = nil, trigger: String = "push", pullRequestId: String? = nil, state: String = "pending",
        error: String? = nil, rerunOf: String? = nil, createdByUserId: String? = nil, startedAt: String? = nil,
        completedAt: String? = nil, createdAt: String, updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.repositoryId = repositoryId
        self.number = number
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
        case id, number, name, sha, ref, trigger, state, error
        case teamId = "team_id"
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
        CiRun.Run(id: id, number: number, name: name, state: state, error: error, sha: sha, startedAt: startedAt, completedAt: completedAt)
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

    public init(
        id: String, teamId: String, runId: String, key: String = "", check: String, position: Int = 0,
        runsOn: String? = nil, needs: String? = nil, matrix: String? = nil, timeoutMin: Int = 60,
        status: String = "queued", conclusion: String? = nil, reason: String? = nil, runnerDeviceId: String? = nil,
        steps: String? = nil, logKey: String? = nil, logBytes: Int = 0, queuedAt: String? = nil,
        startedAt: String? = nil, completedAt: String? = nil, createdAt: String, updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.runId = runId
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
        case id, key, check, position, needs, matrix, status, conclusion, reason, steps
        case teamId = "team_id"
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
    }
}
