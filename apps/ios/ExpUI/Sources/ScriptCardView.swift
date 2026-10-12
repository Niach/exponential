import ExpCore
import SwiftUI

// EXP-1279 (EXP-1259 D13): a code-mode call (contract toolKind `script`) as
// ONE card — "Ran a script · 59 calls · 3.0 s", the calls it made grouped by
// tool (our brand mark on an Exponential tool, the generic tool glyph
// otherwise; first subject + "+n", ×count, failures in rose) and the script
// itself behind the "Script" disclosure, in mono. The model is ExpCore's
// (`ScriptCards.card`, fixture `script-card.json` ×4); this only paints it.
// Web `@exp/ui` `ScriptCard`, desktop `ui::script_card`, Android
// `ScriptCard.kt`.
public struct ScriptCardView: View {
    let card: ScriptCard
    /// The call is still going: the title alone with the run's spinner.
    let running: Bool
    /// The exec call itself failed (not one of its calls).
    let failed: Bool
    /// The exec input's `script`; nil = no disclosure.
    let source: String?

    @State private var open = false
    @Environment(\.motion) private var motion

    public init(card: ScriptCard, running: Bool = false, failed: Bool = false, source: String? = nil) {
        self.card = card
        self.running = running
        self.failed = failed
        let trimmed = source?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        self.source = trimmed.isEmpty ? nil : source
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            if open, let source {
                ScrollView {
                    Text(source)
                        .font(.caption2.monospaced())
                        .foregroundStyle(muted)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 6)
                }
                .frame(maxHeight: 288)
                .fixedSize(horizontal: false, vertical: true)
                .overlay(
                    RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                        .stroke(GlassTokens.strokeRow, lineWidth: GlassTokens.hairline)
                )
                .padding(.horizontal, 12)
                .padding(.bottom, 8)
                .transition(.opacity)
                .accessibilityIdentifier(ScriptCards.TestIds.source)
            }
            if !card.groups.isEmpty || card.overflow != nil {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(card.groups, id: \.tool) { group in
                        groupRow(group)
                    }
                    if let overflow = card.overflow {
                        HStack {
                            Text(overflow).foregroundStyle(muted)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 12)
                        .frame(height: 28)
                    }
                }
                .padding(.bottom, 6)
            }
        }
        .font(.caption)
        .foregroundStyle(.white)
        .background(GlassTokens.fillRow)
        .clipShape(RoundedRectangle(cornerRadius: GlassTokens.groupRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.groupRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(ScriptCards.TestIds.card)
    }

    private var header: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.toolScript, size: 13)
                .foregroundStyle(failed ? DesignTokens.Semantic.red : muted)
            Text(card.title)
                .fontWeight(.medium)
                .foregroundStyle(failed ? DesignTokens.Semantic.red : .white)
                .fixedSize()
            if running { InlineSpinner(size: 12) }
            if let meta = card.meta {
                Text("· \(meta)")
                    .monospacedDigit()
                    .foregroundStyle(muted)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Spacer(minLength: 8)
            if source != nil {
                Button {
                    withAnimation(motion.standard) { open.toggle() }
                } label: {
                    HStack(spacing: 4) {
                        Text(ScriptCards.Copy.script)
                        AppIcon(AppIcons.uiChevronRight, size: 12)
                            .rotationEffect(.degrees(open ? 90 : 0))
                    }
                    .foregroundStyle(muted)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(open ? .isSelected : [])
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 40)
    }

    private func groupRow(_ group: ScriptCardGroup) -> some View {
        HStack(spacing: 8) {
            if group.exponential {
                FeedbackMark(size: 12, color: muted, animated: false)
            } else {
                AppIcon(AppIcons.codingTool, size: 12).foregroundStyle(muted)
            }
            Text(group.label).lineLimit(1).fixedSize()
            if let subject = group.subject {
                Text(group.more.map { "\(subject) \($0)" } ?? subject)
                    .foregroundStyle(muted)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Spacer(minLength: 8)
            if let failedCount = group.failed {
                Text(failedCount).monospacedDigit().foregroundStyle(DesignTokens.Semantic.red)
            } else if let count = group.count {
                Text(count).monospacedDigit().foregroundStyle(muted)
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 28)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(ScriptCards.TestIds.group)
    }

    private var muted: Color { .white.opacity(TextOpacity.tertiary) }
}
