import Combine
import SwiftUI

// N-14 (refines N-6) — WHERE a picker's rows open, ×4
// (`packages/domain-contract/fixtures/picker-presentation.json`). The
// collapsed trigger never changes; the rows open DOWNWARDS: inside a ROW host
// (THE composer options row) into a slot under the whole row, as wide as the
// row, so the trigger and its siblings never move; elsewhere directly under
// the trigger. One slot shows ONE picker's rows.

/// Where an open picker's body draws. The fixture's `body`.
public enum PickerBodyPlacement: String, CaseIterable, Equatable, Sendable {
    /// The platform's own surface — the phone's bottom sheet (`popover`).
    case surface
    /// Inline, in the row host's slot under the whole row.
    case rowSlot
    /// Inline, directly under the trigger at the host's width.
    case underTrigger
}

/// THE placement rule, replayed against the fixture's `placementCases`.
/// `inDialog` = the host is a sheet (`\.pickerPresentation == .inline`),
/// `inRow` = a row host offers a slot (`\.inlinePickerSlot`), `requested` =
/// the caller's own `presentation`, which wins over the host.
public func pickerPlacement(
    inDialog: Bool,
    inRow: Bool,
    requested: PickerPresentation?
) -> (presentation: PickerPresentation, body: PickerBodyPlacement) {
    let presentation = requested ?? (inDialog ? .inline : .sheet)
    switch presentation {
    case .sheet: return (.sheet, .surface)
    case .inline: return (.inline, inRow ? .rowSlot : .underTrigger)
    }
}

/// The row host's slot: the ONE open picker's rows, drawn under the row.
/// A picker publishes its body on open and clears it on fold; a second
/// picker's open replaces the first's rows and folds the first.
@MainActor
public final class InlinePickerSlot: ObservableObject {
    @Published public private(set) var owner: UUID?
    @Published public private(set) var content: AnyView?
    private var onReplaced: (() -> Void)?

    public init() {}

    public var isOpen: Bool { owner != nil }

    /// Shows `owner`'s rows. Another owner's rows are replaced and that
    /// picker is told to fold (`onReplaced`); the same owner republishes
    /// (its value or rows changed) without folding anything.
    public func publish(owner: UUID, content: AnyView, onReplaced: @escaping () -> Void) {
        if let previous = self.owner, previous != owner {
            let fold = self.onReplaced
            self.onReplaced = nil
            fold?()
        }
        self.owner = owner
        self.content = content
        self.onReplaced = onReplaced
    }

    /// Folds `owner`'s rows; a no-op when another picker owns the slot (a
    /// replaced picker's own fold must not clear its successor).
    public func clear(owner: UUID) {
        guard self.owner == owner else { return }
        self.owner = nil
        self.content = nil
        self.onReplaced = nil
    }
}

public struct InlinePickerSlotKey: EnvironmentKey {
    public static let defaultValue: InlinePickerSlot? = nil
}

extension EnvironmentValues {
    /// The enclosing row host's slot; nil = not in a row host.
    public var inlinePickerSlot: InlinePickerSlot? {
        get { self[InlinePickerSlotKey.self] }
        set { self[InlinePickerSlotKey.self] = newValue }
    }
}

/// A ROW host: its content (the pins, untouched) and, while a picker in it
/// is open INLINE, that picker's rows under the whole row at the row's width,
/// capped at `GlassPickerTokens.inlineMaxHeight`. On a page (no sheet host)
/// the pickers keep their sheets and the slot stays empty.
public struct InlinePickerRow<Content: View>: View {
    @StateObject private var slot = InlinePickerSlot()
    private let content: Content

    public init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            content
                .environment(\.inlinePickerSlot, slot)
            if let rows = slot.content {
                rows
                    .id(slot.owner)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .frame(maxHeight: GlassPickerTokens.inlineMaxHeight)
                    .environment(\.inlinePickerSlot, nil)
                    .accessibilityIdentifier("picker-row-slot")
            }
        }
    }
}

extension View {
    /// Makes this view a ROW host (`InlinePickerRow`).
    public func inlinePickerRow() -> some View {
        InlinePickerRow { self }
    }
}
