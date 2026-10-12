import ExpCore
import SwiftUI
import UIKit

// SLOP-28 (EXP-1259 D4/D5) — THE search list on iOS: ONE component for every
// place that searches a list and picks from it (search-list.json ×4, web
// `SearchList`, desktop `ui::search_list`, Android `SearchList.kt`): the `#`
// issue typeahead's bottom sheet, the composer "+" menu's Implement issue and
// Run action pages, the composer's issue picker and the Search tab.
//
// Anatomy: an INPUT row (the nav-search glyph, the field, trailing the muted
// result COUNT once a query is typed) over the hairline, then the ROWS — the
// picker's own rows (`GlassPickerSheetRow`, 44pt on phones; no new row type)
// in a scroller with a bottom fade while more rows sit below. Return picks
// the TOP row; Backspace in an EMPTY input reports `onBackspaceEmpty` (the
// `#` typeahead closes and deletes its `#`).

/// The input row's field: a UIKit text field, because SwiftUI's cannot report
/// a Backspace on an EMPTY field (no text changes) and the `#` typeahead
/// closes on exactly that.
struct SearchListTextField: UIViewRepresentable {
    @Binding var text: String
    let placeholder: String
    var identifier: String = SearchList.TestId.input
    let autofocus: Bool
    let onSubmit: () -> Void
    let onBackspaceEmpty: (() -> Void)?

    final class Field: UITextField {
        var onBackspaceEmpty: (() -> Void)?

        override func deleteBackward() {
            if (text ?? "").isEmpty { onBackspaceEmpty?() }
            super.deleteBackward()
        }
    }

    @MainActor final class Coordinator: NSObject, UITextFieldDelegate {
        var parent: SearchListTextField
        var focusedOnce = false

        init(_ parent: SearchListTextField) {
            self.parent = parent
        }

        @objc func changed(_ field: UITextField) {
            parent.text = field.text ?? ""
        }

        func textFieldShouldReturn(_ textField: UITextField) -> Bool {
            parent.onSubmit()
            return false
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> Field {
        let field = Field()
        field.delegate = context.coordinator
        field.text = text
        field.font = .preferredFont(forTextStyle: .body)
        field.textColor = .white
        field.tintColor = .white
        field.returnKeyType = .search
        field.autocorrectionType = .no
        field.autocapitalizationType = .none
        field.attributedPlaceholder = NSAttributedString(
            string: placeholder,
            attributes: [.foregroundColor: UIColor.white.withAlphaComponent(TextOpacity.tertiary)]
        )
        field.accessibilityIdentifier = identifier
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        field.addTarget(context.coordinator, action: #selector(Coordinator.changed(_:)), for: .editingChanged)
        field.onBackspaceEmpty = onBackspaceEmpty
        return field
    }

    func updateUIView(_ field: Field, context: Context) {
        context.coordinator.parent = self
        field.onBackspaceEmpty = onBackspaceEmpty
        if field.text != text { field.text = text }
        if autofocus, !context.coordinator.focusedOnce {
            context.coordinator.focusedOnce = true
            // After the sheet's own presentation transaction, or the first
            // responder request is dropped.
            DispatchQueue.main.async { field.becomeFirstResponder() }
        }
    }
}

/// THE search list: the input row and the picker rows under it.
public struct SearchListView<Value: Hashable & Sendable>: View {
    let items: [PickerItem<Value>]
    @Binding var query: String
    let placeholder: String
    let mode: PickerMode
    let value: Set<Value>
    /// True = the list filters `items` itself (`PickerSearch`); false = the
    /// caller already ranked them for `query` (the issue engine, EXP-892).
    let filters: Bool
    /// The input's accessibility identifier (a host's own, e.g. the
    /// composer's submenu search); `search-list-input` by default.
    let inputIdentifier: String
    let autofocus: Bool
    let loading: Bool
    let emptyText: String?
    let footer: (() -> AnyView)?
    let onPick: (Value) -> Void
    let onBackspaceEmpty: (() -> Void)?

    public init(
        items: [PickerItem<Value>],
        query: Binding<String>,
        placeholder: String,
        mode: PickerMode = .single,
        value: Set<Value> = [],
        filters: Bool = false,
        inputIdentifier: String = SearchList.TestId.input,
        autofocus: Bool = false,
        loading: Bool = false,
        emptyText: String? = nil,
        footer: (() -> AnyView)? = nil,
        onPick: @escaping (Value) -> Void,
        onBackspaceEmpty: (() -> Void)? = nil
    ) {
        self.items = items
        self._query = query
        self.placeholder = placeholder
        self.mode = mode
        self.value = value
        self.filters = filters
        self.inputIdentifier = inputIdentifier
        self.autofocus = autofocus
        self.loading = loading
        self.emptyText = emptyText
        self.footer = footer
        self.onPick = onPick
        self.onBackspaceEmpty = onBackspaceEmpty
    }

    private var visible: [PickerItem<Value>] {
        filters ? PickerSearch.filter(items, query: query) : items
    }

    private var hasQuery: Bool {
        !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    public var body: some View {
        VStack(spacing: 0) {
            input
            GlassDivider()
            rows
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SearchList.TestId.list)
    }

    private var input: some View {
        HStack(spacing: 10) {
            AppIcon(AppIcons.navSearch, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            SearchListTextField(
                text: $query,
                placeholder: placeholder,
                identifier: inputIdentifier,
                autofocus: autofocus,
                onSubmit: {
                    if let top = visible.first(where: { !$0.disabled }) { onPick(top.value) }
                },
                onBackspaceEmpty: onBackspaceEmpty
            )
            .frame(maxWidth: .infinity)
            if hasQuery, !loading {
                Text("\(visible.count)")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityIdentifier(SearchList.TestId.count)
            }
        }
        .padding(.horizontal, GlassPickerTokens.searchHPadding)
        .frame(height: CGFloat(SearchList.inputHeight) + 8)
    }

    private var rows: some View {
        ScrollView {
            LazyVStack(spacing: GlassPickerTokens.rowSpacing) {
                if loading {
                    SkeletonRows(rowHeight: CGFloat(SearchList.rowHeightPhone), dividers: false)
                } else if visible.isEmpty {
                    if let emptyText {
                        EmptyState(title: emptyText, size: .list)
                    }
                } else {
                    ForEach(visible) { item in
                        GlassPickerSheetRow(
                            item: item,
                            checked: item.checkedState(in: value),
                            style: GlassPickerTokens.selectionStyle(for: mode),
                            mark: nil,
                            content: nil
                        ) {
                            onPick(item.value)
                        }
                        .accessibilityIdentifier(SearchList.TestId.row)
                    }
                }
                if let footer { footer() }
            }
            .padding(.horizontal, GlassPickerTokens.listHPadding)
            .padding(.top, 6)
            .padding(.bottom, GlassPickerTokens.listBottomPadding)
        }
        .scrollDismissesKeyboard(.never)
        .overlay(alignment: .bottom) {
            if visible.count > SearchList.maxVisibleRows {
                LinearGradient(
                    colors: [GlassSheetTokens.background.opacity(0), GlassSheetTokens.background],
                    startPoint: .top,
                    endPoint: .bottom
                )
                .frame(height: CGFloat(SearchList.fadeHeight))
                .allowsHitTesting(false)
            }
        }
    }
}

/// The search list as a phone BOTTOM SHEET (our sheet chrome, never a system
/// popover): the `#` typeahead and the global search wear it. Large detent so
/// the list keeps its height while the result count changes under the
/// keyboard.
public struct SearchListSheet<Value: Hashable & Sendable>: View {
    let list: SearchListView<Value>

    public init(_ list: SearchListView<Value>) {
        self.list = list
    }

    public var body: some View {
        list
            .padding(.top, 14)
            .frame(maxHeight: .infinity, alignment: .top)
            .presentationDetents([.large])
            .presentationBackground(GlassSheetTokens.background)
            .presentationCornerRadius(GlassSheetTokens.cornerRadius)
            .presentationDragIndicator(.visible)
            .presentationContentInteraction(.scrolls)
            .preferredColorScheme(.dark)
    }
}
