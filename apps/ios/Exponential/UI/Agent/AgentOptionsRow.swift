import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the launch options as ONE muted inline line under the composer
/// card (Danny's variant B): Device, Account, Model, a Plan switch, the Resume
/// switch inline while a worktree makes it offerable (EXP-481), and a `⋯` pill
/// for the rest (`AgentOptionsSheet`: Effort, Subagent model, Ultracode).
/// Every pill is a picker or a toggle — no disabled controls, the footer under
/// the row explains what cannot start.
///
/// EXP-872 folded the Agent pill INTO the Account one: the list is every login
/// the picked machine reports across agents, and picking one implies its agent.
/// EXP-993 dropped the Repository pill: a phone never picks a chat's repo — the
/// team's first repository is the anchor.
struct AgentOptionsRow: View {
    let model: AgentComposerModel

    @State private var showsMore = false

    private var launch: LaunchOptionsState { model.launch }

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                devicePill
                accountPill
                modelPill
                planPill
                resumePill
                morePill
            }
            .padding(.horizontal, 2)
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
    /// EXP-1030: `AccountPickerMenu` is the trigger + the lone-login rule over
    /// the SHARED `AccountPicker` (`ExpUI`) — the pill opens the one picker
    /// sheet now, brand mark + email per row with the EXP-992 limit bars under
    /// each, instead of a menu of its own.
    ///
    /// EXP-642: the store slide's pop-out rect is measured off the agent
    /// control, so the identifier stays on this one.
    private var accountPill: some View {
        let options = launch.accountOptions(on: model.device)
        return AccountPickerMenu(
            options: options,
            selection: launch.selectedAccount(in: options),
            mark: { AgentBrandMark.image($0) },
            onSelect: { model.selectAccount($0) }
        )
        .accessibilityLabel("Account")
        .accessibilityIdentifier("start-coding-agent-picker")
    }

    private var modelPill: some View {
        GlassMenu {
            ForEach(LaunchVocabulary.modelValues(for: launch.agent), id: \.self) { value in
                GlassMenuItem(LaunchVocabulary.modelLabel(value)) {
                    launch.model = value
                }
            }
        } label: {
            OptionPillLabel(text: LaunchVocabulary.modelLabel(launch.model))
        }
        .accessibilityLabel("Model")
    }

    /// Plan mode is claude's (EXP-441/EXP-849); a resume never re-enters plan
    /// mode (the machine clamps it too), so the switch hides while one is on.
    /// EXP-827: a slide switch on every platform (web and desktop use one),
    /// not a lit select pill. EXP-859 rebuilt it without `.fixedSize()` and
    /// without scaling the app-wide toggle down: a scaled switch reported its
    /// UNSCALED size, which pushed the caption out of the pill and clipped the
    /// track. One caption, a plain toggle, both inside the capsule.
    @ViewBuilder
    private var planPill: some View {
        if LaunchVocabulary.supportsPlanMode(launch.agent), !model.resumeActive {
            @Bindable var launch = model.launch
            Toggle(isOn: $launch.planMode) {
                Text("Plan")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
            .toggleStyle(OptionPillToggleStyle())
            .accessibilityLabel("Plan mode")
        }
    }

    /// EXP-481: offered only while the picked machine's synced worktree
    /// inventory carries a row for the ONE checked issue.
    @ViewBuilder
    private var resumePill: some View {
        if model.resumeCandidate != nil {
            GlassPill(
                "Resume",
                mode: .select(isSelected: launch.resume) { launch.resume.toggle() }
            )
            .accessibilityLabel("Resume previous session")
        }
    }

    private var morePill: some View {
        Button {
            showsMore = true
        } label: {
            OptionPillLabel(icon: AppIcons.uiMore, text: "", chevron: false)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("More options")
        .sheet(isPresented: $showsMore) {
            AgentOptionsSheet(model: model)
        }
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

/// EXP-825: the `⋯` sheet — the options that did not earn a pill: Effort
/// (Reasoning / Thinking per agent), the Subagent model (EXP-981, claude only)
/// and Ultracode (claude only — it IS `--effort ultracode`, so it disables the
/// Effort row). EXP-862 promoted the Account out of here into the row itself.
/// No MCP-server picker: mobile has none.
///
/// The Subagent model sits here rather than on the pill row: the row already
/// carries the pills a phone can hold, and this is the same place its sibling
/// Effort lives.
///
/// EXP-994: ONE grouped card, rows separated by `GlassDivider` hairlines —
/// the Settings idiom. It used to be a 2pt-gapped stack of individually
/// bordered rows, the shape every grouped list on this client stopped using.
struct AgentOptionsSheet: View {
    let model: AgentComposerModel

    var body: some View {
        @Bindable var launch = model.launch
        GlassSheetChrome(title: "Options") {
            VStack(spacing: 0) {
                GlassPickerRow(
                    LaunchVocabulary.effortTitle(for: launch.agent),
                    selection: $launch.effort,
                    options: [LaunchVocabulary.cliDefault] + LaunchVocabulary.effortValues(for: launch.agent),
                    label: { value in
                        value == LaunchVocabulary.cliDefault
                            ? "CLI default"
                            : LaunchVocabulary.effortLabel(value)
                    },
                    enabled: !(launch.agent == "claude" && launch.ultracode)
                )
                .padding(.horizontal, 12)
                .padding(.vertical, 12)

                // EXP-981: the model this run's SUBAGENTS get. Claude-only,
                // hidden for every other agent exactly like Ultracode.
                if LaunchVocabulary.supportsSubagentModel(launch.agent) {
                    GlassDivider()
                    GlassPickerRow(
                        "Subagent model",
                        selection: $launch.subagentModel,
                        options: LaunchVocabulary.subagentModelValues(),
                        label: { LaunchVocabulary.subagentModelLabel($0) }
                    )
                    .padding(.horizontal, 12)
                    .padding(.vertical, 12)
                    .accessibilityIdentifier("agent-subagent-model-row")
                }

                if launch.agent == "claude" {
                    GlassDivider()
                    Toggle("Ultracode", isOn: $launch.ultracode)
                        .tint(DesignTokens.Palette.primary)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                }
            }
            .glassSection()
            .padding(.horizontal, GlassSheetTokens.headerHPadding)
            .padding(.bottom, 16)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-options-sheet")
    }
}

/// EXP-859: the options row's switch — the pill IS the toggle. The app-wide
/// glass switch is UISwitch-sized (51×31) and too tall for the 28pt row, and
/// scaling it down left SwiftUI laying out the unscaled size, which is what
/// clipped the track and shoved the caption out of the capsule. This draws the
/// track at the row's own scale instead, so nothing is transformed.
struct OptionPillToggleStyle: ToggleStyle {
    @Environment(\.motion) private var motion

    func makeBody(configuration: Configuration) -> some View {
        Button {
            withAnimation(motion.fast) { configuration.isOn.toggle() }
        } label: {
            HStack(spacing: 6) {
                configuration.label
                track(isOn: configuration.isOn)
            }
            .padding(.leading, 10)
            .padding(.trailing, 5)
            .frame(height: 28)
            .background(GlassTokens.fillRow, in: Capsule())
            .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(.isToggle)
        .accessibilityValue(configuration.isOn ? "On" : "Off")
    }

    /// The glass switch at row scale: same colours and the same 2pt thumb
    /// inset, 34×20 instead of 51×31.
    private func track(isOn: Bool) -> some View {
        Capsule()
            .fill(isOn ? DesignTokens.Palette.primary : GlassTokens.fillCard)
            .overlay(
                Capsule().stroke(
                    isOn ? Color.clear : GlassTokens.strokeCard,
                    lineWidth: GlassTokens.hairline
                )
            )
            .overlay(alignment: isOn ? .trailing : .leading) {
                Circle()
                    .fill(
                        isOn
                            ? DesignTokens.Palette.primaryForeground
                            : DesignTokens.Palette.mutedForeground
                    )
                    .frame(width: 16, height: 16)
                    .padding(2)
            }
            .frame(width: 34, height: 20)
    }
}
