import ExpUI
import ExpCore
import SwiftUI

/// The label block both issue editors wear (EXP-698 r5): a text-only heading
/// over a cloud of ALL the team's labels as select pills, plus a "+ Label"
/// action pill.
///
/// It was the New-issue page's block; the Properties sheet had its own
/// near-copy that listed only the ASSIGNED labels behind a gutter glyph, so
/// adding a label there meant a second sheet while creating one took a tap.
/// One block, one behaviour: tap a pill to toggle, tap "+ Label" for whatever
/// the host puts behind it (creating a team label on the create page, the
/// searchable sheet on the properties one).
struct IssueLabelsSelector: View {
    /// The team's labels, name-sorted by the caller.
    let labels: [LabelEntity]
    let selectedIds: Set<String>
    let onToggle: (String) -> Void
    let onAdd: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // EXP-698 r4: a plain heading. The leading gutter glyph went away
            // with the metadata rows' — a section title above a chip cloud
            // needs no icon to be found.
            Text("Labels")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            FlowLayout(spacing: 6) {
                ForEach(labels, id: \.id) { label in
                    GlassPill(
                        label.name,
                        mode: .select(isSelected: selectedIds.contains(label.id)) {
                            onToggle(label.id)
                        },
                        dot: Color(hex: label.color) ?? .gray
                    )
                }
                GlassPill("Label", icon: AppIcons.uiAdd, mode: .action(onAdd))
            }
        }
    }
}
