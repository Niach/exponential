import ExpCore
import ExpUI
import SwiftUI

/// EXP-897/EXP-980 — the prompt a start on BLOCKED work raises, identical ×4.
///
/// It used to be a UIKit alert, which could hold a sentence and three buttons
/// and nothing else. It now hosts the transitive chain (`IssueGraphView`), so
/// it is a sheet: the reader can see WHAT is in the way before choosing.
///
/// Cancel · `Start anyway` · `Stacked PR`, and the stacked button is never
/// hidden any more — where the shared rule says it cannot run (a cycle, a
/// batch, a machine without `stacked-start`) it is DISABLED and its reason
/// note sits under the row.
struct BlockedStartSheet: View {
    let prompt: BlockedStartPrompt
    /// nil = the stacked start is on.
    let disabledReason: StackStart.StackDisabledReason?
    let onCancel: () -> Void
    let onStartAnyway: () -> Void
    let onStartStacked: () -> Void
    let onOpenIssue: (String) -> Void

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
        guard !prompt.isBatch else { return Text(StackStart.blockedBatchBody) }
        let names = prompt.identifiers.filter { !$0.isEmpty }
        var sentence = Text(StackStart.bodyPrefix)
        for (index, name) in names.enumerated() {
            if index > 0 {
                sentence = sentence + Text(index == names.count - 1 ? " and " : ", ")
            }
            sentence = sentence + Text(name).font(.subheadline.monospaced())
        }
        return sentence + Text(StackStart.bodySuffix)
    }

    @ViewBuilder
    private var actions: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                GlassPill("Cancel", size: .md, mode: .action(onCancel))
                GlassPill(
                    StackStart.startAnywayLabel, size: .md, mode: .action(onStartAnyway)
                )
                Spacer(minLength: 0)
                GlassPill(
                    StackStart.stackedPrLabel,
                    size: .md,
                    mode: .action(onStartStacked),
                    primary: true,
                    enabled: disabledReason == nil
                )
                .accessibilityIdentifier("blocked-start-stacked")
            }
            if let disabledReason {
                Text(StackStart.stackDisabledNote(disabledReason))
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("blocked-start-disabled-note")
            }
        }
    }
}
