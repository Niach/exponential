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

    /// The stack box's height and every item's placement, `heights` oldest
    /// first (the LAST item is the newest, in front). A bottom-anchored stack
    /// still measures `offset` from the box's top.
    public static func geometry(
        heights: [Double],
        expanded: Bool,
        anchoredBottom: Bool
    ) -> (height: Double, items: [ItemGeometry]) {
        let count = heights.count
        guard count > 0 else { return (0, []) }
        let peek = Constants.peek
        let gap = Constants.gap

        let front = heights[count - 1]
        var collapsedHeight = front + peek * Double(count - 1)
        for (index, height) in heights.enumerated() {
            let rank = Double(count - 1 - index)
            collapsedHeight = max(collapsedHeight, height + peek * rank)
        }
        let expandedHeight = heights.reduce(0, +) + gap * Double(count - 1)
        let boxHeight = expanded ? expandedHeight : collapsedHeight

        var items: [ItemGeometry] = []
        items.reserveCapacity(count)
        for (index, height) in heights.enumerated() {
            let rankInt = count - 1 - index
            let rank = Double(rankInt)
            let offset: Double
            if expanded {
                let newer = heights[(index + 1)...].reduce(0, +)
                offset = anchoredBottom
                    ? expandedHeight - newer - rank * gap - height
                    : newer + rank * gap
            } else {
                offset = anchoredBottom
                    ? collapsedHeight - height - rank * peek
                    : rank * peek
            }
            let scale = expanded
                ? 1
                : 1 - Constants.scaleStep * Double(min(rankInt, Constants.visible - 1))
            items.append(
                ItemGeometry(
                    offset: offset,
                    scale: scale,
                    visible: expanded || rankInt < Constants.visible
                )
            )
        }
        return (boxHeight, items)
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
