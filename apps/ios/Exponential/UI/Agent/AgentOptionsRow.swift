import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the launch options as ONE muted inline line under the composer
/// card (Danny's variant B): Device, Account, Model and a Plan switch.
/// EXP-1249: nothing else — the `⋯` pill is gone; Effort, Ultracode, MCP
/// servers and Computer use live in the composer's "+" menu
/// (`ComposerPlusMenu`). Every pill is a picker or a toggle — no disabled
/// controls, the footer under the row explains what cannot start.
///
/// EXP-872 folded the Agent pill INTO the Account one: the list is every login
/// the picked machine reports across agents, and picking one implies its agent.
/// EXP-993 dropped the Repository pill: a phone never picks a chat's repo — the
/// team's first repository is the anchor.
struct AgentOptionsRow: View {
    let model: AgentComposerModel

    private var launch: LaunchOptionsState { model.launch }

    var body: some View {
        // N-14: THE row host — inside a sheet a pin's rows disclose in the
        // slot under the whole row, so the pins never move.
        InlinePickerRow {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    devicePill
                    accountPill
                    modelPill
                }
                .padding(.horizontal, 2)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-options-row")
    }

    // MARK: - Pills

    /// The machine the run lands on — always named once one resolves (the
    /// desktop and web say it too); a picker only while there is a choice, a
    /// lone machine reads as a plain label like a lone agent does.
    @ViewBuilder
    private var devicePill: some View {
        if let device = model.device {
            if model.candidateDevices.count > 1 {
                // EXP-1030: the SHARED `DevicePicker`; the pill is only its
                // trigger. EXP-862 stands — a machine is recognised by its own
                // glyph (EXP-924: its owner's pick, else the kind default), and
                // the picker leads every row with it, exactly as the trigger
                // draws it. A shared machine names its owner under it. The
                // ONE row bridge (`DevicePickerDevice(SteerDevice)`) decides
                // all of that.
                DevicePicker(
                    devices: model.candidateDevices.map(DevicePickerDevice.init),
                    value: device.deviceId,
                    onChange: { model.selectDevice($0) },
                    trigger: {
                        OptionPillLabel(
                            icon: DeviceIconDisplay.iconName(for: device),
                            text: LaunchVocabulary.deviceName(device)
                        )
                    }
                )
                .accessibilityLabel("Device")
                .accessibilityIdentifier("agent-device-pill")
            } else {
                OptionPillLabel(
                    icon: DeviceIconDisplay.iconName(for: device),
                    text: LaunchVocabulary.deviceName(device),
                    chevron: false
                )
                .accessibilityLabel("Device")
                .accessibilityIdentifier("agent-device-pill")
            }
        }
    }

    /// EXP-872: the ONE account picker — the agent pill folded into it. Every
    /// signed-in login the picked machine reports, across agents, brand mark +
    /// email, the machine's default first; picking one picks its agent too.
    /// EXP-849: a login the agent REFUSED stays on offer wearing its badge, or
    /// a run starts and dies on an expired credential.
    ///
    /// EXP-1030: the SHARED `AccountPicker` (`ExpUI`) — the pill opens the one
    /// picker sheet, brand mark + email per row with the EXP-992 limit bars
    /// under each. A lone login is not a choice: the picker disables itself
    /// and the trigger drops its chevron.
    ///
    /// EXP-642: the store slide's pop-out rect is measured off the agent
    /// control, so the identifier stays on this one.
    private var accountPill: some View {
        let options = launch.accountOptions(on: model.device)
        // The trigger always names a login: the pick, else the first option
        // (the last used one).
        let current = launch.selectedAccount(in: options) ?? options.first
        return AccountPicker(
            options: options,
            // Keyed by `<agent>:<profileId>`: the ambient `system` login
            // repeats across agents, so a profile id alone is not one row.
            value: current?.key,
            onChange: { key in
                guard let picked = options.first(where: { $0.key == key }) else { return }
                model.selectAccount(picked)
            },
            mark: { AgentBrandMark.image($0) },
            trigger: {
                AccountPickerTriggerLabel(
                    option: current,
                    mark: current.flatMap { AgentBrandMark.image($0.agent) },
                    chevron: options.count > 1
                )
            }
        )
        .accessibilityLabel("Account")
        .accessibilityIdentifier("start-coding-agent-picker")
    }

    /// EXP-1345: a single-select pick = THE Picker (a bottom sheet, the
    /// picked row's check trailing), never an anchored menu.
    private var modelPill: some View {
        let state = model.launch
        return ModelPicker(
            agent: state.agent,
            value: state.model,
            onChange: { state.model = $0 },
            trigger: { OptionPillLabel(text: LaunchVocabulary.modelLabel(state.model)) }
        )
        .accessibilityLabel("Model")
    }
}

/// One option pill's LABEL — a glyph, a value and a chevron, on the row fill.
/// A label, not a button: `GlassMenu` and `GlassPicker` wrap it in a trigger
/// of their own, so a `GlassPill` (a `Button` itself) would nest two. The
/// agent's brand-marked trigger is `AgentPickerTriggerLabel` (ExpUI) since
/// EXP-862.
struct OptionPillLabel: View {
    var icon: String? = nil
    let text: String
    var chevron: Bool = true

    var body: some View {
        HStack(spacing: 5) {
            if let icon {
                AppIcon(icon, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            if !text.isEmpty {
                Text(text)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
            if chevron {
                AppIcon(AppIcons.uiChevronDown, size: 10)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .padding(.horizontal, text.isEmpty ? 8 : 10)
        .frame(height: 28)
        .background(GlassTokens.fillRow, in: Capsule())
        .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
        .contentShape(Capsule())
    }
}
