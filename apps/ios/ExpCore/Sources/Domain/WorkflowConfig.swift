import Foundation

// EXP-981: the jsonb payloads a workflow and its nodes carry — `launch` (how
// the run starts), `metrics` (the plan's shape, written by the server layout)
// and a node's review. Electric delivers them as JSON values which the
// entities store as stringified JSON (the `automations.trigger` pattern), so
// every one of these parses TOLERANTLY: unknown keys are ignored and a missing
// one falls back to its default rather than dropping the row.
// Mirrors packages/db-schema domain.ts (`WorkflowLaunch`,
// `WorkflowMetricsJson`).

/// `workflows.launch` — the launch options the engine starts every node with.
/// Every field is optional: absent = the runner device's own defaults.
public struct WorkflowLaunch: Sendable, Equatable {
    public var agent: String?
    public var model: String?
    /// EXP-1029: the STRONG model — contract, integration and `risk: high`
    /// nodes, and every agent review. The phase pins and `reviewModel` below
    /// are deprecated (they fold into this one; EXP-1014 removes them). The
    /// phone CARRIES it, never edits it.
    public var strongModel: String?
    /// DEPRECATED (EXP-1029) — the EXP-1002 per-PHASE pins. They fold into
    /// `strongModel` (`normalized` below) and nothing writes them any more;
    /// they are still DECODED, because old rows carry them and the fold is the
    /// only thing that still reads what they say.
    public var contractModel: String?
    public var integrationModel: String?
    public var riskModel: String?
    /// DEPRECATED (EXP-1029) — claude's subagents run on `model`.
    public var subagentModel: String?
    /// DEPRECATED (EXP-1029) — a workflow run picks no effort.
    public var effort: String?
    /// An agent profile id on the runner device.
    public var account: String?
    /// How many nodes may run at once (contract `workflowMaxParallelDefault`
    /// when unset).
    public var maxParallel: Int?
    /// DEPRECATED (EXP-1029) — every agent review runs on `strongModel`. Still
    /// decoded: on an old row it is the FIRST candidate the fold takes.
    public var reviewModel: String?

    public init(
        agent: String? = nil,
        model: String? = nil,
        strongModel: String? = nil,
        contractModel: String? = nil,
        integrationModel: String? = nil,
        riskModel: String? = nil,
        subagentModel: String? = nil,
        effort: String? = nil,
        account: String? = nil,
        maxParallel: Int? = nil,
        reviewModel: String? = nil
    ) {
        self.agent = agent
        self.model = model
        self.strongModel = strongModel
        self.contractModel = contractModel
        self.integrationModel = integrationModel
        self.riskModel = riskModel
        self.subagentModel = subagentModel
        self.effort = effort
        self.account = account
        self.maxParallel = maxParallel
        self.reviewModel = reviewModel
    }
}

extension WorkflowLaunch: Codable {
    enum CodingKeys: String, CodingKey {
        case agent, model, strongModel, contractModel, integrationModel, riskModel
        case subagentModel, effort, account, maxParallel, reviewModel
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        agent = try c.decodeIfPresent(String.self, forKey: .agent)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        strongModel = try c.decodeIfPresent(String.self, forKey: .strongModel)
        contractModel = try c.decodeIfPresent(String.self, forKey: .contractModel)
        integrationModel = try c.decodeIfPresent(String.self, forKey: .integrationModel)
        riskModel = try c.decodeIfPresent(String.self, forKey: .riskModel)
        subagentModel = try c.decodeIfPresent(String.self, forKey: .subagentModel)
        effort = try c.decodeIfPresent(String.self, forKey: .effort)
        account = try c.decodeIfPresent(String.self, forKey: .account)
        maxParallel = try? c.decodeWireInt(forKey: .maxParallel)
        reviewModel = try c.decodeIfPresent(String.self, forKey: .reviewModel)
    }

    /// Only the set fields ride the wire — the server replaces the whole
    /// `launch` object, so an omitted key IS "unset". The three EXP-1002 phase
    /// pins are the exception: for THEM an absent key means "keep the stored
    /// value" (the server's carry-forward for clients that predate them), so
    /// they always ride EXPLICITLY — `null` clears, a string sets.
    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(agent, forKey: .agent)
        try c.encodeIfPresent(model, forKey: .model)
        // EXP-1029: carried, so an absent key keeps the stored value.
        try c.encodeIfPresent(strongModel, forKey: .strongModel)
        try c.encode(contractModel, forKey: .contractModel)
        try c.encode(integrationModel, forKey: .integrationModel)
        try c.encode(riskModel, forKey: .riskModel)
        try c.encodeIfPresent(subagentModel, forKey: .subagentModel)
        try c.encodeIfPresent(effort, forKey: .effort)
        try c.encodeIfPresent(account, forKey: .account)
        try c.encodeIfPresent(maxParallel, forKey: .maxParallel)
        try c.encodeIfPresent(reviewModel, forKey: .reviewModel)
    }
}

public extension WorkflowLaunch {
    /// The stored jsonb string as a value; absent/malformed = every default.
    static func parse(_ json: String?) -> WorkflowLaunch {
        decodeJson(json) ?? WorkflowLaunch()
    }
}

// MARK: - The strict launch (EXP-1029)

public extension WorkflowLaunch {
    /// EXP-1029 — what a workflow run actually starts with: ONE agent and TWO
    /// models. `model` is the CHEAP one (leaf nodes, and the subagents inside
    /// every node run), `strongModel` the capable one (contract nodes,
    /// integration nodes, `risk: high` nodes and every agent review).
    ///
    /// The phone CARRIES the stored launch and never edits it; this is only
    /// what it says about a node.
    struct Normalized: Sendable, Equatable {
        public let agent: String
        public let model: String
        public let strongModel: String
        /// An agent profile id on the runner device; nil = its ambient login.
        public let account: String?

        public init(agent: String, model: String, strongModel: String, account: String? = nil) {
            self.agent = agent
            self.model = model
            self.strongModel = strongModel
            self.account = account
        }
    }

    /// The stored launch (any vintage, or garbage) → the strict one. Mirrors
    /// web `normalizeWorkflowLaunch` (`lib/workflow-launch.ts`) and Rust
    /// `coding::workflows::launch`, rule for rule:
    /// - `agent`: `claude` or `codex`; anything else → `claude`.
    /// - `model`: the stored one, else that agent's contract default.
    /// - `strongModel`: the stored one; else the first set of the deprecated
    ///   pins (`reviewModel`, `riskModel`, `contractModel`, `integrationModel`
    ///   — an old row's choice), else that agent's default strong model.
    /// - `subagentModel`, `effort` and `maxParallel` are dropped.
    var normalized: Normalized {
        let agent: String = {
            if let stored = Self.text(self.agent),
               DomainContract.workflowLaunchAgents.contains(stored) {
                return stored
            }
            return Self.claudeAgent
        }()
        let legacyStrong = [reviewModel, riskModel, contractModel, integrationModel]
            .lazy.compactMap(Self.text).first
        return Normalized(
            agent: agent,
            model: Self.text(model) ?? Self.defaultModel(for: agent),
            strongModel: Self.text(strongModel)
                ?? legacyStrong
                ?? Self.defaultStrongModel(for: agent),
            account: Self.text(account)
        )
    }

    /// The cheap model an agent runs on when the row names none.
    static func defaultModel(for agent: String) -> String {
        agent == codexAgent
            ? DomainContract.workflowLaunchCodexModel
            : DomainContract.workflowLaunchClaudeModel
    }

    /// The strong model an agent runs its contract, integration, high-risk and
    /// review work on when the row names none.
    static func defaultStrongModel(for agent: String) -> String {
        agent == codexAgent
            ? DomainContract.workflowLaunchCodexStrongModel
            : DomainContract.workflowLaunchClaudeStrongModel
    }

    internal static let claudeAgent = "claude"
    internal static let codexAgent = "codex"

    /// A stored string field that carries a value: non-blank, else nil.
    private static func text(_ value: String?) -> String? {
        guard let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !trimmed.isEmpty
        else { return nil }
        return trimmed
    }
}

/// `workflows.metrics` — the plan's shape. `cycles` non-empty means the
/// workflow cannot start; `cycleEdges` names the edges inside them
/// (`<fromNodeId>\n<toNodeId>`) so the graph can paint them red.
public struct WorkflowMetrics: Sendable, Equatable {
    public var nodes: Int
    public var edges: Int
    public var depth: Int
    public var width: Int
    public var cycles: [[String]]
    public var cycleEdges: [String]
    /// EXP-984: the run's COUNTERS, which accumulate inside the same jsonb next
    /// to the shape keys — every whole-number key of the object, the shape ones
    /// included. A value that is not a whole number is left out rather than
    /// read as 0 (the web rule's `Number.isFinite` guard).
    public var counters: [String: Int]

    public init(
        nodes: Int = 0,
        edges: Int = 0,
        depth: Int = 0,
        width: Int = 0,
        cycles: [[String]] = [],
        cycleEdges: [String] = [],
        counters: [String: Int] = [:]
    ) {
        self.nodes = nodes
        self.edges = edges
        self.depth = depth
        self.width = width
        self.cycles = cycles
        self.cycleEdges = cycleEdges
        self.counters = counters
    }
}

extension WorkflowMetrics: Decodable {
    enum CodingKeys: String, CodingKey {
        case nodes, edges, depth, width, cycles, cycleEdges
    }

    /// Any key of the metrics object — the counter set is OPEN, so a newer
    /// server's counter still arrives.
    private struct CounterKey: CodingKey {
        let stringValue: String
        var intValue: Int? { nil }
        init?(stringValue: String) { self.stringValue = stringValue }
        init?(intValue _: Int) { nil }
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // SE-0230 flattens the try?-of-optional: a garbled counter degrades to
        // 0 rather than losing the whole metrics object.
        nodes = (try? c.decodeWireInt(forKey: .nodes)) ?? 0
        edges = (try? c.decodeWireInt(forKey: .edges)) ?? 0
        depth = (try? c.decodeWireInt(forKey: .depth)) ?? 0
        width = (try? c.decodeWireInt(forKey: .width)) ?? 0
        cycles = (try? c.decodeIfPresent([[String]].self, forKey: .cycles)) ?? []
        cycleEdges = (try? c.decodeIfPresent([String].self, forKey: .cycleEdges)) ?? []
        var found: [String: Int] = [:]
        if let open = try? decoder.container(keyedBy: CounterKey.self) {
            for key in open.allKeys {
                if let value = try? open.decode(Int.self, forKey: key) {
                    found[key.stringValue] = value
                }
            }
        }
        counters = found
    }
}

public extension WorkflowMetrics {
    static func parse(_ json: String?) -> WorkflowMetrics {
        decodeJson(json) ?? WorkflowMetrics()
    }
}

/// EXP-984 — `workflow_nodes.review.oracle`: an executable check the reviewer
/// RAN (the contract tests on the trunk). An agent's opinion is advisory; a
/// passing oracle is evidence.
public struct WorkflowReviewOracle: Sendable, Equatable {
    public var command: String
    public var passed: Bool

    public init(command: String = "", passed: Bool = false) {
        self.command = command
        self.passed = passed
    }
}

extension WorkflowReviewOracle: Decodable {
    enum CodingKeys: String, CodingKey {
        case command, passed
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        command = (try? c.decodeIfPresent(String.self, forKey: .command)) ?? ""
        passed = c.decodeWireBool(forKey: .passed, default: false)
    }
}

/// EXP-984 — `workflow_nodes.review`: the latest verdict an agent reviewer
/// submitted. An approval counts as THE approval only with a passing oracle and
/// never on a contract node; otherwise it is advisory and a person still
/// approves.
public struct WorkflowNodeReview: Sendable, Equatable {
    /// contract `wfReviewVerdict`.
    public var verdict: String
    public var findings: String
    public var oracle: WorkflowReviewOracle?
    /// The model that reviewed (a `risk: high` node: never its author's).
    public var model: String?
    public var round: Int
    public var at: String

    public init(
        verdict: String,
        findings: String = "",
        oracle: WorkflowReviewOracle? = nil,
        model: String? = nil,
        round: Int = 0,
        at: String = ""
    ) {
        self.verdict = verdict
        self.findings = findings
        self.oracle = oracle
        self.model = model
        self.round = round
        self.at = at
    }
}

extension WorkflowNodeReview: Decodable {
    enum CodingKeys: String, CodingKey {
        case verdict, findings, oracle, model, round, at
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        verdict = (try? c.decodeIfPresent(String.self, forKey: .verdict)) ?? ""
        findings = (try? c.decodeIfPresent(String.self, forKey: .findings)) ?? ""
        oracle = try? c.decodeIfPresent(WorkflowReviewOracle.self, forKey: .oracle)
        model = try? c.decodeIfPresent(String.self, forKey: .model)
        round = (try? c.decodeWireInt(forKey: .round)) ?? 0
        at = (try? c.decodeIfPresent(String.self, forKey: .at)) ?? ""
    }
}

public extension WorkflowNodeReview {
    /// Nil until a reviewer submitted one — and on a payload that names no
    /// verdict, which is the one thing every surface reads.
    static func parse(_ json: String?) -> WorkflowNodeReview? {
        guard let value: WorkflowNodeReview = decodeJson(json), !value.verdict.isEmpty
        else { return nil }
        return value
    }
}

/// One stored jsonb string, decoded permissively — nil on absent or malformed
/// JSON, never a throw: a jsonb column a newer server wrote must degrade to a
/// default, not break the surface that reads it.
private func decodeJson<T: Decodable>(_ json: String?) -> T? {
    guard let json, let data = json.data(using: .utf8) else { return nil }
    return try? JSONDecoder().decode(T.self, from: data)
}
