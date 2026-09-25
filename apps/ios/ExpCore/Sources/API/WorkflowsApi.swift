import Foundation

// Mirrors apps/web/src/lib/trpc/workflows.ts (EXP-981). A workflow is a picked
// set of backlog issues of ONE repository, planned as a DAG: the `blocks`
// relations among them are the edges, a parent with its sub-issues is ONE
// compound node, and the SERVER computes the layout. Rows sync as the 23rd and
// 24th Electric shapes — this router is the write path (any team member: a
// workflow is work, not a team setting). Refusals are human sentences; every
// surface shows them verbatim.

/// One `workflows` row as the router returns it (camelCase, `launch`/`metrics`
/// as objects). The list surfaces read the synced store instead; this is what a
/// write echoes back — `create` names the workflow the detail then opens.
public struct WorkflowDto: Identifiable, Sendable, Equatable {
    public let id: String
    public let teamId: String
    public let repositoryId: String?
    public let name: String
    /// contract `wfStatus`.
    public let status: String
    public let deviceId: String?
    public let launch: WorkflowLaunch
    public let integrationBranch: String
    public let finalPrUrl: String?
    public let finalPrNumber: Int?
    public let finalPrState: String?
    public let decisions: String
    public let metrics: WorkflowMetrics
    public let startedAt: String?
    public let endedAt: String?
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        repositoryId: String? = nil,
        name: String,
        status: String = "draft",
        deviceId: String? = nil,
        launch: WorkflowLaunch = WorkflowLaunch(),
        integrationBranch: String = "",
        finalPrUrl: String? = nil,
        finalPrNumber: Int? = nil,
        finalPrState: String? = nil,
        decisions: String = "",
        metrics: WorkflowMetrics = WorkflowMetrics(),
        startedAt: String? = nil,
        endedAt: String? = nil,
        createdAt: String = "",
        updatedAt: String = ""
    ) {
        self.id = id
        self.teamId = teamId
        self.repositoryId = repositoryId
        self.name = name
        self.status = status
        self.deviceId = deviceId
        self.launch = launch
        self.integrationBranch = integrationBranch
        self.finalPrUrl = finalPrUrl
        self.finalPrNumber = finalPrNumber
        self.finalPrState = finalPrState
        self.decisions = decisions
        self.metrics = metrics
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

extension WorkflowDto: Decodable {
    enum CodingKeys: String, CodingKey {
        case id, teamId, repositoryId, name, status, deviceId, launch
        case integrationBranch, finalPrUrl, finalPrNumber, finalPrState, decisions
        case metrics, startedAt, endedAt, createdAt, updatedAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        repositoryId = try c.decodeIfPresent(String.self, forKey: .repositoryId)
        name = (try? c.decode(String.self, forKey: .name)) ?? ""
        status = (try? c.decodeIfPresent(String.self, forKey: .status)) ?? "draft"
        deviceId = try c.decodeIfPresent(String.self, forKey: .deviceId)
        // The two jsonb columns: objects over tRPC, pre-stringified from
        // fixtures — both go through the tolerant parse.
        launch = WorkflowLaunch.parse(c.decodeWireJsonString(forKey: .launch))
        integrationBranch =
            (try? c.decodeIfPresent(String.self, forKey: .integrationBranch)) ?? ""
        finalPrUrl = try c.decodeIfPresent(String.self, forKey: .finalPrUrl)
        finalPrNumber = try? c.decodeWireInt(forKey: .finalPrNumber)
        finalPrState = try c.decodeIfPresent(String.self, forKey: .finalPrState)
        decisions = (try? c.decodeIfPresent(String.self, forKey: .decisions)) ?? ""
        metrics = WorkflowMetrics.parse(c.decodeWireJsonString(forKey: .metrics))
        startedAt = try c.decodeIfPresent(String.self, forKey: .startedAt)
        endedAt = try c.decodeIfPresent(String.self, forKey: .endedAt)
        createdAt = (try? c.decode(String.self, forKey: .createdAt)) ?? ""
        updatedAt = (try? c.decode(String.self, forKey: .updatedAt)) ?? ""
    }
}

public extension WorkflowDto {
    /// The synced local row as the DTO every surface renders.
    init(entity: WorkflowEntity) {
        self.init(
            id: entity.id,
            teamId: entity.teamId,
            repositoryId: entity.repositoryId,
            name: entity.name,
            status: entity.status,
            deviceId: entity.deviceId,
            launch: entity.parsedLaunch,
            integrationBranch: entity.integrationBranch,
            finalPrUrl: entity.finalPrUrl,
            finalPrNumber: entity.finalPrNumber,
            finalPrState: entity.finalPrState,
            decisions: entity.decisions,
            metrics: entity.parsedMetrics,
            startedAt: entity.startedAt,
            endedAt: entity.endedAt,
            createdAt: entity.createdAt,
            updatedAt: entity.updatedAt
        )
    }
}

/// `workflows.create` / `.update` return `{ workflow, txId }`.
public struct WorkflowResult: Decodable, Sendable {
    public let workflow: WorkflowDto
}

private struct CreateInput: Encodable {
    let teamId: String
    let issueIds: [String]
    let name: String?
}

/// A partial `workflows.update`: an OMITTED field keeps what the row has,
/// while `deviceId` is CLEARABLE — a nested optional, so `.some(nil)` sends an
/// explicit null ("no runner bound").
///
/// EXP-1014/EXP-1033: the phone sends NAME and DEVICE, nothing else. The
/// workflow screen configures no run any more — binding a runner to a draft
/// re-seeds the launch from THAT machine's defaults server-side, and every
/// model is derived from the two the launch carries — so the launch never
/// rides this patch (EXP-1090: nor any start rule — there is only one).
public struct WorkflowPatch: Sendable, Equatable {
    public var name: String?
    public var deviceId: String??

    public init(
        name: String? = nil,
        deviceId: String?? = nil
    ) {
        self.name = name
        self.deviceId = deviceId
    }
}

/// Internal (not private) so the wire-format test can pin the omit-vs-null
/// encoding per field.
struct WorkflowUpdateInput: Encodable {
    let id: String
    let patch: WorkflowPatch

    enum CodingKeys: String, CodingKey {
        case id, name, deviceId
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(patch.name, forKey: .name)
        // The unwrapped inner optional is encoded even when nil — an explicit
        // null is what unbinds the runner device server-side.
        if let deviceId = patch.deviceId {
            try c.encode(deviceId, forKey: .deviceId)
        }
    }
}

private struct SetIssuesInput: Encodable {
    let id: String
    let addIssueIds: [String]
    let removeIssueIds: [String]
}

/// A partial `workflows.updateNode`, addressed by ISSUE (a member's id
/// resolves to its compound node).
public struct WorkflowNodePatch: Sendable, Equatable {
    public var kind: String?
    public var risk: String?
    public var touches: [String]?

    public init(
        kind: String? = nil,
        risk: String? = nil,
        touches: [String]? = nil
    ) {
        self.kind = kind
        self.risk = risk
        self.touches = touches
    }
}

struct WorkflowNodeUpdateInput: Encodable {
    let workflowId: String
    let issueId: String
    let patch: WorkflowNodePatch

    enum CodingKeys: String, CodingKey {
        case workflowId, issueId, kind, risk, touches
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(workflowId, forKey: .workflowId)
        try c.encode(issueId, forKey: .issueId)
        try c.encodeIfPresent(patch.kind, forKey: .kind)
        try c.encodeIfPresent(patch.risk, forKey: .risk)
        try c.encodeIfPresent(patch.touches, forKey: .touches)
    }
}

private struct IdInput: Encodable {
    let id: String
}

/// `workflows.mergeFinalPr` answers `{ merged: true }`.
private struct MergeFinalPrResult: Decodable {
    let merged: Bool
}

private struct ApproveNodeInput: Encodable {
    let nodeId: String
    let approved: Bool
}

private struct ResolveNodeInput: Encodable {
    let nodeId: String
    let action: String
}

private struct AdmitNodeInput: Encodable {
    let nodeId: String
    let admit: Bool
}

public final class WorkflowsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Member-gated `workflows.create` — the picked issues in display order.
    /// The server refuses a started issue, an issue on a repo-less board and a
    /// second repository, each with its own sentence.
    @discardableResult
    public func create(
        accountId: String,
        teamId: String,
        issueIds: [String],
        name: String? = nil
    ) async throws -> WorkflowDto {
        let result: WorkflowResult = try await trpc.mutation(
            accountId: accountId,
            path: "workflows.create",
            input: CreateInput(teamId: teamId, issueIds: issueIds, name: name)
        )
        return result.workflow
    }

    /// Member-gated `workflows.update` — the name is a label, everything else
    /// is the run's configuration and is DRAFT-only server-side. The synced row
    /// echoes the change back, so success needs no local write.
    @discardableResult
    public func update(
        accountId: String,
        id: String,
        patch: WorkflowPatch
    ) async throws -> WorkflowDto {
        let result: WorkflowResult = try await trpc.mutation(
            accountId: accountId,
            path: "workflows.update",
            input: WorkflowUpdateInput(id: id, patch: patch)
        )
        return result.workflow
    }

    /// Member-gated `workflows.setIssues` — add or drop issues of a DRAFT; the
    /// server re-plans and re-lays-out in the same transaction.
    public func setIssues(
        accountId: String,
        id: String,
        addIssueIds: [String] = [],
        removeIssueIds: [String] = []
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "workflows.setIssues",
            input: SetIssuesInput(
                id: id, addIssueIds: addIssueIds, removeIssueIds: removeIssueIds
            )
        )
    }

    /// Member-gated `workflows.updateNode` — what the plan declares per node.
    /// Kind and touches shape the PLAN, so the server takes them on a DRAFT
    /// only; RISK rides at any status (it is the one lever onto the launch's
    /// strong model, and a running workflow still has nodes to raise).
    public func updateNode(
        accountId: String,
        workflowId: String,
        issueId: String,
        patch: WorkflowNodePatch
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "workflows.updateNode",
            input: WorkflowNodeUpdateInput(
                workflowId: workflowId, issueId: issueId, patch: patch
            )
        )
    }

    /// Member-gated `workflows.replan` — re-derive the compound nodes, the
    /// layout and the metrics now.
    public func replan(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "workflows.replan",
            input: IdInput(id: id)
        )
    }

    /// Member-gated `workflows.delete`. A live workflow has to be cancelled
    /// first (the server says so); the rows leave via Electric.
    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "workflows.delete",
            input: IdInput(id: id)
        )
    }

    // MARK: - Running a workflow (EXP-982)

    // The server only flips intent; the deterministic ENGINE on the runner
    // device does the work off the synced rows. Every refusal is a human
    // sentence the caller shows verbatim.

    /// `workflows.start` — the draft's nodes reset and the row goes `running`.
    /// `WorkflowView.startBlocker` names every refusal in advance.
    @discardableResult
    public func start(accountId: String, id: String) async throws -> WorkflowDto {
        let result: WorkflowResult = try await trpc.mutation(
            accountId: accountId,
            path: "workflows.start",
            input: IdInput(id: id)
        )
        return result.workflow
    }

    /// `workflows.pause` — the engine starts and lands nothing new; live runs
    /// finish.
    public func pause(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "workflows.pause", input: IdInput(id: id)
        )
    }

    public func resume(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "workflows.resume", input: IdInput(id: id)
        )
    }

    /// `workflows.cancel` — the engine ends the live runs and deletes ONE
    /// branch. Landed work stays there; nothing reached the default branch.
    public func cancel(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "workflows.cancel", input: IdInput(id: id)
        )
    }

    /// Member-gated `workflows.mergeFinalPr` — squash-merge the workflow's ONE
    /// final pull request (integration branch → the default branch), the one
    /// human review of the whole run. GitHub's acceptance completes the
    /// workflow in the same call (EXP-1032), so the synced row carries the
    /// result back; idempotent for an already merged PR, and a refusal (no
    /// final PR yet, GitHub said no) is the server's own sentence.
    @discardableResult
    public func mergeFinalPr(accountId: String, id: String) async throws -> Bool {
        let result: MergeFinalPrResult = try await trpc.mutation(
            accountId: accountId,
            path: "workflows.mergeFinalPr",
            input: IdInput(id: id)
        )
        return result.merged
    }

    /// The human gate: `workflows.approveNode` clears a node's open PR for the
    /// merge train. `approved: false` takes it back while the node has not
    /// landed.
    public func approveNode(
        accountId: String, nodeId: String, approved: Bool = true
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "workflows.approveNode",
            input: ApproveNodeInput(nodeId: nodeId, approved: approved)
        )
    }

    /// `workflows.resolveNode` — a person unsticks a node: `retry` gives it a
    /// fresh attempt, `skip` takes it out so its dependents go on without it.
    public func resolveNode(
        accountId: String, nodeId: String, action: WorkflowNodeResolution
    ) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "workflows.resolveNode",
            input: ResolveNodeInput(nodeId: nodeId, action: action.rawValue)
        )
    }

    // MARK: - Dynamic graphs (EXP-984)

    /// `workflows.admitNode` — a follow-up filed during the run arrived as a
    /// `proposed` node because it was not plainly additive. Admitting it makes
    /// it part of the run (the server re-plans); dismissing it deletes the row.
    /// Either way the change arrives back over Electric.
    public func admitNode(accountId: String, nodeId: String, admit: Bool) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "workflows.admitNode",
            input: AdmitNodeInput(nodeId: nodeId, admit: admit)
        )
    }
}

/// How a person unsticks a node (`workflows.resolveNode`).
public enum WorkflowNodeResolution: String, Sendable {
    case retry
    case skip
}
