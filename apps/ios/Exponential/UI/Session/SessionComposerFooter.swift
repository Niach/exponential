import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the expanded steer composer's footer row, web's order: `+`
/// attach (wave D: Photo | File) · the model pill (the session's `model`
/// config option; picking one SENDS `/model <alias>` as a plain message,
/// EXP-877; codex gets a plain label; before the relay reports one it falls
/// back to the row/device model) · `Plan mode` (blue while the run is in plan
/// mode, read-only: EXP-790 keeps plan mode launch-time) · spacer · the usage
/// ring (hidden until context data lands; opens the Usage sheet, EXP-1150).
/// Rides `GlassComposer`'s `tools:` slot, so the send button stays the card's
/// own. A thread's box carries the same controls for its ROOT session, minus
/// the plan chip on a dispatcher root.
struct SessionComposerFooter: View {
    /// The session view's own controls: the plan chip, the model pill and
    /// the usage ring.
    struct Controls {
        let planModeActive: Bool
        /// `AgentSessionModel.composerModelValue` (live, else the row, else
        /// the device/contract defaults) — nil hides the pill.
        let modelValue: String?
        /// The run's agent — claude gets the picker, anything else a label.
        let agent: String
        let onPickModel: (String) -> Void
        let usageFraction: Double?
        let usageSeverity: AgentUsageSeverity
        let onUsage: () -> Void
        /// A dispatcher root (`sessions.dispatch`) has no plan mode of its
        /// own: its children take the launch's picks, so no plan chip.
        var showsPlan: Bool = true
    }

    /// nil = the bare box (the `+` alone).
    let controls: Controls?
    let attachEnabled: Bool
    /// Wave D: the `+` opens the Photo | File sub-choice (iOS only, see
    /// `SteerAttachChoiceMenu`): the photo picker or the Files importer.
    let onPhoto: () -> Void
    let onFile: () -> Void
    /// Owned by the host so an open menu (its cover resigns the field's
    /// focus) never reads as an idle composer and folds it away.
    @Binding var attachMenuOpen: Bool

    @State private var attachAnchor: CGRect = .zero

    var body: some View {
        // ONE row that claims the tools slot's width (`layoutPriority`), so
        // `GlassComposer`'s own spacer collapses and the ring sits beside Send.
        HStack(spacing: GlassComposerTokens.toolSpacing) {
            // EXP-850 §13: the steer composers attach with the `ui-add` (plus)
            // concept ×4; `editor-image` stays the comment/description glyph.
            GlassComposerToolButton(
                AppIcons.uiAdd,
                accessibilityLabel: "Add file or image",
                enabled: attachEnabled
            ) {
                var transaction = Transaction()
                transaction.disablesAnimations = true
                withTransaction(transaction) { attachMenuOpen = true }
            }
            .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
                attachAnchor = frame
            }
            .steerAttachChoiceMenu(
                isPresented: $attachMenuOpen,
                anchor: attachAnchor,
                onPhoto: onPhoto,
                onFile: onFile
            )

            if let controls {
                modelPill(controls)

                if controls.showsPlan {
                    Text(AgentFeed.planModeFooterLabel)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(
                            controls.planModeActive
                                ? DesignTokens.Semantic.blue
                                : .white.opacity(TextOpacity.tertiary)
                        )
                        .lineLimit(1)
                        .accessibilityIdentifier("session-plan-mode")
                }
            }

            Spacer(minLength: 0)

            // No context data yet = no ring (web/Android draw nothing then).
            if let controls, controls.usageFraction != nil {
                Button(action: controls.onUsage) {
                    ContextRing(fraction: controls.usageFraction, severity: controls.usageSeverity)
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
        }
        .layoutPriority(1)
    }

    @ViewBuilder
    private func modelPill(_ controls: Controls) -> some View {
        let agent = controls.agent
        if let modelValue = controls.modelValue {
            if agent == "claude" {
                GlassMenu {
                    ForEach(LaunchVocabulary.modelValues(for: agent), id: \.self) { value in
                        GlassMenuItem(LaunchVocabulary.modelLabel(value)) {
                            controls.onPickModel(value)
                        }
                    }
                } label: {
                    ModelLabel(text: LaunchVocabulary.modelLabel(modelValue), chevron: true)
                }
                .accessibilityLabel("Model")
                .accessibilityIdentifier("session-model-pill")
            } else {
                ModelLabel(text: LaunchVocabulary.modelLabel(modelValue), chevron: false)
                    .accessibilityLabel("Model")
                    .accessibilityIdentifier("session-model-pill")
            }
        }
    }
}

/// The model read-out: plain muted text + chevron, no capsule (web/desktop
/// draw no pill background in the composer row).
private struct ModelLabel: View {
    let text: String
    let chevron: Bool

    var body: some View {
        HStack(spacing: 4) {
            Text(text)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
            if chevron {
                AppIcon(AppIcons.uiChevronDown, size: 10)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .frame(minHeight: GlassComposerTokens.toolHitSize)
        .padding(.horizontal, 4)
        .contentShape(Rectangle())
    }
}
