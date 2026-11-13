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
}
