import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the launch options as ONE muted inline line under the composer
/// card (Danny's variant B): Device, Agent, Account (EXP-862: only where the
/// machine holds more than one login for the agent), Model, a Plan switch, the
/// Resume switch inline while a worktree makes it offerable (EXP-481), the
/// Repository pick only while there is no subject (a chat's optional anchor,
/// EXP-739), and a `⋯` pill for the rest (`AgentOptionsSheet`: Effort,
/// Ultracode). Every pill is a menu or a toggle — no disabled controls, the
/// footer under the row explains what cannot start.
struct AgentOptionsRow: View {
    let model: AgentComposerModel

    @State private var showsMore = false

    private var launch: LaunchOptionsState { model.launch }

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                devicePill
                agentPill
                accountPill
                modelPill
                planPill
                resumePill
                repositoryPill
                morePill
            }
            .padding(.horizontal, 2)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-options-row")
    }

    // MARK: - Pills

    /// The machine the run lands on — always named once one resolves (the
    /// desktop and web say it too); a menu only while there is a choice, a
    /// lone machine reads as a plain label like a lone agent does.
    @ViewBuilder
    private var devicePill: some View {
        if let device = model.device {
            if model.candidateDevices.count > 1 {
                GlassMenu {
                    ForEach(model.candidateDevices) { candidate in
                        // EXP-862: every picker menu whose selected value shows
                        // an icon shows it on the items too — here the machine's
                        // own kind glyph, exactly as the trigger draws it.
                        GlassMenuItem(
                            LaunchVocabulary.deviceCaption(candidate),
                            icon: candidate.isServer ? AppIcons.uiServer : AppIcons.uiDevice
                        ) {
                            model.selectDevice(candidate.deviceId)
                        }
                    }
                } label: {
                    OptionPillLabel(
                        icon: device.isServer ? AppIcons.uiServer : AppIcons.uiDevice,
                        text: LaunchVocabulary.deviceName(device)
                    )
                }
                .accessibilityLabel("Device")
                .accessibilityIdentifier("agent-device-pill")
            } else {
                OptionPillLabel(
                    icon: device.isServer ? AppIcons.uiServer : AppIcons.uiDevice,
                    text: LaunchVocabulary.deviceName(device),
                    chevron: false
                )
                .accessibilityLabel("Device")
                .accessibilityIdentifier("agent-device-pill")
            }
        }
    }

    /// The agent — the SHARED picker (EXP-862 `AgentPickerMenu`, ×4): an
    /// icon-only trigger (brand mark + chevron) whose menu rows carry that same
    /// mark beside the agent's name. EXP-642: the store slide's pop-out rect
    /// used to be measured off the segmented strip this replaced, so the
    /// identifier stays on this control.
    @ViewBuilder
    private var agentPill: some View {
        if model.availableAgents.count > 1 {
            AgentPickerMenu(
                agents: model.availableAgents,
                selection: launch.agent,
                label: { LaunchVocabulary.agentLabel($0) },
                mark: { AgentBrandMark.image($0) },
                onSelect: { model.selectAgent($0) }
            )
            .accessibilityIdentifier("start-coding-agent-picker")
        } else {
            AgentPickerTriggerLabel(mark: AgentBrandMark.image(launch.agent))
                .accessibilityLabel(LaunchVocabulary.agentLabel(launch.agent))
                .accessibilityIdentifier("start-coding-agent-picker")
        }
    }

    /// EXP-862: the ACCOUNT the run launches under, promoted out of the `⋯`
    /// sheet into the row — but only where it is a choice: the picked machine
    /// has to report two or more logins for the picked agent (×4 rule). The
    /// label is the login itself (email, else its profile label).
    @ViewBuilder
    private var accountPill: some View {
        let profiles = launch.accountProfiles(on: model.device)
        if profiles.count >= 2 {
            @Bindable var launch = model.launch
            GlassMenu {
                GlassMenuItem("Active login") { launch.account = "" }
                ForEach(profiles, id: \.id) { profile in
                    GlassMenuItem(accountLabel(profile)) { launch.account = profile.id }
                }
            } label: {
                OptionPillLabel(
                    icon: AppIcons.navAccount,
                    text: profiles.first { $0.id == launch.account }
                        .map(accountLabel) ?? "Active login"
                )
            }
            .accessibilityLabel("Account")
            .accessibilityIdentifier("agent-account-pill")
        }
    }

    /// One login's name. EXP-849: a login the agent REFUSED is still a login
    /// the machine holds, so it stays on offer — but it has to say so, or the
    /// run starts and dies on an expired credential.
    private func accountLabel(_ profile: AgentAccountProfile) -> String {
        let name = profile.email ?? profile.label ?? profile.id
        let health = AgentAccountHealth.of(profile)
        guard let badge = health.badgeLabel else { return name }
        return "\(name) · \(badge.lowercased())"
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

    /// The chat's OPTIONAL repository (EXP-615/739): with one the run gets
    /// its own `exp/chat-<id8>` worktree, without one it runs in the agent's
    /// scratch dir. Only while there is no subject — an issue brings its
    /// board's repo, an action its own.
    @ViewBuilder
    private var repositoryPill: some View {
        if model.subject == .none {
            if model.repos.isEmpty {
                OptionPillLabel(icon: AppIcons.actionRepository, text: "No repository", chevron: false)
            } else {
                GlassMenu {
                    GlassMenuItem("No repository") { model.chatRepoId = "" }
                    ForEach(model.repos) { repo in
                        GlassMenuItem(repo.fullName) { model.chatRepoId = repo.id }
                    }
                } label: {
                    OptionPillLabel(
                        icon: AppIcons.actionRepository,
                        text: model.repos.first { $0.id == model.chatRepoId }?.fullName ?? "No repository"
                    )
                }
                .accessibilityLabel("Repository")
            }
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
/// A label, not a button: `GlassMenu` wraps it in its own trigger, so a
/// `GlassPill` (a `Button` itself) would nest two. The agent's brand-marked
/// trigger is `AgentPickerTriggerLabel` (ExpUI) since EXP-862.
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
/// (Reasoning / Thinking per agent) and Ultracode (claude only — it IS
/// `--effort ultracode`, so it disables the Effort row). EXP-862 promoted the
/// Account out of here into the row itself. No MCP-server picker: mobile has
/// none.
struct AgentOptionsSheet: View {
    let model: AgentComposerModel

    var body: some View {
        @Bindable var launch = model.launch
        GlassSheetChrome(title: "Options") {
            VStack(spacing: 2) {
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
                .glassRow()

                if launch.agent == "claude" {
                    Toggle("Ultracode", isOn: $launch.ultracode)
                        .tint(DesignTokens.Palette.primary)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .glassRow()
                }

            }
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
