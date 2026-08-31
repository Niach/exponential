import ExpCore
import ExpUI
import SwiftUI

// EXP-615: the ONE device/agent/model/effort block. Start coding, Chat,
// Create action and the automation editor used to hand-roll four slightly
// different versions of the same rows (different labels, a pill strip here, a
// picker row there); they now render this, so the four dialogs read the same
// on every platform.
//
// `Variant.launch` is a RUN's options: the Device picker (only when there is a
// choice), the agent capsule, Model + Effort and the launch toggles.
// `Variant.automation` configures a BINDING instead — its device row says
// "Runs on" and lists every automation-capable machine plainly (offline
// included: a sleeping box still owns the binding and fires the missed
// schedule when it comes back), and there are no toggles (an automated run
// takes the machine's own). Everything else is IDENTICAL: EXP-615 retired the
// automation-only "Device default" agent segment, so both variants render the
// same brand-marked capsule over the machine's runnable agents, and
// Model/Effort speak the launch "CLI default" sentinel — blank is what stores
// NULL on the row and lets the machine decide.
//
// `Variant.device` (EXP-694) is a MACHINE's stored launch defaults — the
// device-settings sheet used to hand-roll the same rows. It has no device
// picker (the sheet IS one machine) and folds that agent's account + usage
// rows in through `accountFooter`.
//
// EXP-694 (S3/S4): everything below the device picker is ONE grouped card —
// the agent strip is its first row (embedded, no capsule of its own), then
// Model + Effort, then the toggles the caller bound, then the footer slot.
struct LaunchOptionsSection: View {
    enum Variant {
        case launch
        case automation
        case device
    }

    /// EXP-481's "Resume previous session" offer — launch variant only.
    struct ResumeRow {
        let isOn: Binding<Bool>
        /// The issue the existing worktree belongs to (nil renders "this issue").
        let identifier: String?
        let branch: String
        /// Whether the offer is currently taken (hides the plan-mode toggle).
        let active: Bool
    }

    let variant: Variant
    /// The machines this block offers. Empty renders `noDeviceNote` instead.
    let devices: [SteerDevice]
    @Binding var deviceId: String
    /// Why there is nothing to pick — the caller words it per subject.
    let noDeviceNote: String
    /// The resolved machine's runnable agents, in contract order.
    let availableAgents: [String]
    /// The pinned agent — always a concrete one, seeded by the caller from the
    /// resolved machine's own default (EXP-615).
    let agent: String
    let onAgentChange: (String) -> Void
    @Binding var model: String
    @Binding var effort: String
    var ultracode: Binding<Bool>? = nil
    var planMode: Binding<Bool>? = nil
    var resumeRow: ResumeRow? = nil
    /// EXP-694: the device variant tabs every agent the machine REPORTED on,
    /// which can include one it has no editable defaults for — its tab shows
    /// the account/usage rows only. Everything else always has options.
    var showsOptions: Bool = true
    /// A sentence under the card (the device variant's offline notice). The
    /// resume note, when there is one, sits above it.
    var footerNote: String? = nil
    /// The device variant's account + usage rows, folded into the SAME card as
    /// the options they belong to (EXP-688 put them in this section; EXP-694
    /// moved the section here).
    var accountFooter: (() -> AnyView)? = nil

    var body: some View {
        Group {
            if variant != .device {
                deviceSection
            }
            optionsSection
        }
    }

    // MARK: - Device

    /// The launch variant hides a one-machine picker (there is nothing to
    /// choose); the automation variant always shows it — the binding names the
    /// machine that owns it, so it must be visible even when there is one.
    private var showsDevicePicker: Bool {
        variant == .automation ? !devices.isEmpty : devices.count > 1
    }

    private var deviceTitle: String {
        variant == .automation ? "Runs on" : "Device"
    }

    /// No section header on either variant (EXP-615 dedupe) — the picker row
    /// already says "Runs on" / "Device".
    @ViewBuilder
    private var deviceSection: some View {
        if devices.isEmpty {
            Section {
                Text(noDeviceNote)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .listRowBackground(glassFormRowFill)
        } else if showsDevicePicker {
            Section {
                GlassPickerRow(
                    deviceTitle,
                    selection: $deviceId,
                    options: devices.map(\.deviceId),
                    label: { id in
                        devices.first { $0.deviceId == id }
                            .map(LaunchVocabulary.deviceCaption) ?? id
                    }
                )
            }
            .listRowBackground(glassFormRowFill)
        }
    }

    // MARK: - Agent / model / effort

    /// A binding offers "CLI default" for EVERY agent — a blank pin stores
    /// NULL on the row — where a run only offers it where the CLI has one
    /// (claude's model is explicit-always).
    private var modelOptions: [String] {
        variant == .automation
            ? [LaunchVocabulary.cliDefault]
                + LaunchVocabulary.automationModelValues(for: agent)
            : LaunchVocabulary.modelValues(for: agent)
    }

    private var effortOptions: [String] {
        [LaunchVocabulary.cliDefault] + LaunchVocabulary.effortValues(for: agent)
    }

    private func effortRowLabel(_ value: String) -> String {
        value == LaunchVocabulary.cliDefault
            ? "CLI default"
            : LaunchVocabulary.effortLabel(value)
    }

    private var effortTitle: String {
        LaunchVocabulary.effortTitle(for: agent)
    }

    /// Ultracode IS `--effort ultracode`, so it disables the Effort picker; a
    /// binding has no toggles, so its row is always live.
    private var effortEnabled: Bool {
        variant == .automation || ultracode?.wrappedValue != true
    }

    /// ONE grouped card (EXP-694): the agent strip is its first row, then
    /// Model + Effort, then whatever toggles the caller bound, then the
    /// account/usage slot. The strip used to float above the card as an
    /// outlined capsule in a section of its own; it is now embedded — no fill,
    /// no border, 8pt row insets all round, and the hairline separator below it
    /// is deliberately left visible so it reads as a row of the card.
    private var optionsSection: some View {
        Section {
            // A lone option is not a choice, on any variant. No container
            // accessibility label: it would merge the segment buttons into one
            // VoiceOver element.
            if availableAgents.count > 1 {
                GlassSegmentedControl(
                    options: availableAgents,
                    selection: agent,
                    label: { LaunchVocabulary.agentLabel($0) },
                    icon: { Image("agent-\($0)") },
                    style: .embedded,
                    onSelect: { onAgentChange($0) }
                )
                // EXP-642: the store slide's pop-out rect is measured off this
                // strip (`PopRects`), so it needs a stable handle. `contain`
                // keeps the individual segments queryable.
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("start-coding-agent-picker")
                .listRowInsets(EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))
            }
            if showsOptions {
                GlassPickerRow(
                    "Model",
                    selection: $model,
                    options: modelOptions,
                    label: { LaunchVocabulary.modelLabel($0) }
                )
                GlassPickerRow(
                    effortTitle,
                    selection: $effort,
                    options: effortOptions,
                    label: { effortRowLabel($0) },
                    enabled: effortEnabled
                )
                // EXP-208: no helper notices, like the IDE. Ultracode is
                // claude-only, plan mode is claude+pi (EXP-441); a binding
                // (the automation variant) has neither.
                if let resumeRow {
                    Toggle("Resume previous session", isOn: resumeRow.isOn)
                }
                if let ultracode, agent == "claude" {
                    Toggle("Ultracode", isOn: ultracode)
                }
                // A resume never re-enters plan mode (the machine clamps it
                // too) — hide the toggle while one is active.
                if let planMode, LaunchVocabulary.supportsPlanMode(agent),
                   resumeRow?.active != true {
                    Toggle("Plan mode", isOn: planMode)
                }
            }
            if let accountFooter {
                accountFooter()
            }
        } footer: {
            optionsFooter
        }
        .listRowBackground(glassFormRowFill)
    }

    private var resumeNote: String? {
        guard let resumeRow, resumeRow.active else { return nil }
        return "A worktree for \(resumeRow.identifier ?? "this issue") already exists (\(resumeRow.branch))."
    }

    @ViewBuilder
    private var optionsFooter: some View {
        if resumeNote != nil || footerNote != nil {
            VStack(alignment: .leading, spacing: 4) {
                if let resumeNote {
                    Text(resumeNote)
                }
                if let footerNote {
                    Text(footerNote)
                }
            }
        }
    }
}
