import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: the expanded steer composer's footer row, web's order: `+`
/// attach (wave D: Photo | File) · the model pill (the session's `model`
/// config option; picking one SENDS `/model <alias>` as a plain message,
/// EXP-877; codex and an ENDED row get a plain label; before the relay
/// reports one it falls back to the row/device model) · `Plan mode` (blue,
/// only while the run is in plan mode — or the row's pick before the relay
/// reports; read-only: EXP-790 keeps plan mode launch-time) · spacer · the usage
/// ring (hidden until context data lands; opens the Usage sheet, EXP-1150).
/// Rides `GlassComposer`'s `tools:` slot, so the send button stays the card's
/// own. A thread's box carries the same controls for its ROOT session, minus
/// the plan chip on a dispatcher root.
struct SessionComposerFooter: View {
    /// The session view's own controls: the plan chip, the model pill and
    /// the usage ring.
    struct Controls {
        /// The run is in plan mode right now (the latest `config_state`).
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
        /// The session's row ENDED: the model pin is a read-only label (web
        /// disables the picker on `status === 'ended'`).
        var ended: Bool = false
        /// Before the relay reports a `config_state`, the synced row's own
        /// `planMode` pick stands in for the read-out (web parity).
        var planModeFallback: Bool = false

        /// Codex has no mid-run model prompt and an ended row takes no
        /// `/model`: both draw the chevron-less label.
        var canPickModel: Bool { !ended && agent == "claude" }

        /// The plan read-out shows only while plan mode is in force (or the
        /// row says so before the relay does) — never a muted "off" label.
        var showsPlanLabel: Bool { showsPlan && (planModeActive || planModeFallback) }
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
    /// EXP-1334: the `+` opens the host's composer-menu.json sheet instead
    /// of the bare Photo | File choice (the thread composer).
    var onPlus: (() -> Void)?

    @State private var attachAnchor: CGRect = .zero

    var body: some View {
        // ONE row that claims the tools slot's width (`layoutPriority`), so
        // `GlassComposer`'s own spacer collapses and the ring sits beside Send.
        HStack(spacing: GlassComposerTokens.toolSpacing) {
            // EXP-850 §13: the steer composers attach with the `ui-add` (plus)
            // concept ×4; `editor-image` stays the comment/description glyph.
            GlassComposerToolButton(
                AppIcons.uiAdd,
                accessibilityLabel: onPlus == nil ? "Add file or image" : ComposerMenu.plusLabel,
                enabled: onPlus != nil || attachEnabled
            ) {
                if let onPlus {
                    onPlus()
                    return
                }
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
                SessionModelPill(controls: controls)

                if controls.showsPlanLabel {
                    Text(AgentFeed.planModeFooterLabel)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(DesignTokens.Semantic.blue)
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
}

/// The model pill (`/model <alias>` on claude; a plain label otherwise).
struct SessionModelPill: View {
    let controls: SessionComposerFooter.Controls

    var body: some View {
        let agent = controls.agent
        if let modelValue = controls.modelValue {
            if controls.canPickModel {
                // EXP-1345: THE Picker (bottom sheet, trailing check).
                ModelPicker(
                    agent: agent,
                    value: modelValue,
                    onChange: { controls.onPickModel($0) },
                    trigger: { ModelLabel(text: LaunchVocabulary.modelLabel(modelValue), chevron: true) }
                )
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

/// EXP-1334: the thread composer's options line UNDER the card — the model
/// pill, the Plan switch (live mid-run through `sessions.configure`; nil =
/// none) and, trailing, the usage ring (hidden until context data lands).
struct SessionComposerOptionsLine: View {
    let controls: SessionComposerFooter.Controls
    var plan: Binding<Bool>?

    var body: some View {
        HStack(spacing: 12) {
            SessionModelPill(controls: controls)
            if let plan {
                HStack(spacing: 6) {
                    Text("Plan")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Toggle("Plan", isOn: plan)
                        .labelsHidden()
                        .toggleStyle(.switch)
                        .controlSize(.mini)
                        .tint(DesignTokens.Semantic.blue)
                        .disabled(controls.ended)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("session-plan-toggle")
            }
            Spacer(minLength: 0)
            if controls.usageFraction != nil {
                Button(action: controls.onUsage) {
                    ContextRing(fraction: controls.usageFraction, severity: controls.usageSeverity)
                        .frame(width: GlassComposerTokens.toolHitSize, height: GlassComposerTokens.toolHitSize)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Usage")
                .accessibilityIdentifier("session-context-ring")
            }
        }
        .padding(.horizontal, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-composer-options")
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
