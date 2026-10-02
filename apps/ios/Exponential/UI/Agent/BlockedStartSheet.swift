import ExpCore
import ExpUI
import SwiftUI

/// EXP-897/EXP-980 — the prompt a start on BLOCKED work raises, identical ×4.
///
/// It hosts the transitive chain (`IssueGraphView`), so it is a sheet rather
/// than an alert: the reader can see WHAT is in the way before choosing.
///
/// Three answers: Cancel · `Start anyway` · `Stacked PR` (primary). The
/// stacked answer is never hidden: it is DISABLED with its reason note
/// (`BlockedStart.stackTarget`, SLOP-3).
struct BlockedStartSheet: View {
    let prompt: BlockedStartPrompt
    let onCancel: () -> Void
    let onStartAnyway: () -> Void
    let onStartStacked: () -> Void
    let onOpenIssue: (String) -> Void

    private var stackable: Bool { prompt.stack.target != nil }

    var body: some View {
        GlassSheetChrome(title: prompt.title, height: .fitted) {
            VStack(alignment: .leading, spacing: 12) {
                body(for: prompt)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .fixedSize(horizontal: false, vertical: true)

                IssueGraphView(
                    graph: prompt.graph, issues: prompt.issues, onOpenIssue: onOpenIssue
                )

                actions
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .accessibilityIdentifier("blocked-start-sheet")
    }

    /// One issue keeps the EXP-897 sentence, identifiers monospaced inside it;
    /// a batch says the shared batch line.
    private func body(for prompt: BlockedStartPrompt) -> Text {
        guard !prompt.isBatch else { return Text(BlockedStart.blockedBatchBody) }
        let names = prompt.identifiers.filter { !$0.isEmpty }
        var sentence = Text(BlockedStart.bodyPrefix)
        for (index, name) in names.enumerated() {
            if index > 0 {
                sentence = sentence + Text(index == names.count - 1 ? " and " : ", ")
            }
            sentence = sentence + Text(name).font(.subheadline.monospaced())
        }
        return sentence + Text(
            stackable ? BlockedStart.bodySuffixStackable : BlockedStart.bodySuffix
        )
    }

    private var actions: some View {
        VStack(alignment: .trailing, spacing: 6) {
            HStack(spacing: 8) {
                GlassPill("Cancel", size: .md, mode: .action(onCancel))
                Spacer(minLength: 0)
                GlassPill(
                    BlockedStart.startAnywayLabel,
                    size: .md,
                    mode: .action(onStartAnyway)
                )
                .accessibilityIdentifier("blocked-start-anyway")
                GlassPill(
                    BlockedStart.stackedPrLabel,
                    size: .md,
                    mode: .action(onStartStacked),
                    primary: true,
                    enabled: stackable
                )
                .accessibilityIdentifier("blocked-start-stacked")
                .accessibilityHint(prompt.stackNote ?? "")
            }
            if let note = prompt.stackNote {
                Text(note)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.trailing)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("blocked-start-stacked-note")
            }
        }
    }
}
