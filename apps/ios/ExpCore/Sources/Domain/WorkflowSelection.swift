import Foundation

/// EXP-1086: what the workflow page's chip strip has selected — `All` (nil)
/// or ONE node. Pure, so the page's selection rules are testable without a
/// view: tap selects, tapping the selected chip again goes back to All, and
/// stepping walks the strip in DAG order (waves, then lanes) with All at
/// position 0.
public struct WorkflowSelection: Sendable, Equatable {
    /// The selected node id; nil = All.
    public private(set) var nodeId: String?

    public init(nodeId: String? = nil) {
        self.nodeId = nodeId
    }

    public var isAll: Bool { nodeId == nil }

    /// The strip's chip ids in DAG order — the order `nodeStrip` draws them.
    public static func order(_ strip: [StripWave]) -> [String] {
        strip.flatMap { $0.nodes.map(\.id) }
    }

    /// Select one node, or All with nil.
    public mutating func select(_ id: String?) {
        nodeId = id
    }

    /// A chip tap: the selected chip goes back to All, any other is selected.
    public mutating func toggle(_ id: String) {
        nodeId = nodeId == id ? nil : id
    }

    /// The position in the strip, All = 0, the first node = 1. A selected node
    /// that is not in `order` (it left the workflow) reads as All.
    public func position(in order: [String]) -> Int {
        guard let nodeId, let index = order.firstIndex(of: nodeId) else { return 0 }
        return index + 1
    }

    /// Step `delta` chips along the strip, clamped to All … the last node.
    public mutating func step(_ delta: Int, in order: [String]) {
        let target = min(max(position(in: order) + delta, 0), order.count)
        nodeId = target == 0 ? nil : order[target - 1]
    }

    /// A node that left the workflow (dismissed, re-planned away) falls back
    /// to All.
    public mutating func reconcile(with order: [String]) {
        if let nodeId, !order.contains(nodeId) { self.nodeId = nil }
    }
}
