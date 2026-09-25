import Foundation

/// EXP-1084/1086: the workflow page's PICKER model — pure, mirrored ×4 (web
/// `lib/workflow-selection.ts`, desktop `ui::workflow_view::Selection`,
/// Android `WorkflowSelection`) and locked by the contract fixture
/// `workflow-view.json` `selection`. The chip strip is the picker: `All`
/// first (position 0), then every node in DAG order. A selection is a set of
/// node ids; an EMPTY selection IS `All`.
///
/// A click picks exactly that node (a click on the already picked chip KEEPS
/// it); a toggle adds or removes one node (the last one out = All); extend
/// picks the DAG-order range from the anchor (the last plain or toggling
/// click) to the node; a step moves ONE position from the cursor (the last
/// clicked or stepped node, else the last picked one) through All + the
/// nodes, clamped at both ends; pruning drops nodes that left the workflow.
public struct WorkflowSelection: Sendable, Equatable {
    /// The picked nodes, in DAG order. Empty = `All`.
    public private(set) var ids: [String]
    /// Where an extend range starts: the last plain or toggling click.
    public private(set) var anchor: String?
    /// The node a step moves from: the last one clicked or stepped to.
    public private(set) var cursor: String?

    public init(ids: [String] = [], anchor: String? = nil, cursor: String? = nil) {
        self.ids = ids
        self.anchor = anchor
        self.cursor = cursor
    }

    /// One picked node (a click or a step), else nil (All, or several).
    public init(nodeId: String?) {
        if let nodeId { self.init(ids: [nodeId], anchor: nodeId, cursor: nodeId) } else { self.init() }
    }

    public static let all = WorkflowSelection()

    public var isAll: Bool { ids.isEmpty }

    /// The ONE node the page shows: the cursor when picked, else the last
    /// picked one; nil = All.
    public var nodeId: String? {
        if ids.isEmpty { return nil }
        if let cursor, ids.contains(cursor) { return cursor }
        return ids.last
    }

    /// The strip's chip ids in DAG order — the order `nodeStrip` draws them.
    public static func order(_ strip: [StripWave]) -> [String] {
        strip.flatMap { $0.nodes.map(\.id) }
    }

    /// A plain click: exactly that node; nil (the All chip) or an unknown id
    /// = All. A click on the already picked chip keeps it.
    public mutating func click(_ id: String?, in order: [String]) {
        guard let id, order.contains(id) else { self = .all; return }
        self = WorkflowSelection(ids: [id], anchor: id, cursor: id)
    }

    /// Add or remove one node, kept in DAG order; the last one out = All.
    public mutating func toggle(_ id: String, in order: [String]) {
        guard order.contains(id) else { self = .all; return }
        var picked = Set(ids)
        if picked.remove(id) == nil { picked.insert(id) }
        let next = order.filter { picked.contains($0) }
        self = next.isEmpty ? .all : WorkflowSelection(ids: next, anchor: id, cursor: id)
    }

    /// The DAG-order range from the anchor (else this node) to `id`.
    public mutating func extend(to id: String, in order: [String]) {
        guard let to = order.firstIndex(of: id) else { self = .all; return }
        let start = anchor.flatMap { order.contains($0) ? $0 : nil } ?? id
        let from = order.firstIndex(of: start) ?? to
        let range = Array(order[min(from, to)...max(from, to)])
        self = WorkflowSelection(ids: range, anchor: start, cursor: id)
    }

    /// The position in the strip, All = 0, the first node = 1: the cursor
    /// (the last clicked or stepped node, else the last picked one).
    public func position(in order: [String]) -> Int {
        guard let nodeId, let index = order.firstIndex(of: nodeId) else { return 0 }
        return index + 1
    }

    /// Step `delta` positions from the cursor through All + the nodes,
    /// clamped, landing on exactly one node (or All).
    public mutating func step(_ delta: Int, in order: [String]) {
        let target = min(max(position(in: order) + delta, 0), order.count)
        if target == 0 { self = .all; return }
        let id = order[target - 1]
        self = WorkflowSelection(ids: [id], anchor: id, cursor: id)
    }

    /// Drops nodes that left the workflow (none left = All); the anchor and
    /// cursor stay when their node stays, else fall to the first picked one.
    public mutating func prune(with order: [String]) {
        let kept = order.filter { ids.contains($0) }
        if kept.count == ids.count { return }
        guard let first = kept.first else { self = .all; return }
        self = WorkflowSelection(
            ids: kept,
            anchor: anchor.flatMap { kept.contains($0) ? $0 : nil } ?? first,
            cursor: cursor.flatMap { kept.contains($0) ? $0 : nil } ?? first
        )
    }
}
