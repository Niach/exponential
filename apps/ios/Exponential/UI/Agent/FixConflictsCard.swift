import ExpCore
import ExpUI
import SwiftUI

/// EXP-1233: the Fix merge conflicts builtin's CARD — what the composer's
/// leading slot draws instead of the generic "Pull request" picker row
/// (web `FixConflictsCard`, desktop `chat_screen::render_fix_conflicts_card`,
/// Android `FixConflictsCard`, ×4):
///
///   ┌ ⑂ #2117  ⎇ exp/APP-14 → master                            ⌄ ┐  PR row
///   │ ⚠ Merge refused: the branch has conflicts.                   │  note
///   └──────────────────────────────────────────────────────────────┘
///
/// The PR ROW is the picker's trigger (the chevron says so): the open-PR
/// glyph in the Reviews row's green, the number in mono, the branch glyph and
/// `branch → base` in mono at 70% (`FixConflictsPr.branchLine`: the base
/// omitted while unknown); nothing picked = the muted placeholder alone. The
/// NOTE under a hairline shows only when a refused merge opened the composer
/// (the seed's `conflict`): a pick made from the action picker has nothing
/// to explain. The surface is the borderless group (`.glassSection()`, web's
/// `GlassGroup`). Words = the contract's `composerUi*`.
struct FixConflictsCard: View {
    let model: AgentComposerModel
    let def: ActionInputDto

    @State private var showsPicker = false

    var body: some View {
        let pr = model.fixConflictsPr
        let pool = model.pullRequests
        VStack(spacing: 0) {
            prRow(pr, enabled: !pool.isEmpty && !model.sending)
            if model.conflictRefused, pr != nil {
                GlassDivider()
                note
            }
        }
        .glassSection()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer-fix-conflicts")
        // EXP-1021: the SHARED picker's sheet, opened by the PR row below
        // (the card draws its own trigger, so the picker's stays hidden).
        .background {
            GlassPicker(
                items: pool.map { PickerItem(value: $0.issueId, label: $0.label) },
                mode: .single,
                value: [model.value(for: def)],
                onChange: { picked in
                    guard let issueId = picked.first else { return }
                    model.setValue(issueId, for: def)
                },
                title: "Select a pull request",
                open: $showsPicker,
                hideTrigger: true
            ) {
                EmptyView()
            }
        }
    }

    /// The trigger row. A plain tap target (not a Button), so it reads the
    /// same inside the composer card as the other picker rows.
    private func prRow(_ pr: FixConflictsPr?, enabled: Bool) -> some View {
        HStack(spacing: 12) {
            if let pr {
                AppIcon(AppIcons.prOpen, size: 16)
                    .foregroundStyle(IssueStatus.inReview.color)
                if let number = pr.prNumber {
                    Text(verbatim: "#\(number)")
                        .font(.subheadline.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.primary))
                        .fixedSize()
                }
                let line = pr.branchLine
                if !line.isEmpty {
                    HStack(spacing: 6) {
                        AppIcon(AppIcons.uiBranch, size: 14)
                        Text(line)
                            .font(.subheadline.monospaced())
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    .foregroundStyle(.white.opacity(0.7))
                }
            } else {
                Text(DomainContract.composerUiPrPlaceholder)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            AppIcon(AppIcons.uiChevronDown, size: 14)
                .foregroundStyle(.white.opacity(enabled ? 0.5 : TextOpacity.quaternary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
        .onTapGesture {
            guard enabled else { return }
            showsPicker = true
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(accessibilityLabel(pr))
        .accessibilityIdentifier("agent-composer-fix-conflicts-pr")
    }

    private func accessibilityLabel(_ pr: FixConflictsPr?) -> String {
        guard let pr else { return DomainContract.composerUiPrPlaceholder }
        return pr.prNumber.map { "Pull request #\($0)" } ?? "Pull request"
    }

    /// The refusal line: the warning glyph and the contract's note, in the
    /// destructive tone.
    private var note: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.uiWarning, size: 16)
            Text(DomainContract.composerUiConflictNote)
                .font(.subheadline)
                .lineLimit(2)
            Spacer(minLength: 0)
        }
        .foregroundStyle(DesignTokens.Semantic.red)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("agent-composer-conflict-note")
    }
}
