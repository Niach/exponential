import Foundation

// #EXP-1617 / #EXP-1804: mirrors the people half of apps/web/src/lib/trpc/ci.ts
// (the run page and a device's CI runner section). Runs and jobs come from
// sync (the ci_runs / ci_jobs shapes); only the LOG is read on demand.

/// `ci.log`'s answer: the text from the asked offset, where to read next, and
/// whether the job's log is whole (no further reads).
public struct CiLogChunk: Decodable, Sendable, Equatable {
    public let text: String
    public let nextOffset: Int
    public let complete: Bool

    public init(text: String, nextOffset: Int, complete: Bool) {
        self.text = text
        self.nextOffset = nextOffset
        self.complete = complete
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
        nextOffset = try c.decodeIfPresent(Int.self, forKey: .nextOffset) ?? 0
        complete = try c.decodeIfPresent(Bool.self, forKey: .complete) ?? false
    }

    enum CodingKeys: String, CodingKey { case text, nextOffset, complete }
}

private struct CiLogInput: Encodable {
    let jobId: String
    let offset: Int
}

private struct CiRerunInput: Encodable {
    let runId: String
    let failedOnly: Bool
}

private struct CiRunIdInput: Encodable {
    let runId: String
}

private struct CiRerunResult: Decodable {
    let runId: String
}

private struct CiSetRunnerInput: Encodable {
    let deviceId: String
    let enabled: Bool
    let labels: [String]
    let slots: Int
    let timeoutMin: Int
}

/// #EXP-1617 (repo-ci.json): a repository's `ci_config`; absent keys = off / none.
public struct RepoCiConfig: Decodable, Sendable, Equatable {
    public var requiredChecks: [String]
    public var githubStatuses: Bool
    public var actionsRunners: Bool

    public init(requiredChecks: [String] = [], githubStatuses: Bool = false, actionsRunners: Bool = false) {
        self.requiredChecks = requiredChecks
        self.githubStatuses = githubStatuses
        self.actionsRunners = actionsRunners
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        requiredChecks = (try? c.decodeIfPresent([String].self, forKey: .requiredChecks)) ?? []
        githubStatuses = (try? c.decodeIfPresent(Bool.self, forKey: .githubStatuses)) ?? false
        actionsRunners = (try? c.decodeIfPresent(Bool.self, forKey: .actionsRunners)) ?? false
    }

    enum CodingKeys: String, CodingKey { case requiredChecks, githubStatuses, actionsRunners }
}

/// `ci.repoSettings`'s answer: the config + the secret NAMES (values are
/// never read back).
public struct RepoCiSettings: Decodable, Sendable, Equatable {
    public var config: RepoCiConfig
    public var secrets: [RepoCi.Secret]

    public init(config: RepoCiConfig = RepoCiConfig(), secrets: [RepoCi.Secret] = []) {
        self.config = config
        self.secrets = secrets
    }

    private struct SecretJSON: Decodable {
        let name: String
        let updatedAt: String?
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        config = (try? c.decodeIfPresent(RepoCiConfig.self, forKey: .config)) ?? RepoCiConfig()
        secrets = (try c.decodeIfPresent([SecretJSON].self, forKey: .secrets) ?? [])
            .map { RepoCi.Secret(name: $0.name, updatedAt: $0.updatedAt ?? "") }
    }

    enum CodingKeys: String, CodingKey { case config, secrets }
}

private struct CiRepoInput: Encodable {
    let repositoryId: String
}

/// Nil keys drop off the wire: only the given keys change.
private struct CiSetRepoConfigInput: Encodable {
    let repositoryId: String
    let requiredChecks: [String]?
    let githubStatuses: Bool?
    let actionsRunners: Bool?
}

private struct CiSetSecretInput: Encodable {
    let repositoryId: String
    let name: String
    let value: String
}

private struct CiDeleteSecretInput: Encodable {
    let repositoryId: String
    let name: String
}

public final class CiApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// `ci.log{jobId, offset}`: member-only; read from 0, then from `nextOffset`.
    public func log(accountId: String, jobId: String, offset: Int) async throws -> CiLogChunk {
        try await trpc.query(accountId: accountId, path: "ci.log", input: CiLogInput(jobId: jobId, offset: offset))
    }

    /// `ci.rerun{runId, failedOnly: true}`: the new run's id (the page opens it).
    public func rerunFailed(accountId: String, runId: String) async throws -> String {
        let result: CiRerunResult = try await trpc.mutation(
            accountId: accountId, path: "ci.rerun", input: CiRerunInput(runId: runId, failedOnly: true)
        )
        return result.runId
    }

    /// `ci.cancel{runId}`.
    public func cancel(accountId: String, runId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "ci.cancel", input: CiRunIdInput(runId: runId))
    }

    /// `ci.setRunner{deviceId, enabled, labels, slots, timeoutMin}`: the WHOLE
    /// settings object, the device owner only (`deviceId` = the steer id).
    public func setRunner(accountId: String, deviceId: String, settings: RunnerSettings.Settings) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "ci.setRunner",
            input: CiSetRunnerInput(
                deviceId: deviceId, enabled: settings.enabled, labels: settings.labels,
                slots: settings.slots, timeoutMin: settings.timeoutMin
            )
        )
    }

    /// `ci.repoSettings{repositoryId}`: the repo's CI config + secret names (members).
    public func repoSettings(accountId: String, repositoryId: String) async throws -> RepoCiSettings {
        try await trpc.query(accountId: accountId, path: "ci.repoSettings", input: CiRepoInput(repositoryId: repositoryId))
    }

    /// `ci.setRepoConfig`: only the non-nil keys change (the repo's managers).
    public func setRepoConfig(
        accountId: String,
        repositoryId: String,
        requiredChecks: [String]? = nil,
        githubStatuses: Bool? = nil,
        actionsRunners: Bool? = nil
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "ci.setRepoConfig",
            input: CiSetRepoConfigInput(
                repositoryId: repositoryId, requiredChecks: requiredChecks,
                githubStatuses: githubStatuses, actionsRunners: actionsRunners
            )
        )
    }

    /// `ci.setSecret{repositoryId, name, value}` (creates or replaces).
    public func setSecret(accountId: String, repositoryId: String, name: String, value: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "ci.setSecret",
            input: CiSetSecretInput(repositoryId: repositoryId, name: name, value: value)
        )
    }

    /// `ci.deleteSecret{repositoryId, name}`.
    public func deleteSecret(accountId: String, repositoryId: String, name: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "ci.deleteSecret",
            input: CiDeleteSecretInput(repositoryId: repositoryId, name: name)
        )
    }
}
