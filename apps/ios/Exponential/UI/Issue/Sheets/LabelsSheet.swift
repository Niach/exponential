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

/// Searchable multi-toggle label sheet (EXP-240): rows toggle assignment and
/// the sheet STAYS open; a `+ Create new label "query"` row appears when the
/// query has no case-insensitive exact match and creates + assigns with the
/// deterministic auto color (no swatch picking in this flow).
struct LabelsSheet: View {
    /// The issue's team's labels, name-sorted by the caller.
    let labels: [LabelEntity]
    let assignedIds: Set<String>
    let onToggle: (String) -> Void
    let onCreate: (String) -> Void

    @State private var searchText = ""

    private var trimmedQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var filtered: [LabelEntity] {
        guard !trimmedQuery.isEmpty else { return labels }
        return labels.filter { $0.name.localizedCaseInsensitiveContains(trimmedQuery) }
    }

    private var showsCreateRow: Bool {
        !trimmedQuery.isEmpty
            && !labels.contains { $0.name.caseInsensitiveCompare(trimmedQuery) == .orderedSame }
    }

    var body: some View {
        GlassSheetChrome(title: "Labels", detents: [.medium, .large]) {
            GlassSheetSearchField(placeholder: "Search or create labels", text: $searchText)
            ScrollView {
                VStack(spacing: 2) {
                    ForEach(filtered, id: \.id) { label in
                        GlassSheetRow(
                            label: label.name,
                            selected: assignedIds.contains(label.id),
                            action: { onToggle(label.id) }
                        ) {
                            Circle()
                                .fill(Color(hex: label.color) ?? .gray)
                                .frame(width: 10, height: 10)
                        }
                    }

                    if showsCreateRow {
                        GlassSheetRow(
                            label: "Create new label \u{201C}\(trimmedQuery)\u{201D}",
                            action: {
                                onCreate(trimmedQuery)
                                searchText = ""
                            }
                        ) {
                            Image(systemName: "plus")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                    }

                    if filtered.isEmpty, !showsCreateRow {
                        Text("No labels yet — type a name to create one.")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.top, 16)
                    }
                }
                .padding(.horizontal, 6)
                .padding(.bottom, 16)
            }
        }
    }
}
