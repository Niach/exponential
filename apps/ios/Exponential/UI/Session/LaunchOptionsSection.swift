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
    /// The pinned agent — always a concrete one, seeded by the caller from the
    /// resolved machine's own default (EXP-615).
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

    @ViewBuilder
    private var optionsSection: some View {
        // ONE segmented capsule with the brand mark per agent (web parity,
        // EXP-615 — the loose pill strip that used to ride the Model header
        // is gone). A lone option is not a choice, on either variant. The
        // capsule lives in its OWN card-less section: sharing the Model/Effort
        // section painted that card behind it and clipped its bottom edge, and
        // zero row insets keep it flush with the grouped cards' margins.
        if availableAgents.count > 1 {
            Section {
                // No container accessibility label: it would merge the
                // segment buttons into one VoiceOver element.
                GlassSegmentedControl(
                    options: availableAgents,
                    selection: agent,
                    label: { LaunchVocabulary.agentLabel($0) },
                    icon: { Image("agent-\($0)") },
                    onSelect: { onAgentChange($0) }
                )
                // EXP-642: the store slide's pop-out rect is measured off this
                // capsule (`PopRects`), so it needs a stable handle. `contain`
                // keeps the individual segments queryable.
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("start-coding-agent-picker")
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
                .listRowSeparator(.hidden)
            }
        }
        Section {
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
