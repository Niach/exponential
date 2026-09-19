import Foundation

// EXP-981: the jsonb payloads a workflow and its nodes carry — `launch` (how
// the run starts), `metrics` (the plan's shape, written by the server layout)
// and a node's `budget`. Electric delivers them as JSON values which the
// entities store as stringified JSON (the `automations.trigger` pattern), so
// every one of these parses TOLERANTLY: unknown keys are ignored and a missing
// one falls back to its default rather than dropping the row.
// Mirrors packages/db-schema domain.ts (`WorkflowLaunch`,
// `WorkflowMetricsJson`, `WorkflowNodeBudget`).

/// `workflows.launch` — the launch options the engine starts every node with.
/// Every field is optional: absent = the runner device's own defaults.
public struct WorkflowLaunch: Sendable, Equatable {
    public var agent: String?
    public var model: String?
    /// Claude only: the model its subagents run on (EXP-981).
    public var subagentModel: String?
    public var effort: String?
    /// An agent profile id on the runner device.
    public var account: String?
    /// How many nodes may run at once (contract `workflowMaxParallelDefault`
    /// when unset).
    public var maxParallel: Int?

    public init(
        agent: String? = nil,
        model: String? = nil,
        subagentModel: String? = nil,
        effort: String? = nil,
        account: String? = nil,
        maxParallel: Int? = nil
    ) {
        self.agent = agent
        self.model = model
        self.subagentModel = subagentModel
        self.effort = effort
        self.account = account
        self.maxParallel = maxParallel
    }
}

extension WorkflowLaunch: Codable {
    enum CodingKeys: String, CodingKey {
        case agent, model, subagentModel, effort, account, maxParallel
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        agent = try c.decodeIfPresent(String.self, forKey: .agent)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        subagentModel = try c.decodeIfPresent(String.self, forKey: .subagentModel)
        effort = try c.decodeIfPresent(String.self, forKey: .effort)
        account = try c.decodeIfPresent(String.self, forKey: .account)
        maxParallel = try? c.decodeWireInt(forKey: .maxParallel)
    }

    /// Only the set fields ride the wire — the server replaces the whole
    /// `launch` object, so an omitted key IS "unset".
    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(agent, forKey: .agent)
        try c.encodeIfPresent(model, forKey: .model)
        try c.encodeIfPresent(subagentModel, forKey: .subagentModel)
        try c.encodeIfPresent(effort, forKey: .effort)
        try c.encodeIfPresent(account, forKey: .account)
        try c.encodeIfPresent(maxParallel, forKey: .maxParallel)
    }
}

public extension WorkflowLaunch {
    /// The stored jsonb string as a value; absent/malformed = every default.
    static func parse(_ json: String?) -> WorkflowLaunch {
        decodeJson(json) ?? WorkflowLaunch()
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

    public init(
        nodes: Int = 0,
        edges: Int = 0,
        depth: Int = 0,
        width: Int = 0,
        cycles: [[String]] = [],
        cycleEdges: [String] = []
    ) {
        self.nodes = nodes
        self.edges = edges
        self.depth = depth
        self.width = width
        self.cycles = cycles
        self.cycleEdges = cycleEdges
    }
}

extension WorkflowMetrics: Decodable {
    enum CodingKeys: String, CodingKey {
        case nodes, edges, depth, width, cycles, cycleEdges
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
    }
}

public extension WorkflowMetrics {
    static func parse(_ json: String?) -> WorkflowMetrics {
        decodeJson(json) ?? WorkflowMetrics()
    }
}

/// `workflow_nodes.budget` — crossing either bound pauses the node and
/// notifies. Absent entirely on a node nobody bounded.
public struct WorkflowNodeBudget: Sendable, Equatable {
    public var tokens: Int?
    public var minutes: Int?

    public init(tokens: Int? = nil, minutes: Int? = nil) {
        self.tokens = tokens
        self.minutes = minutes
    }
}

extension WorkflowNodeBudget: Codable {
    enum CodingKeys: String, CodingKey {
        case tokens, minutes
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        tokens = try? c.decodeWireInt(forKey: .tokens)
        minutes = try? c.decodeWireInt(forKey: .minutes)
    }
}

public extension WorkflowNodeBudget {
    /// Nil when the stored jsonb is absent or names neither bound.
    static func parse(_ json: String?) -> WorkflowNodeBudget? {
        guard let value: WorkflowNodeBudget = decodeJson(json) else { return nil }
        return value.tokens == nil && value.minutes == nil ? nil : value
    }
}

/// One stored jsonb string, decoded permissively — nil on absent or malformed
/// JSON, never a throw: a jsonb column a newer server wrote must degrade to a
/// default, not break the surface that reads it.
private func decodeJson<T: Decodable>(_ json: String?) -> T? {
    guard let json, let data = json.data(using: .utf8) else { return nil }
    return try? JSONDecoder().decode(T.self, from: data)
}
