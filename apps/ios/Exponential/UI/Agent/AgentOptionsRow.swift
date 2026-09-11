import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the launch options as ONE muted inline line under the composer
/// card (Danny's variant B): Device, Agent, Model, a Plan switch, the Resume
/// switch inline while a worktree makes it offerable (EXP-481), the
/// Repository pick only while there is no subject (a chat's optional anchor,
/// EXP-739), and a `⋯` pill for the rest (`AgentOptionsSheet`: Effort,
/// Ultracode, Account). Every pill is a menu or a toggle — no disabled
/// controls, the footer under the row explains what cannot start.
struct AgentOptionsRow: View {
    let model: AgentComposerModel

    @State private var showsMore = false

    private var launch: LaunchOptionsState { model.launch }

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                devicePill
                agentPill
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

    /// The machine — only when there is a choice (a lone machine is not one).
    @ViewBuilder
    private var devicePill: some View {
        if model.candidateDevices.count > 1 {
            GlassMenu {
                ForEach(model.candidateDevices) { device in
                    GlassMenuItem(LaunchVocabulary.deviceCaption(device)) {
                        model.selectDevice(device.deviceId)
                    }
                }
            } label: {
                OptionPillLabel(
                    icon: model.device?.isServer == true ? AppIcons.uiServer : AppIcons.uiDevice,
                    text: model.device.map(LaunchVocabulary.deviceName) ?? "Device"
                )
            }
            .accessibilityLabel("Device")
        }
    }

    /// The agent — brand-marked like the segmented strip it replaces. EXP-642:
    /// the store slide's pop-out rect used to be measured off that strip, so
    /// the identifier stays on this pill.
    @ViewBuilder
    private var agentPill: some View {
        if model.availableAgents.count > 1 {
            GlassMenu {
                ForEach(model.availableAgents, id: \.self) { agent in
                    GlassMenuItem(LaunchVocabulary.agentLabel(agent)) {
                        model.selectAgent(agent)
                    }
                }
            } label: {
                OptionPillLabel(
                    brand: launch.agent,
                    text: LaunchVocabulary.agentLabel(launch.agent)
                )
            }
            .accessibilityLabel("Agent")
            .accessibilityIdentifier("start-coding-agent-picker")
        } else {
            OptionPillLabel(
                brand: launch.agent,
                text: LaunchVocabulary.agentLabel(launch.agent),
                chevron: false
            )
            .accessibilityIdentifier("start-coding-agent-picker")
        }
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

    /// Plan mode is claude + pi (EXP-441); a resume never re-enters plan
    /// mode (the machine clamps it too), so the switch hides while one is on.
    /// EXP-827: a slide switch on every platform (web and desktop use one),
    /// not a lit select pill. The app-wide glass toggle is UISwitch-sized, so
    /// it scales down to sit in the 28pt row; the caption toggles it too.
    @ViewBuilder
    private var planPill: some View {
        if LaunchVocabulary.supportsPlanMode(launch.agent), !model.resumeActive {
            @Bindable var launch = model.launch
            HStack(spacing: 6) {
                Text("Plan")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                Toggle("Plan", isOn: $launch.planMode)
                    .labelsHidden()
                    .fixedSize()
                    .scaleEffect(0.7)
                    .frame(width: 36, height: 22)
                    .accessibilityLabel("Plan mode")
            }
            .padding(.leading, 10)
            .padding(.trailing, 5)
            .frame(height: 28)
            .background(GlassTokens.fillRow, in: Capsule())
            .overlay(Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
            .contentShape(Capsule())
            .onTapGesture { launch.planMode.toggle() }
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

/// One option pill's LABEL — a glyph (or an agent brand mark), a value and a
/// chevron, on the row fill. A label, not a button: `GlassMenu` wraps it in
/// its own trigger, so a `GlassPill` (a `Button` itself) would nest two.
struct OptionPillLabel: View {
    var icon: String? = nil
    var brand: String? = nil
    let text: String
    var chevron: Bool = true

    var body: some View {
        HStack(spacing: 5) {
            if let brand {
                Image("agent-\(brand)")
                    .resizable()
                    .scaledToFit()
                    .frame(width: 13, height: 13)
            } else if let icon {
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
/// (Reasoning / Thinking per agent), Ultracode (claude only — it IS
/// `--effort ultracode`, so it disables the Effort row) and, when the picked
/// machine reports two or more login profiles for the agent, the Account to
/// launch under (EXP-792). No MCP-server picker: mobile has none.
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

                let profiles = launch.accountProfiles(on: model.device)
                if profiles.count >= 2 {
                    GlassPickerRow(
                        "Account",
                        selection: $launch.account,
                        options: [""] + profiles.map(\.id),
                        label: { id in
                            guard !id.isEmpty else { return "Active login" }
                            let profile = profiles.first { $0.id == id }
                            return profile?.email ?? id
                        }
                    )
                    .padding(.horizontal, 12)
                    .padding(.vertical, 12)
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
