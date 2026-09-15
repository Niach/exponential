import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the expanded steer composer's footer row — desktop
/// `steer-composer.tsx` in concept: `Plan mode` (blue while the run is in
/// plan mode, read-only: EXP-790 keeps plan mode launch-time) · `+` attach ·
/// spacer · the model pill (the session's `model` config option; picking one
/// SENDS `/model <alias>` as a plain message, EXP-877; codex gets a plain
/// label; hidden when the engine reported no model) · the usage ring, which
/// opens the Usage sheet. Rides `GlassComposer`'s `tools:` slot, so the send
/// button stays the card's own.
struct SessionComposerFooter: View {
    let planModeActive: Bool
    let attachEnabled: Bool
    let onAttach: () -> Void
    /// `WorkFaces.sessionModel` — nil hides the pill.
    let modelValue: String?
    /// The run's agent — claude gets the picker, anything else a label.
    let agent: String
    let onPickModel: (String) -> Void
    let usageFraction: Double?
    let usageSeverity: AgentUsageSeverity
    let onUsage: () -> Void

    var body: some View {
        Text(AgentFeed.planModeFooterLabel)
            .font(.caption.weight(.medium))
            .foregroundStyle(
                planModeActive ? DesignTokens.Semantic.blue : .white.opacity(TextOpacity.tertiary)
            )
            .lineLimit(1)
            .padding(.trailing, 4)
            .accessibilityIdentifier("session-plan-mode")

        // EXP-850 §13: the steer composers attach with the `ui-add` (plus)
        // concept ×4; `editor-image` stays the comment/description glyph.
        GlassComposerToolButton(
            AppIcons.uiAdd,
            accessibilityLabel: "Attach image",
            enabled: attachEnabled,
            action: onAttach
        )

        Spacer(minLength: 0)

        modelPill

        Button(action: onUsage) {
            ContextRing(fraction: usageFraction, severity: usageSeverity)
                .frame(
                    width: GlassComposerTokens.toolHitSize,
                    height: GlassComposerTokens.toolHitSize
                )
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Usage")
        .accessibilityIdentifier("session-context-ring")
    }

    @ViewBuilder
    private var modelPill: some View {
        if let modelValue {
            if agent == "claude" {
                GlassMenu {
                    ForEach(LaunchVocabulary.modelValues(for: agent), id: \.self) { value in
                        GlassMenuItem(LaunchVocabulary.modelLabel(value)) {
                            onPickModel(value)
                        }
                    }
                } label: {
                    OptionPillLabel(text: LaunchVocabulary.modelLabel(modelValue))
                }
                .accessibilityLabel("Model")
                .accessibilityIdentifier("session-model-pill")
            } else {
                OptionPillLabel(text: LaunchVocabulary.modelLabel(modelValue), chevron: false)
                    .accessibilityLabel("Model")
                    .accessibilityIdentifier("session-model-pill")
            }
        }
    }
}
