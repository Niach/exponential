import Foundation

// EXP-1031: THE toast, identical ×4 (web packages/ui toast.tsx, desktop
// crates/ui toast.rs, Android ToastStack.kt). The numbers and the per-item
// geometry are the contract fixture's
// (packages/domain-contract/fixtures/toast-stack.json), locked by
// `ToastStackTests`; the SwiftUI host (`ExpUI/Toast.swift`) only draws what
// `ToastStack.geometry` answers.

/// A toast's kind. Its colour sits on the ICON alone.
public enum ToastKind: String, CaseIterable, Sendable, Equatable {
    case success
    case error
    case info
    case warning
}

/// The optional one button a toast carries. Running it dismisses the toast.
public struct ToastAction: Sendable {
    public let label: String
    public let handler: @Sendable () -> Void

    public init(label: String, handler: @escaping @Sendable () -> Void) {
        self.label = label
        self.handler = handler
    }
}

/// One toast: a sentence, an optional description, an optional action.
public struct ToastItem: Identifiable, Sendable, Equatable {
    public let id: UUID
    public let kind: ToastKind
    public let title: String
    public let description: String?
    public let action: ToastAction?

    public init(
        id: UUID = UUID(),
        kind: ToastKind,
        title: String,
        description: String? = nil,
        action: ToastAction? = nil
    ) {
        self.id = id
        self.kind = kind
        self.title = title
        self.description = description
        self.action = action
    }

    public static func == (lhs: ToastItem, rhs: ToastItem) -> Bool {
        lhs.id == rhs.id && lhs.kind == rhs.kind && lhs.title == rhs.title
            && lhs.description == rhs.description
    }
}

public enum ToastStack {
    /// The fixture's `constants` (points).
    public enum Constants {
        public static let width: Double = 356
        public static let gap: Double = 14
        public static let peek: Double = 14
        public static let scaleStep: Double = 0.05
        public static let visible: Int = 3
        public static let durationMs: Int = 4000
        public static let swipeThreshold: Double = 45
        public static let viewportOffset: Double = 24
        public static let mobileViewportOffset: Double = 16
        /// `placement.pointer`: web from `touchMaxWidth` up, the IDE.
        public static let placementPointer: String = "bottom-right"
        /// `placement.touch`: phones — iOS hangs the stack from the top.
        public static let placementTouch: String = "top-center"
        public static let touchMaxWidth: Double = 600
        public static let kinds: [ToastKind] = ToastKind.allCases
    }

    /// One item's placement: `offset` = its top inside the stack box,
    /// `scale` = its width factor, `visible` = whether it draws at all.
    public struct ItemGeometry: Equatable, Sendable {
        public let offset: Double
        public let scale: Double
        public let visible: Bool

        public init(offset: Double, scale: Double, visible: Bool) {
            self.offset = offset
            self.scale = scale
            self.visible = visible
        }
    }

    /// The toast's numbers (N-12: the shared `Stack.layout` takes them).
    public static let stack = StackConstants(
        peek: Constants.peek, scaleStep: Constants.scaleStep,
        visible: Constants.visible, gap: Constants.gap
    )

    /// The stack box's height and every item's placement, `heights` oldest
    /// first (the LAST item is the newest, in front). A bottom-anchored stack
    /// still measures `offset` from the box's top.
    public static func geometry(
        heights: [Double],
        expanded: Bool,
        anchoredBottom: Bool
    ) -> (height: Double, items: [ItemGeometry]) {
        Stack.layout(heights: heights, expanded: expanded, anchoredBottom: anchoredBottom, constants: stack)
    }
}

/// N-12: one stack's numbers: the toast's (toast-stack.json `constants`) or
/// THE Dock pile's (dock.json `stack`).
public struct StackConstants: Equatable, Sendable {
    /// How far an older item peeks out behind the one in front of it.
    public let peek: Double
    /// How much smaller each rank behind the front draws.
    public let scaleStep: Double
    /// How many items draw at all while collapsed.
    public let visible: Int
    /// The space between items while expanded.
    public let gap: Double

    public init(peek: Double, scaleStep: Double, visible: Int, gap: Double) {
        self.peek = peek
        self.scaleStep = scaleStep
        self.visible = visible
        self.gap = gap
    }
}

/// N-12: THE stack geometry ×4 (TS reference src/stack.ts `stackLayout`),
/// shared by the toasts and the Dock pile.
public enum Stack {
    private static func round(_ value: Double) -> Double {
        (value * 1e4).rounded() / 1e4
    }

    /// `heights` oldest first (the last is the front). `anchoredBottom` = the
    /// box grows upward from its bottom edge; `offset` is from the box's top.
    public static func layout(
        heights: [Double],
        expanded: Bool,
        anchoredBottom: Bool,
        constants: StackConstants
    ) -> (height: Double, items: [ToastStack.ItemGeometry]) {
        let count = heights.count
        guard count > 0 else { return (0, []) }
        // Distance of each item's anchor-side edge from the box's anchor edge.
        let lead: [Double] = heights.indices.map { index in
            expanded
                ? heights[(index + 1)...].reduce(0) { $0 + $1 + constants.gap }
                : constants.peek * Double(count - 1 - index)
        }
        var height: Double = 0
        for (index, h) in heights.enumerated() { height = max(height, lead[index] + h) }
        let items = heights.enumerated().map { index, h -> ToastStack.ItemGeometry in
            let rank = count - 1 - index
            let offset = round(anchoredBottom ? height - lead[index] - h : lead[index])
            if expanded { return ToastStack.ItemGeometry(offset: offset, scale: 1, visible: true) }
            return ToastStack.ItemGeometry(
                offset: offset,
                scale: round(1 - constants.scaleStep * Double(min(rank, constants.visible - 1))),
                visible: rank < constants.visible
            )
        }
        return (round(height), items)
    }
}

/// The live toasts, oldest first, plus the expanded flag. A value type the
/// host's `Toaster` owns.
public struct ToastQueue: Equatable, Sendable {
    /// More than this and the oldest drop off the back.
    public static let cap = 10

    public private(set) var items: [ToastItem] = []
    public var expanded = false

    public init() {}

    /// Add `item` as the newest (in front).
    public mutating func append(_ item: ToastItem) {
        items.append(item)
        if items.count > Self.cap {
            items.removeFirst(items.count - Self.cap)
        }
    }

    /// Remove one toast; an emptied stack collapses.
    public mutating func dismiss(_ id: UUID) {
        items.removeAll { $0.id == id }
        if items.isEmpty { expanded = false }
    }

    public mutating func removeAll() {
        items.removeAll()
        expanded = false
    }
}
