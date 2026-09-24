import ExpUI
import ExpCore
import SwiftUI

/// The auto-color palette — byte-identical to Android's `LabelPalette.colors`
/// (NOT the legacy `suggestedLabelColors` swatch strip, whose values differ):
/// the deterministic pick must land on the same hex on both platforms.
private let autoLabelPalette = [
    "#ef4444", "#f97316", "#f59e0b", "#eab308",
    "#84cc16", "#22c55e", "#10b981", "#14b8a6",
    "#06b6d4", "#3b82f6", "#6366f1", "#8b5cf6",
    "#a855f7", "#d946ef", "#ec4899", "#f43f5e",
    "#64748b", "#6b7280", "#71717a", "#737373",
]

/// Deterministic auto color for one-tap label creation (EXP-240):
/// `palette[abs(hash(lowercased name)) % palette.count]`, where hash is the
/// Java/Kotlin `String.hashCode` (31-based wrapping Int32 over UTF-16 units)
/// so iOS and Android pick the SAME color for the same name. Swift's own
/// `hashValue` is seed-randomized per launch and must never be used here.
func autoLabelColor(for name: String) -> String {
    var hash: Int32 = 0
    for unit in name.lowercased().utf16 {
        hash = hash &* 31 &+ Int32(unit)
    }
    let index = Int(hash.magnitude) % autoLabelPalette.count
    return autoLabelPalette[index]
}

/// EXP-1021 — the labels picker: the shared `LabelPicker` (multi by contract,
/// so the sheet stays open across toggles and a picked row reads by its own
/// highlight, never a checkmark) plus the one thing only this surface has —
/// a `+ Create new label "query"` row when the typed name has no
/// case-insensitive exact match. It creates + assigns with the deterministic
/// auto color above; there is no swatch picking in this flow (EXP-240).
///
/// It is HOST-DRIVEN (`open`): the trigger is a chip in the properties chip
/// box or a row in the Properties sheet, two different view trees, and the
/// host already knows which picker is open.
struct IssueLabelsPicker: View {
    /// The issue's team's labels, name-sorted by the caller.
    let labels: [LabelEntity]
    let assignedIds: Set<String>
    let open: Binding<Bool>
    var onDismiss: (() -> Void)?
    let onToggle: (String) -> Void
    let onCreate: (String) -> Void

    @State private var searchText = ""

    private var trimmedQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// The picker renders what it is handed, so the filter lives here — the
    /// same query that decides whether the create row shows.
    private var filtered: [LabelEntity] {
        guard !trimmedQuery.isEmpty else { return labels }
        return labels.filter { $0.name.localizedCaseInsensitiveContains(trimmedQuery) }
    }

    private var showsCreateRow: Bool {
        !trimmedQuery.isEmpty
            && !labels.contains { $0.name.caseInsensitiveCompare(trimmedQuery) == .orderedSame }
    }

    var body: some View {
        LabelPicker(
            labels: filtered.map(LabelPickerLabel.init),
            value: assignedIds,
            // The picker reports the WHOLE new set; the view model toggles one
            // label at a time, so the difference is what changed.
            onChange: { picked in
                for id in picked.symmetricDifference(assignedIds) { onToggle(id) }
            },
            query: $searchText,
            open: open,
            hideTrigger: true,
            onDismiss: onDismiss,
            footer: showsCreateRow
                ? { AnyView(createRow) }
                : nil,
            trigger: { EmptyView() }
        )
    }

    private var createRow: some View {
        Button {
            onCreate(trimmedQuery)
            searchText = ""
        } label: {
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiAdd, size: AppIcon.Size.small, weight: .semibold)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: GlassPickerTokens.markWidth)
                Text("Create new label \u{201C}\(trimmedQuery)\u{201D}")
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, GlassPickerTokens.rowHPadding)
            .frame(minHeight: GlassPickerTokens.rowMinHeight)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
