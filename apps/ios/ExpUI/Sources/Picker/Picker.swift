import SwiftUI

// EXP-1029 contract, EXP-1021 implementation — THE picker primitive on iOS.
//
// One primitive per platform, typed pickers on top, the same names
// everywhere: web `packages/ui/src/picker` (`Picker`), IDE `ui::picker`,
// Android `ui/components/picker`. SwiftUI owns the bare `Picker` name, so
// the iOS primitive is `GlassPicker`; the typed pickers keep their names.
//
// Presentation belongs to the primitive, never to the caller: on the phone
// every picker is a bottom sheet (`GlassSheetChrome`, the ONE shell) of PLAIN
// rows — no card, no section, no bordered shell between the sheet and the
// list; multi-select marks a picked row by the row's own HIGHLIGHT, never a
// leading circle or a trailing checkmark; a swipe down closes (the platform
// sheet gesture, EXP-687). `search` adds the filter field at the top. The
// trigger is whatever chip or button the caller hands in; the primitive owns
// the sheet.
//
// That selection language is the whole of EXP-1021: the app had three picker
// sheets that disagreed — two wrapped their rows in a card, one drew circles,
// the relations linker's issue picker drew plain rows and read best. This is
// that one, generalised, so a picker cannot drift again.
//
// `ExpUI/Tests/PickerContractTests.swift` carries the presentation rules and
// the typed-picker gate (web, IDE and Android carry the same case names).

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
    /// Multi mode only: what THIS row reads as when membership in `value` is
    /// not the whole story — a bulk edit over rows that disagree marks a label
    /// that sits on ALL of them `.all`, on only SOME of them `.some`. Set, it
    /// WINS over membership; absent, the state is derived from it.
    public let checked: PickerChecked?

    public init(
        value: Value,
        label: String,
        icon: String? = nil,
        color: Color? = nil,
        description: String? = nil,
        disabled: Bool = false,
        keywords: [String] = [],
        checked: PickerChecked? = nil
    ) {
        self.value = value
        self.label = label
        self.icon = icon
        self.color = color
        self.description = description
        self.disabled = disabled
        self.keywords = keywords
        self.checked = checked
    }

    /// How this row reads: its explicit `checked` when the caller set one,
    /// else plain membership in the selection.
    public func checkedState(in selection: Set<Value>) -> PickerChecked {
        checked ?? (selection.contains(value) ? PickerChecked.all : PickerChecked.none)
    }

    /// The keywords the row matches on: the explicit ones, else its label.
    public var searchKeywords: [String] { keywords.isEmpty ? [label] : keywords }

    /// What the row draws in its leading slot. A colour WITH a glyph tints the
    /// glyph; a colour WITHOUT one draws the row's DOT — that is what makes a
    /// label row a coloured dot and a board row a tinted glyph without any
    /// call site choosing a shape.
    public var mark: PickerRowMark {
        if let icon { return .glyph(icon, color) }
        if let color { return .dot(color) }
        return .plain
    }
}

/// The leading slot of a picker row — derived from the item, never picked by
/// a caller (`PickerItem.mark`).
public enum PickerRowMark: Equatable {
    case glyph(String, Color?)
    case dot(Color)
    case plain
}

public enum PickerMode: Equatable, Sendable {
    /// Closes on a pick; `onChange` gets the one value.
    case single
    /// Toggles without closing; `onChange` gets the whole new set.
    case multi
}

/// How a picked row reads. There is deliberately exactly ONE case: a second
/// idiom — a leading circle, a trailing check — is how the three sheets
/// EXP-1021 replaced drifted apart in the first place. Web passes the same
/// single value (`selectionStyle="highlight"`).
public enum PickerSelectionStyle: String, CaseIterable, Sendable {
    case highlight
}

/// How much of a multi-selection a row carries. THREE weights of the ONE
/// highlight — a partial pick is the same wash without the stroke, never a
/// check-and-minus column beside the rows (which is what the bulk label sheet
/// this replaced drew). Web spells it `boolean | "indeterminate"`, Android
/// `PickerChecked`; the paint is identical.
public enum PickerChecked: String, CaseIterable, Sendable {
    /// Not picked.
    case none
    /// Picked on SOME of the rows a bulk edit covers.
    case some
    /// Picked (on all of them).
    case all
}

/// The picker sheet's own metrics and fills. Every value is a read of the
/// shared glass tokens: a literal here would be the drift this issue exists
/// to end.
public enum GlassPickerTokens {
    /// The one and only way a picked row reads.
    public static let selectionStyle: PickerSelectionStyle = .highlight

    /// 44pt — the same tap target every glass sheet row has.
    public static let rowMinHeight: CGFloat = 44
    public static let rowRadius: CGFloat = GlassTokens.rowRadius
    public static let rowSpacing: CGFloat = 2
    public static let rowHPadding: CGFloat = 14
    /// The leading slot's width, so labels line up whether a row marks itself
    /// with a glyph, a dot or nothing at all.
    public static let markWidth: CGFloat = 24
    public static let dotSize: CGFloat = 10

    /// A RESTING row is plain: no fill, no hairline, nothing between it and
    /// the sheet. This is the "no cards inside the sheet" rule, as a value.
    public static let restingFill: Color = .clear
    public static let restingStroke: Color = .clear
    /// A PICKED row is the one bright fill + its paired stroke — the whole
    /// selection language.
    public static let pickedFill: Color = GlassTokens.fillActive
    public static let pickedStroke: Color = GlassTokens.strokeActive

    /// A PARTIAL pick: the same wash WITHOUT the inset stroke, so all three
    /// states read as weights of one mark rather than two different idioms.
    public static let partialFill: Color = GlassTokens.fillActive
    public static let partialStroke: Color = .clear

    /// The list's own inset inside the sheet (matching every other glass
    /// sheet's row list) and the search field's.
    public static let listHPadding: CGFloat = 6
    public static let listBottomPadding: CGFloat = 16
    public static let searchHPadding: CGFloat = 16
    public static let searchVPadding: CGFloat = 8
}

/// The `search` filter: a row matches when ANY of its keywords contains the
/// query, case- and diacritic-insensitively. Pure, so the contract test can
/// name it without a view.
public enum PickerSearch {
    public static func matches<Value>(_ item: PickerItem<Value>, query: String) -> Bool {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return true }
        return item.searchKeywords.contains { $0.localizedCaseInsensitiveContains(trimmed) }
    }

    public static func filter<Value>(
        _ items: [PickerItem<Value>], query: String
    ) -> [PickerItem<Value>] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return items }
        return items.filter { matches($0, query: trimmed) }
    }
}

/// What a tap on a row does — the difference between the two modes, in one
/// place so the sheet body has no mode branches of its own.
public enum PickerSelection {
    /// The whole new selection after a tap on `value`. Single REPLACES (and
    /// never clears — the sheet has no third state); multi TOGGLES.
    public static func picking<Value: Hashable>(
        _ value: Value, in selection: Set<Value>, mode: PickerMode
    ) -> Set<Value> {
        switch mode {
        case .single:
            return [value]
        case .multi:
            var next = selection
            if next.contains(value) {
                next.remove(value)
            } else {
                next.insert(value)
            }
            return next
        }
    }

    /// Single closes on a pick; multi stays open across toggles.
    public static func closesOnPick(_ mode: PickerMode) -> Bool { mode == .single }
}

/// THE picker: the caller's trigger, and the bottom sheet it opens.
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
    /// The filter field's placeholder; nil derives one from `title`.
    public let searchPlaceholder: String?
    /// Controlled search text — for an external ranking engine
    /// (`IssueSearch.rank` + the debounced server hits, EXP-892). With it the
    /// primitive stops filtering and renders `items` verbatim, in the caller's
    /// order.
    public let query: Binding<String>?
    /// A muted "Loading…" row instead of the rows, while the caller is still
    /// reading its pool. Distinct from `emptyText`: "nothing yet" and "nothing
    /// at all" are different answers.
    public let loading: Bool
    /// Controlled presentation, for a host that opens the picker from its own
    /// menu item or its own property row (web's `PickerSurfaceProps`). With
    /// `hideTrigger` the trigger is never rendered at all.
    public let open: Binding<Bool>?
    public let hideTrigger: Bool
    /// Fires once the sheet finished dismissing — what a hand-off (the
    /// duplicate-status interception) is promoted on: a sheet cannot present
    /// while its sibling is still animating away.
    public let onDismiss: (() -> Void)?
    /// REPLACES the search field and the rows with an inline body — the icon
    /// picker's swatch grid, the one caller. `AnyView` rather than a second
    /// generic parameter: one more generic on `GlassPicker` would re-spell
    /// every typed picker's `Body` for the sake of that single grid.
    public let panel: (() -> AnyView)?
    /// Rendered UNDER the rows — the labels sheet's "Create new label …" row.
    public let footer: (() -> AnyView)?
    /// Replaces a row's leading MARK. The selection language stays the
    /// primitive's, so a custom mark can never invent a second "this is
    /// picked" idiom.
    public let renderMark: ((PickerItem<Value>) -> AnyView?)?
    /// EXP-1030 — a row's own DETAIL, drawn under its label where the
    /// description sits: the account picker's inline limit bars (EXP-992),
    /// which are a drawing and not a string. Like `renderMark` it may not
    /// touch the row's fill or its stroke, so the selection language stays
    /// the primitive's.
    public let renderDetail: ((PickerItem<Value>) -> AnyView?)?
    /// EXP-1030 — an accessibility identifier for the SHEET, for a surface
    /// whose flow is pinned by one (the composer's `#` and ▶ tools). The
    /// trigger keeps `picker`; only the presented list takes this.
    public let sheetIdentifier: String?
    private let trigger: () -> Trigger

    /// The sheet's presentation while no caller controls it. The primitive
    /// owns it — a caller that owned it would own the presentation rules too.
    @State private var isPresented = false

    public init(
        items: [PickerItem<Value>],
        mode: PickerMode,
        value: Set<Value>,
        onChange: @escaping (Set<Value>) -> Void,
        search: Bool = false,
        emptyText: String? = nil,
        title: String? = nil,
        disabled: Bool = false,
        searchPlaceholder: String? = nil,
        query: Binding<String>? = nil,
        loading: Bool = false,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        panel: (() -> AnyView)? = nil,
        footer: (() -> AnyView)? = nil,
        renderMark: ((PickerItem<Value>) -> AnyView?)? = nil,
        renderDetail: ((PickerItem<Value>) -> AnyView?)? = nil,
        sheetIdentifier: String? = nil,
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
        self.searchPlaceholder = searchPlaceholder
        self.query = query
        self.loading = loading
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.panel = panel
        self.footer = footer
        self.renderMark = renderMark
        self.renderDetail = renderDetail
        self.sheetIdentifier = sheetIdentifier
        self.trigger = trigger
    }

    /// The caller's binding when there is one, else the primitive's own state.
    private var presented: Binding<Bool> {
        open ?? Binding(get: { isPresented }, set: { isPresented = $0 })
    }

    public var body: some View {
        Group {
            if hideTrigger {
                // A host-driven picker still needs a node to hang the sheet
                // on; it just must not draw one.
                Color.clear.frame(width: 0, height: 0)
            } else {
                // The trigger is a LABEL, never its own button: the primitive
                // owns the tap so every picker opens the same way (and a
                // disabled picker simply does not).
                Button {
                    presented.wrappedValue = true
                } label: {
                    trigger()
                }
                .buttonStyle(.plain)
                .disabled(disabled)
            }
        }
        .accessibilityIdentifier("picker")
        .sheet(isPresented: presented, onDismiss: onDismiss) {
            GlassPickerSheetBody(
                items: items,
                mode: mode,
                value: value,
                onChange: onChange,
                search: search,
                emptyText: emptyText,
                title: title,
                searchPlaceholder: searchPlaceholder,
                query: query,
                loading: loading,
                panel: panel,
                footer: footer,
                renderMark: renderMark,
                renderDetail: renderDetail
            )
            // Sheets present outside the host's environment, and the app is
            // dark-forced everywhere else (EXP-687).
            .preferredColorScheme(.dark)
            .modifier(PickerSheetIdentifier(identifier: sheetIdentifier))
        }
    }
}

/// Names the presented sheet for the UI flows pinned to one, and nothing at
/// all without an identifier — a modifier rather than an inline `if`, which
/// would fork the sheet's view identity and re-present it.
private struct PickerSheetIdentifier: ViewModifier {
    let identifier: String?

    func body(content: Content) -> some View {
        if let identifier {
            content
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier(identifier)
        } else {
            content
        }
    }
}

/// The sheet: the ONE shell, a pinned filter field, and the rows. Nothing
/// between the shell and the rows — that is the rule, and it is visible here
/// as the absence of a container.
private struct GlassPickerSheetBody<Value: Hashable & Sendable>: View {
    let items: [PickerItem<Value>]
    let mode: PickerMode
    let value: Set<Value>
    let onChange: (Set<Value>) -> Void
    let search: Bool
    let emptyText: String?
    let title: String?
    let searchPlaceholder: String?
    /// The caller's query when an external engine ranks the rows, else nil and
    /// the primitive filters with `localQuery`.
    let query: Binding<String>?
    let loading: Bool
    let panel: (() -> AnyView)?
    let footer: (() -> AnyView)?
    let renderMark: ((PickerItem<Value>) -> AnyView?)?
    let renderDetail: ((PickerItem<Value>) -> AnyView?)?

    @Environment(\.dismiss) private var dismiss
    @State private var localQuery = ""

    private var queryBinding: Binding<String> {
        query ?? Binding(get: { localQuery }, set: { localQuery = $0 })
    }

    var body: some View {
        GlassSheetChrome(
            title: title,
            pinnedHeader: {
                // The filter must not scroll away with the rows, and a panel
                // brings its own body — there is nothing to filter.
                if search, panel == nil {
                    GlassSheetSearchField(
                        placeholder: searchPlaceholder
                            ?? "Search \(title?.lowercased() ?? "options")",
                        text: queryBinding
                    )
                    .padding(.horizontal, GlassPickerTokens.searchHPadding)
                    .padding(.vertical, GlassPickerTokens.searchVPadding)
                }
            },
            content: {
                if let panel {
                    panel()
                } else {
                    rows
                }
            }
        )
    }

    /// A caller-ranked list is rendered VERBATIM: it already is the answer to
    /// the query, in its own order.
    private var visible: [PickerItem<Value>] {
        query == nil ? PickerSearch.filter(items, query: localQuery) : items
    }

    private var rows: some View {
        LazyVStack(spacing: GlassPickerTokens.rowSpacing) {
            if loading {
                ProgressView()
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 24)
            } else if visible.isEmpty {
                // `emptyText` covers BOTH an empty list and an empty search —
                // one string, so a picker never says two different things
                // about having nothing to show.
                if let emptyText {
                    Text(emptyText)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 24)
                }
            } else {
                ForEach(visible) { item in
                    GlassPickerSheetRow(
                        item: item,
                        checked: item.checkedState(in: value),
                        mark: renderMark?(item),
                        detail: renderDetail?(item)
                    ) {
                        onChange(PickerSelection.picking(item.value, in: value, mode: mode))
                        if PickerSelection.closesOnPick(mode) { dismiss() }
                    }
                }
            }
            if let footer { footer() }
        }
        .padding(.horizontal, GlassPickerTokens.listHPadding)
        .padding(.bottom, GlassPickerTokens.listBottomPadding)
    }
}

/// One picker row: its mark, its label, its muted second line — and, when it
/// is picked, the highlight that says so.
private struct GlassPickerSheetRow<Value: Hashable & Sendable>: View {
    let item: PickerItem<Value>
    let checked: PickerChecked
    /// A caller-drawn leading slot (an avatar); nil = the item's own mark.
    let mark: AnyView?
    /// A caller-drawn block under the label (the account limits bars).
    let detail: AnyView?
    let action: () -> Void

    private var fill: Color {
        switch checked {
        case .all: GlassPickerTokens.pickedFill
        case .some: GlassPickerTokens.partialFill
        case .none: GlassPickerTokens.restingFill
        }
    }

    private var stroke: Color {
        switch checked {
        case .all: GlassPickerTokens.pickedStroke
        case .some: GlassPickerTokens.partialStroke
        case .none: GlassPickerTokens.restingStroke
        }
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                leading
                VStack(alignment: .leading, spacing: 2) {
                    Text(item.label)
                        .font(.subheadline)
                        .foregroundStyle(
                            .white.opacity(item.disabled ? TextOpacity.quaternary : TextOpacity.primary)
                        )
                        .lineLimit(1)
                    if let description = item.description, !description.isEmpty {
                        Text(description)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                    if let detail { detail }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, GlassPickerTokens.rowHPadding)
            .frame(minHeight: GlassPickerTokens.rowMinHeight)
            .background(fill, in: RoundedRectangle(cornerRadius: GlassPickerTokens.rowRadius))
            .overlay(
                RoundedRectangle(cornerRadius: GlassPickerTokens.rowRadius)
                    .strokeBorder(stroke, lineWidth: GlassTokens.hairline)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // A disabled row RENDERS — it is information, not absence — but never
        // picks.
        .disabled(item.disabled)
    }

    @ViewBuilder
    private var leading: some View {
        if let mark {
            mark.frame(width: GlassPickerTokens.markWidth)
        } else {
            switch item.mark {
            case let .glyph(name, color):
                AppIcon(name, size: AppIcon.Size.medium)
                    .foregroundStyle(color ?? .white.opacity(TextOpacity.secondary))
                    .frame(width: GlassPickerTokens.markWidth)
            case let .dot(color):
                Circle()
                    .fill(color)
                    .frame(width: GlassPickerTokens.dotSize, height: GlassPickerTokens.dotSize)
                    .frame(width: GlassPickerTokens.markWidth)
            case .plain:
                EmptyView()
            }
        }
    }
}
