import ExpCore
import ExpUI
import SwiftUI

/// EXP-897/EXP-980 — the prompt a start on BLOCKED work raises, identical ×4.
/// EXP-1215: the app's ONE alert card (`GlassAlert`), the transitive chain
/// (`IssueGraphView`) in its content slot under the sentence, so the reader
/// sees WHAT is in the way before choosing. Copy byte-locked by
/// `blocked-start.json`.
///
/// Three answers: Cancel · `Start anyway` · `Stacked PR` (primary). The
/// stacked answer is never hidden: it is DISABLED with its reason note, or,
/// when it starts a line of 2+ issues, says which one goes first
/// (`BlockedStart.stackPlan`, SLOP-3); the note sits under the graph. Return
/// takes Stacked PR, or Start anyway while that one is disabled.
@MainActor
enum BlockedStartAlert {
    static func card(
        _ prompt: BlockedStartPrompt,
        onStartAnyway: @escaping () -> Void,
        onStartStacked: @escaping () -> Void,
        onOpenIssue: @escaping (String) -> Void
    ) -> GlassAlert<BlockedStartAlertContent> {
        GlassAlert(
            title: prompt.title,
            actions: [
                GlassAlertAction("Cancel", role: .outline, isCancel: true, id: "cancel") {},
                GlassAlertAction(
                    BlockedStart.startAnywayLabel, role: .outline, isDefault: !prompt.stackable,
                    id: "start-anyway", handler: onStartAnyway
                ),
                GlassAlertAction(
                    BlockedStart.stackedPrLabel, role: .primary, enabled: prompt.stackable,
                    id: "stacked-pr", handler: onStartStacked
                ),
            ]
        ) {
            BlockedStartAlertContent(prompt: prompt, onOpenIssue: onOpenIssue)
        }
    }
}

/// The slot: the sentence (identifiers monospaced inside it), the graph, and
/// Stacked PR's note. The graph scrolls inside `graphMaxHeight` (Android's
/// scrolling content slot, web's `max-h` card) so a long chain never pushes
/// the row off the screen; a short one takes only its own height.
struct BlockedStartAlertContent: View {
    let prompt: BlockedStartPrompt
    let onOpenIssue: (String) -> Void

    /// The tallest the graph gets before it scrolls: room for the title, the
    /// sentence, the note and the row on the shortest phone.
    static let graphMaxHeight: CGFloat = 300

    @State private var graphHeight: CGFloat = 0

    var body: some View {
        VStack(alignment: .leading, spacing: GlassAlertMetrics.itemGap) {
            sentence
                .font(.system(size: 15))
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("glass-alert-message")

            ScrollView(.vertical) {
                IssueGraphView(
                    graph: prompt.graph, issues: prompt.issues, onOpenIssue: onOpenIssue
                )
                .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { graphHeight = $0 }
            }
            .scrollBounceBehavior(.basedOnSize)
            // Fit-or-scroll: the ScrollView is greedy, so it is given the
            // graph's own height up to the cap instead of all the room.
            .frame(height: min(graphHeight, Self.graphMaxHeight))
            .accessibilityIdentifier("blocked-start-graph")

            if let note = prompt.stackNote {
                Text(note)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("blocked-start-stacked-note")
            }
        }
    }

    /// One issue keeps the EXP-897 sentence, identifiers monospaced inside it;
    /// a batch says the shared batch line.
    private var sentence: Text {
        guard !prompt.isBatch else { return Text(BlockedStart.blockedBatchBody) }
        let names = prompt.identifiers.filter { !$0.isEmpty }
        var sentence = Text(BlockedStart.bodyPrefix)
        for (index, name) in names.enumerated() {
            if index > 0 {
                sentence = sentence + Text(index == names.count - 1 ? " and " : ", ")
            }
            sentence = sentence + Text(name).font(.system(size: 15).monospaced())
        }
        return sentence + Text(
            prompt.stackable ? BlockedStart.bodySuffixStackable : BlockedStart.bodySuffix
        )
    }
}
