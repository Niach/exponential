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
/// (`BlockedStart.stackPlan`, SLOP-3); the note sits under the graph.
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
                GlassAlertAction("Cancel", role: .outline, id: "cancel") {},
                GlassAlertAction(
                    BlockedStart.startAnywayLabel, role: .outline, id: "start-anyway",
                    handler: onStartAnyway
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
/// Stacked PR's note.
struct BlockedStartAlertContent: View {
    let prompt: BlockedStartPrompt
    let onOpenIssue: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: GlassAlertMetrics.itemGap) {
            sentence
                .font(.system(size: 15))
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("glass-alert-message")

            IssueGraphView(
                graph: prompt.graph, issues: prompt.issues, onOpenIssue: onOpenIssue
            )

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
