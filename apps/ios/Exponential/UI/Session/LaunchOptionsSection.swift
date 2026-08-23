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
// schedule when it comes back), the agent capsule leads with a "Device
// default" segment, Model/Effort carry the same sentinel and stay LOCKED
// until an agent is pinned, and there are no toggles (an automated run takes
// the machine's own).
struct LaunchOptionsSection: View {
    enum Variant {
        case launch
        case automation
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
    /// The pinned agent; `LaunchVocabulary.deviceDefault` in the automation
    /// variant means "whatever the machine launches with".
    let agent: String
    let onAgentChange: (String) -> Void
    @Binding var model: String
    @Binding var effort: String
    var ultracode: Binding<Bool>? = nil
    var planMode: Binding<Bool>? = nil
    var skipPermissions: Binding<Bool>? = nil
    var resumeRow: ResumeRow? = nil

    var body: some View {
        Group {
            deviceSection
            optionsSection
            if variant == .launch {
                togglesSection
            }
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

    @ViewBuilder
    private var deviceSection: some View {
        if devices.isEmpty {
            Section {
                Text(noDeviceNote)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } header: {
                if variant == .automation {
                    Text("Runs on")
                }
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
            } header: {
                if variant == .automation {
                    Text("Runs on")
                }
            }
            .listRowBackground(glassFormRowFill)
        }
    }

    // MARK: - Agent / model / effort

    /// The agent capsule's segments: the automation variant leads with the
    /// "Device default" sentinel.
    private var agentOptions: [String] {
        variant == .automation
            ? [LaunchVocabulary.deviceDefault] + availableAgents
            : availableAgents
    }

    private func agentSegmentLabel(_ value: String) -> String {
        value == LaunchVocabulary.deviceDefault
            ? "Device default"
            : LaunchVocabulary.agentLabel(value)
    }

    /// The brand mark of a real agent; the "Device default" segment has none.
    private func agentSegmentIcon(_ value: String) -> Image? {
        value == LaunchVocabulary.deviceDefault ? nil : Image("agent-\(value)")
    }

    /// Model/effort are only meaningful against an agent: with none pinned the
    /// machine's own per-agent defaults apply, so both rows read "Device
    /// default" and stay locked.
    private var pinnedAgent: String? {
        variant == .launch ? agent : (agent.isEmpty ? nil : agent)
    }

    private var modelOptions: [String] {
        guard let pinnedAgent else { return [LaunchVocabulary.deviceDefault] }
        return variant == .automation
            ? [LaunchVocabulary.deviceDefault]
                + LaunchVocabulary.automationModelValues(for: pinnedAgent)
            : LaunchVocabulary.modelValues(for: pinnedAgent)
    }

    private var effortOptions: [String] {
        guard let pinnedAgent else { return [LaunchVocabulary.deviceDefault] }
        return variant == .automation
            ? [LaunchVocabulary.deviceDefault] + LaunchVocabulary.effortValues(for: pinnedAgent)
            : [LaunchVocabulary.cliDefault] + LaunchVocabulary.effortValues(for: pinnedAgent)
    }

    private func modelRowLabel(_ value: String) -> String {
        if variant == .automation, value == LaunchVocabulary.deviceDefault {
            return "Device default"
        }
        return LaunchVocabulary.modelLabel(value)
    }

    private func effortRowLabel(_ value: String) -> String {
        if variant == .automation, value == LaunchVocabulary.deviceDefault {
            return "Device default"
        }
        return value == LaunchVocabulary.cliDefault
            ? "CLI default"
            : LaunchVocabulary.effortLabel(value)
    }

    private var effortTitle: String {
        LaunchVocabulary.effortTitle(for: pinnedAgent ?? "claude")
    }

    /// Ultracode IS `--effort ultracode`, so it disables the Effort picker.
    private var effortEnabled: Bool {
        guard variant == .launch else { return pinnedAgent != nil }
        return ultracode?.wrappedValue != true
    }

    private var optionsSection: some View {
        Section {
            // ONE segmented capsule with the brand mark per agent (web
            // parity, EXP-615 — the loose pill strip that used to ride the
            // Model header is gone). A lone option is not a choice.
            if agentOptions.count > 1 {
                GlassSegmentedControl(
                    options: agentOptions,
                    selection: agent,
                    label: { agentSegmentLabel($0) },
                    icon: { agentSegmentIcon($0) },
                    onSelect: { onAgentChange($0) }
                )
                .accessibilityLabel("Agent")
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
            }
            GlassPickerRow(
                "Model",
                selection: $model,
                options: modelOptions,
                label: { modelRowLabel($0) },
                enabled: pinnedAgent != nil
            )
            GlassPickerRow(
                effortTitle,
                selection: $effort,
                options: effortOptions,
                label: { effortRowLabel($0) },
                enabled: effortEnabled
            )
        }
        .listRowBackground(glassFormRowFill)
    }

    // MARK: - Toggles (launch only)

    /// One footer-less toggle section (EXP-208 — no helper notices, like the
    /// IDE). Ultracode is claude-only, plan mode is claude+pi (EXP-441), skip
    /// permissions doesn't exist for pi.
    @ViewBuilder
    private var togglesSection: some View {
        Section {
            if let resumeRow {
                Toggle("Resume previous session", isOn: resumeRow.isOn)
            }
            if let ultracode, agent == "claude" {
                Toggle("Ultracode", isOn: ultracode)
            }
            // A resume never re-enters plan mode (the machine clamps it too) —
            // hide the toggle while one is active.
            if let planMode, LaunchVocabulary.supportsPlanMode(agent),
               resumeRow?.active != true {
                Toggle("Plan mode", isOn: planMode)
            }
            if let skipPermissions, agent != "pi" {
                Toggle("Skip permissions", isOn: skipPermissions)
            }
        } footer: {
            if let resumeRow, resumeRow.active {
                Text("A worktree for \(resumeRow.identifier ?? "this issue") already exists (\(resumeRow.branch)).")
            }
        }
        .listRowBackground(glassFormRowFill)
    }
}
