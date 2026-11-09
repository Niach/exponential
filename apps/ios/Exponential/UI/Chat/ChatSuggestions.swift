import ExpCore
import ExpUI
import SwiftUI


/// EXP-1323: what a suggested tasks card can do (`sessions.startSuggestion`
/// / `startAllSuggestions` / `dismissSuggestion`; the ⋯ = THE Menu's sheet:
/// Do it here · Start on… · Start with… · File as issue · Dismiss).
struct ChatSuggestionActions {
    /// Start the item as a child on the defaults.
    var onStart: (_ rowId: String, _ itemId: String) -> Void
    var onStartAll: (_ rowId: String) -> Void
    /// A pick of the item's ⋯ menu (run once its sheet has dismissed).
    var onMenu: (_ rowId: String, _ itemId: String, _ action: SuggestionOptionsSheet.Action) -> Void
    /// Start on…'s rows (the runnable devices), loaded as the menu opens.
    var devices: () async -> [SuggestionOptionsSheet.Device] = { [] }
    /// In-flight keys (`key(rowId, itemId)`; Start all = `key(rowId)`).
    var pending: Set<String> = []

    static func key(_ rowId: String, _ itemId: String? = nil) -> String {
        itemId.map { "\(rowId):\($0)" } ?? "\(rowId):*"
    }
}

/// EXP-1323: THE suggested tasks card ×4 in the Work card's shell (web
/// `ChatSuggestionsCard`): a header (`3 suggested tasks`, `· 2 started`,
/// Start all while more than one is open), then hairline-divided items
/// (`ChatSuggestionItems`). N-12: while any item is open the row is a
/// DockCard and the items are its sheet (`ChatSuggestionsSheet`); in the
/// Conversation the card stays for the rows with no open item (and for a
/// read-only reader).
struct ChatSuggestionsCard: View {
    let rowId: String
    let header: String
    let startAll: Bool
    let items: [SuggestionRow]
    var actions: ChatSuggestionActions?
    var onOpenSession: (String) -> Void = { _ in }
    var onOpenIssue: (String) -> Void = { _ in }

    private typealias Copy = SessionTree.SuggestionsCopy

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiChecklist, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text(header)
                    .fontWeight(.medium)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .accessibilityIdentifier("chat-suggestions-header")
                Spacer(minLength: 8)
                if startAll, let actions {
                    GlassPill(
                        Copy.startAll,
                        mode: .action { actions.onStartAll(rowId) },
                        primary: true,
                        enabled: !actions.pending.contains(ChatSuggestionActions.key(rowId))
                    )
                    .accessibilityIdentifier("chat-suggestions-start-all")
                }
            }
            .padding(.horizontal, 12)
            .frame(minHeight: WorkCards.Geometry.header)
            GlassDivider()
            ChatSuggestionItems(
                rowId: rowId, items: items, actions: actions,
                onOpenSession: onOpenSession, onOpenIssue: onOpenIssue
            )
        }
        .font(.caption)
        .foregroundStyle(.white)
        // N-12: ONE gray ×4: THE card composite under the card hairline.
        .background(GlassTokens.opaqueCardFill)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.groupRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-suggestions")
    }
}

/// N-12: THE suggestions sheet (the DockCard opened, web
/// `ChatSuggestionsSheet`): the items as the card lists them; the footer =
/// Start all while more than one is open.
struct ChatSuggestionsSheet: View {
    let row: Dock.Row
    let rowId: String
    let startAll: Bool
    let items: [SuggestionRow]
    var actions: ChatSuggestionActions?
    var onOpenSession: (String) -> Void = { _ in }
    var onOpenIssue: (String) -> Void = { _ in }
    let onClose: () -> Void

    private typealias Copy = SessionTree.SuggestionsCopy

    var body: some View {
        DockSheetChrome(row: row, onClose: onClose) {
            ChatSuggestionItems(
                rowId: rowId, items: items, actions: actions,
                onOpenSession: onOpenSession, onOpenIssue: onOpenIssue
            )
            .font(.caption)
            .foregroundStyle(.white)
            // The rows pad themselves (web `px-1` + the rows' `px-3`).
            .padding(.horizontal, -12)
            .accessibilityIdentifier("chat-suggestions-sheet")
        } footer: {
            if startAll, let actions {
                GlassPill(
                    Copy.startAll,
                    mode: .action { actions.onStartAll(rowId) },
                    primary: true,
                    enabled: !actions.pending.contains(ChatSuggestionActions.key(rowId))
                )
                .accessibilityIdentifier("chat-suggestions-start-all")
            }
        }
    }
}

/// The suggested tasks' items, hairline-divided (web `SuggestionItemRow`):
/// open = the title and text, the ⋯ (THE Menu's sheet) and Start on one row;
/// started = the child's session card (its caption under the name) opening
/// it; here = `Sent here`; filed = `Filed EXP-9`.
struct ChatSuggestionItems: View {
    let rowId: String
    let items: [SuggestionRow]
    var actions: ChatSuggestionActions?
    var onOpenSession: (String) -> Void = { _ in }
    var onOpenIssue: (String) -> Void = { _ in }

    /// The item whose ⋯ sheet is up, the devices it lists, its pick.
    @State private var menuItem: SuggestionRow?
    @State private var devices: [SuggestionOptionsSheet.Device]?
    @State private var picked: (itemId: String, action: SuggestionOptionsSheet.Action)?
    /// N-6 + N-14: inside a sheet host (THE Dock sheet) the ⋯ discloses
    /// THE Menu's rows INLINE under its item row — one item's at a time.
    @Environment(\.pickerPresentation) private var hostPresentation
    @State private var inlineItemId: String?

    private typealias Copy = SessionTree.SuggestionsCopy
    private var muted: Color { .white.opacity(TextOpacity.tertiary) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                if index > 0 { GlassDivider() }
                itemRow(item)
            }
        }
        .sheet(item: $menuItem, onDismiss: runPick) { item in
            SuggestionOptionsSheet(title: item.title, devices: devices) { action in
                picked = (item.id, action)
                menuItem = nil
            }
            .task {
                guard let actions else { return }
                devices = await actions.devices()
            }
        }
    }

    private func runPick() {
        guard let picked else { return }
        self.picked = nil
        actions?.onMenu(rowId, picked.itemId, picked.action)
    }

    @ViewBuilder
    private func itemRow(_ item: SuggestionRow) -> some View {
        switch item.state {
        case .started:
            if let card = item.card {
                // The card's own shell stands down inside this one.
                SessionCardView(card: card, bare: true) { onOpenSession(card.sessionId) }
                    .accessibilityIdentifier("chat-suggestion-started")
            } else {
                // Started, the child's row not synced yet.
                HStack(spacing: 8) {
                    InlineSpinner(size: 12)
                    Text(item.title).foregroundStyle(muted).lineLimit(1).truncationMode(.tail)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 12)
                .frame(minHeight: WorkCards.Geometry.row)
            }
        case .here, .filed:
            HStack(spacing: 8) {
                Text(item.title).foregroundStyle(muted).lineLimit(1).truncationMode(.tail)
                Spacer(minLength: 8)
                if item.state == .here {
                    Text(Copy.sentHere).foregroundStyle(muted).fixedSize()
                } else {
                    let label = Copy.filed.replacingOccurrences(of: "{issue}", with: item.identifier ?? "")
                    Button {
                        if let issueId = item.issueId { onOpenIssue(issueId) }
                    } label: {
                        Text(label).foregroundStyle(muted).fixedSize()
                    }
                    .buttonStyle(.plain)
                    .disabled(item.issueId == nil)
                }
            }
            .padding(.horizontal, 12)
            .frame(minHeight: WorkCards.Geometry.row)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("chat-suggestion-\(item.state.rawValue)")
        case .open:
            openRow(item)
        }
    }

    /// One row on every width (web `flex items-start gap-3`): the words,
    /// then the ⋯ (ghost) and Start (primary).
    private func openRow(_ item: SuggestionRow) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            openRowLine(item)
            if hostPresentation == .inline, inlineItemId == item.id {
                SuggestionOptionsRows(devices: devices) { action in
                    inlineItemId = nil
                    actions?.onMenu(rowId, item.id, action)
                }
                .frame(maxHeight: GlassPickerTokens.inlineMaxHeight)
                .padding(.bottom, 6)
                .task {
                    guard let actions else { return }
                    devices = await actions.devices()
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-suggestion-options-inline")
            }
        }
    }

    private func openRowLine(_ item: SuggestionRow) -> some View {
        let busy = actions?.pending.contains(ChatSuggestionActions.key(rowId, item.id)) ?? false
        return HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .fontWeight(.medium)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if !item.text.isEmpty {
                    Text(item.text)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let actions {
                HStack(spacing: 6) {
                    GhostIconButton(
                        AppIcons.uiMore,
                        accessibilityLabel: Copy.options,
                        size: 24,
                        glyphSize: 14,
                        enabled: !busy
                    ) {
                        UIApplication.endEditing()
                        if hostPresentation == .inline {
                            inlineItemId = inlineItemId == item.id ? nil : item.id
                        } else {
                            menuItem = item
                        }
                    }
                    .accessibilityIdentifier("chat-suggestion-options")
                    GlassPill(
                        Copy.start,
                        mode: .action { actions.onStart(rowId, item.id) },
                        primary: true,
                        enabled: !busy
                    )
                    .accessibilityIdentifier("chat-suggestion-start")
                }
                .fixedSize()
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-suggestion-open")
    }
}
