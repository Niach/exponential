import SwiftUI

// EXP-1029 contract — THE picker primitive on iOS (EXP-1021 implements it).
//
// One primitive per platform, typed pickers on top, the same names
// everywhere: web `packages/ui/src/picker` (`Picker`), IDE `ui::picker`,
// Android `ui/components/picker`. SwiftUI owns the bare `Picker` name, so
// the iOS primitive is `GlassPicker`; the typed pickers keep their names.
//
// Presentation belongs to the primitive, never to the caller: on the phone
// every picker is a bottom sheet (`GlassPickerSheet`) of PLAIN rows — no
// cards inside the sheet; multi-select marks rows by the highlight colour,
// no circles; swipe down closes. `search` adds the filter field at the top.
// The trigger is whatever chip or button the caller hands in; the primitive
// owns the sheet.
//
// This file is the CONTRACT: the item and mode types and a stub view that
// renders the trigger only. `ExpUI/Tests/PickerContractTests.swift` carries
// the presentation rules and the typed-picker gate, skipped until EXP-1021.

/// One row of a picker.
public struct PickerItem<Value: Hashable>: Identifiable, Sendable where Value: Sendable {
    public var id: Value { value }
    /// The stable identity of the row; also what search matches on.
    public let value: Value
    /// What the row reads as; also the default search keyword.
    public let label: String
    /// A leading glyph — an `AppIcons` name, never a raw SF Symbol.
    public let icon: String?
    /// A colour for the glyph (a board's hex, a label's dot, a status tone).
    public let color: Color?
    /// A muted second line or trailing note (an email, a branch age).
    public let description: String?
    /// Rendered, never pickable.
    public let disabled: Bool
    /// Extra search terms (an identifier, an email).
    public let keywords: [String]

    public init(
        value: Value,
        label: String,
        icon: String? = nil,
        color: Color? = nil,
        description: String? = nil,
        disabled: Bool = false,
        keywords: [String] = []
    ) {
        self.value = value
        self.label = label
        self.icon = icon
        self.color = color
        self.description = description
        self.disabled = disabled
        self.keywords = keywords
    }

    /// The keywords the row matches on: the explicit ones, else its label.
    public var searchKeywords: [String] { keywords.isEmpty ? [label] : keywords }
}

public enum PickerMode: Equatable, Sendable {
    /// Closes on a pick; `onChange` gets the one value.
    case single
    /// Toggles without closing; `onChange` gets the whole new set.
    case multi
}

/// THE picker. Contract stub: renders the trigger only; the sheet comes with
/// EXP-1021.
public struct GlassPicker<Value: Hashable & Sendable, Trigger: View>: View {
    public let items: [PickerItem<Value>]
    public let mode: PickerMode
    /// The current selection (at most one value in `.single`).
    public let value: Set<Value>
    /// The whole new selection (one value in `.single`).
    public let onChange: (Set<Value>) -> Void
    /// A filter field at the top of the sheet.
    public let search: Bool
    /// What an empty list (or an empty search) reads as.
    public let emptyText: String?
    /// The sheet's title.
    public let title: String?
    public let disabled: Bool
    private let trigger: () -> Trigger

    public init(
        items: [PickerItem<Value>],
        mode: PickerMode,
        value: Set<Value>,
        onChange: @escaping (Set<Value>) -> Void,
        search: Bool = false,
        emptyText: String? = nil,
        title: String? = nil,
        disabled: Bool = false,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.items = items
        self.mode = mode
        self.value = value
        self.onChange = onChange
        self.search = search
        self.emptyText = emptyText
        self.title = title
        self.disabled = disabled
        self.trigger = trigger
    }

    public var body: some View {
        trigger()
            .accessibilityIdentifier("picker")
    }
}
