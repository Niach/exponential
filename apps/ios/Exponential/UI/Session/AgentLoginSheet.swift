import ExpCore
import ExpUI
import SwiftUI

/// EXP-862: the sign-in sheet — the ONE place an agent login is driven from a
/// phone, and the mobile twin of the web/desktop login dialog: a title, ONE
/// status line, the machine's own sign-in link, and (for Claude) the field that
/// hands the browser's code back. It closes itself the moment the machine
/// reports the login as signed in.
///
/// It used to live inside the device settings sheet, mixed into that agent's
/// tab beside the launch defaults. EXP-862 split the two jobs: device settings
/// is the machine's configuration, a login is a flow, and the chip menus (a
/// device row's, an account row's "+") open this directly.
///
/// No credential is ever held, copied or forwarded: the machine runs
/// `claude auth login` / `codex login` locally and publishes only the sign-in
/// link it puts on its own screen (EXP-484). EXP-765 closes the loop for
/// Claude, whose browser hands an authorization code back to a CLI still
/// waiting on the machine.
struct AgentLoginTarget: Identifiable {
    let deviceId: String
    let deviceLabel: String
    let agent: String
    /// The EXISTING login to sign in again, `system`/nil for the machine's
    /// ambient one.
    var profileId: String? = nil
    /// EXP-827: create a new profile with this label first and sign into that
    /// one ("+ Add account" on a machine whose ambient login is taken). Never
    /// together with `profileId` — the server refuses that pair.
    var newProfileLabel: String? = nil

    var id: String {
        "\(deviceId):\(agent):\(profileId ?? newProfileLabel ?? "active")"
    }
}

struct AgentLoginSheet: View {
    let viewModel: AgentsViewModel
    let target: AgentLoginTarget

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    /// The queued `agent_login` command id, while one is in flight.
    @State private var pendingLogin = false
    /// The sign-in payload the finished command carried (the machine completes
    /// it EARLY, the moment the link is on its own screen).
    @State private var loginResult: String?
    @State private var errorText: String?
    /// EXP-765: the code typed back from the browser, and what the machine said
    /// about it.
    @State private var codeDraft = ""
    @State private var pendingCode = false
    @State private var codeResult: String?
    @State private var started = false

    var body: some View {
        GlassSheetChrome(title: "Sign in") {
            VStack(alignment: .leading, spacing: 12) {
                statusLine
                link
                if let codeResult {
                    Text(codeResult)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let errorText {
                    Text(errorText)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-login-sheet")
        .onAppear {
            guard !started else { return }
            started = true
            queueLogin()
        }
        // Closes itself on success: the machine re-probes after the login and
        // its next heartbeat reports the profile as signed in.
        .onChange(of: signedIn) { was, now in
            if !was, now { dismiss() }
        }
    }

    /// The ONE status line (the pinned ×4 shape): who is signing in where, and
    /// what is happening right now — never a paragraph.
    private var statusLine: some View {
        HStack(spacing: 8) {
            if pendingLogin || pendingCode {
                ProgressView().controlSize(.small).tint(.white)
            }
            Text(statusText)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
    }

    private var statusText: String {
        let agent = LaunchVocabulary.agentLabel(target.agent)
        if pendingCode { return "Sending the code to \(target.deviceLabel)…" }
        if pendingLogin {
            return "Waiting for \(target.deviceLabel) to publish the \(agent) sign-in link…"
        }
        return "\(agent) on \(target.deviceLabel)"
    }

    /// The published link, its code, and Claude's way back.
    @ViewBuilder
    private var link: some View {
        if let result = loginResult {
            if let parsed = AgentUsagePresentation.parseAgentLoginResult(result),
               let url = URL(string: parsed.url) {
                let wantsCodeBack = parsed.code == nil
                VStack(alignment: .leading, spacing: 8) {
                    Link(destination: url) {
                        Label("Open the sign-in link", appIcon: AppIcons.uiExternalLink)
                    }
                    if let code = parsed.code {
                        HStack(spacing: 8) {
                            Text(code)
                                .font(.caption.monospaced())
                            Button {
                                Platform.copyToPasteboard(code)
                            } label: {
                                AppIcon(AppIcons.uiCopy, size: AppIcon.Size.small)
                                    .foregroundStyle(.secondary)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Copy code")
                        }
                    }
                    if wantsCodeBack {
                        codeEntry
                    }
                    Text(linkCaption(hasCode: parsed.code != nil, wantsCodeBack: wantsCodeBack))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            } else {
                // Not a link payload (an older build, a plain note): verbatim.
                Text(result)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// The caption under a published sign-in link, in the three shapes it
    /// takes: codex's code goes into the browser, Claude's comes back here,
    /// and a machine too old to take it back gets the bare instruction.
    private func linkCaption(hasCode: Bool, wantsCodeBack: Bool) -> String {
        if hasCode {
            return "Open the link on any device and enter the code on the machine."
        }
        return wantsCodeBack
            ? "Open the link on any device, then paste the code it shows here."
            : "Open the link on any device."
    }

    private var codeEntry: some View {
        let typed = codeDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        return HStack(spacing: 8) {
            GlassTextField("Code from the browser", text: $codeDraft)
                .font(.caption.monospaced())
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .onSubmit { submitCode() }
            GlassPill(
                "Enter code",
                icon: AppIcons.uiSignIn,
                mode: .action { submitCode() },
                enabled: !typed.isEmpty && !pendingCode
            )
        }
    }

    // MARK: - Commands

    /// The machine's live report for the login this sheet is signing in.
    private var signedIn: Bool {
        let device = viewModel.devices?.first { $0.deviceId == target.deviceId }
        guard let account = device?.agentAccounts?[target.agent] else { return false }
        guard let profileId = target.profileId,
              profileId != AgentAccountsRows.systemProfileId,
              let profiles = account.profiles
        else { return account.signedIn == true }
        return profiles.first { $0.id == profileId }?.signedIn == true
    }

    private func queueLogin() {
        errorText = nil
        loginResult = nil
        codeResult = nil
        codeDraft = ""
        pendingLogin = true
        Task {
            let outcome = await run(
                kind: "agent_login",
                profileId: target.newProfileLabel == nil ? target.profileId : nil,
                newProfileLabel: target.newProfileLabel
            )
            pendingLogin = false
            switch outcome {
            case let .done(result): loginResult = result
            case let .failed(message): errorText = message
            case .unanswered: break
            }
        }
    }

    private func submitCode() {
        let code = codeDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty, !pendingCode else { return }
        errorText = nil
        codeResult = nil
        pendingCode = true
        Task {
            let outcome = await run(kind: "agent_login_code", code: code)
            pendingCode = false
            switch outcome {
            case let .done(result):
                // The code is in: the link it answered is spent, so it goes and
                // the machine's own words take its place.
                loginResult = nil
                codeDraft = ""
                codeResult = result
            case let .failed(message): errorText = message
            case .unanswered: break
            }
        }
    }

    private enum CommandOutcome {
        case done(String?)
        case failed(String)
        /// An offline machine keeps the command queued server-side; the poll
        /// simply stops watching and the outcome lands via sync.
        case unanswered
    }

    /// Queue the command, then poll its row ~2s until terminal (bounded).
    private func run(
        kind: String,
        code: String? = nil,
        profileId: String? = nil,
        newProfileLabel: String? = nil
    ) async -> CommandOutcome {
        do {
            let created = try await deps.devicesApi.createCommand(
                accountId: accountId,
                deviceId: target.deviceId,
                kind: kind,
                agent: target.agent,
                code: code,
                profileId: profileId,
                newProfileLabel: newProfileLabel
            )
            for _ in 0..<60 {
                try? await Task.sleep(for: .seconds(2))
                if Task.isCancelled { return .unanswered }
                guard let command = try? await deps.devicesApi.getCommand(
                    accountId: accountId, commandId: created.id
                ) else { continue }
                guard !command.isPending else { continue }
                return command.isFailed
                    ? .failed(command.result ?? "The machine refused the command.")
                    : .done(command.result)
            }
            return .unanswered
        } catch {
            return .failed(error.userFacingMessage)
        }
    }
}

/// EXP-862: "+ Add account" — the Accounts header's pill, and the mobile twin
/// of web's `AddAccountDialog`: pick one of MY online machines that can take a
/// sign-in, pick the agent, and the login runs there. A machine whose ambient
/// login is still free takes it; otherwise the machine creates a NEW profile
/// first (EXP-792's per-agent config dirs) and signs into that one.
struct AddAccountSheet: View {
    let viewModel: AgentsViewModel

    @Environment(\.dismiss) private var dismiss

    @State private var deviceId = ""
    @State private var agent = ""
    @State private var loginTarget: AgentLoginTarget?

    /// The caller's machines a sign-in can be queued on right now: own, online,
    /// advertising `agent-login`, with at least one agent installed.
    private var candidates: [SteerDevice] {
        (viewModel.devices ?? []).filter {
            $0.isMine && $0.isOnline && $0.canAgentLogin && !addableAgents($0).isEmpty
        }
    }

    private var selected: SteerDevice? {
        candidates.first { $0.deviceId == deviceId } ?? candidates.first
    }

    /// Every agent INSTALLED on the machine — runnable or signed out; either
    /// can take a login.
    private func addableAgents(_ device: SteerDevice) -> [String] {
        let set = Set(device.agentIds).union(device.unauthedAgentIds)
        return DomainContract.codingAgentValues.filter { set.contains($0) }
    }

    private var agents: [String] {
        selected.map(addableAgents) ?? []
    }

    private var resolvedAgent: String {
        agents.contains(agent) ? agent : (agents.first ?? "")
    }

    var body: some View {
        GlassSheetChrome(
            title: "Add account",
            content: {
                VStack(spacing: 2) {
                    if candidates.isEmpty {
                        Text("None of your machines can take a sign-in right now. Start the Exponential desktop app or the daemon and try again.")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 12)
                            .padding(.vertical, 12)
                            .glassRow()
                    } else {
                        GlassPickerRow(
                            "Device",
                            selection: Binding(
                                get: { selected?.deviceId ?? "" },
                                set: { deviceId = $0 }
                            ),
                            options: candidates.map(\.deviceId),
                            label: { id in
                                candidates.first { $0.deviceId == id }
                                    .map(LaunchVocabulary.deviceCaption) ?? id
                            },
                            icon: { id in
                                candidates.first { $0.deviceId == id }?.isServer == true
                                    ? AppIcons.uiServer
                                    : AppIcons.uiDevice
                            }
                        )
                        .padding(.horizontal, 12)
                        .padding(.vertical, 12)
                        .glassRow()

                        GlassPickerRow(
                            "Agent",
                            selection: Binding(
                                get: { resolvedAgent },
                                set: { agent = $0 }
                            ),
                            options: agents,
                            label: { LaunchVocabulary.agentLabel($0) }
                        )
                        .padding(.horizontal, 12)
                        .padding(.vertical, 12)
                        .glassRow()
                    }
                }
                .padding(.horizontal, GlassSheetTokens.headerHPadding)
                .padding(.bottom, 16)
            },
            primaryAction: {
                GlassSubmitButton("Sign in", enabled: selected != nil && !resolvedAgent.isEmpty) {
                    guard let device = selected else { return }
                    loginTarget = AgentLoginTarget(
                        deviceId: device.deviceId,
                        deviceLabel: LaunchVocabulary.deviceName(device),
                        agent: resolvedAgent,
                        profileId: ambientProfileId(device, agent: resolvedAgent),
                        newProfileLabel: newProfileLabel(device, agent: resolvedAgent)
                    )
                }
            }
        )
        .accessibilityIdentifier("add-account-sheet")
        .sheet(item: $loginTarget) { target in
            AgentLoginSheet(viewModel: viewModel, target: target)
        }
    }

    /// Where a new login lands: the ambient login while it is signed out
    /// (nothing to keep beside it), otherwise a new profile.
    private func ambientSignedIn(_ device: SteerDevice, agent: String) -> Bool {
        guard let account = device.agentAccounts?[agent] else { return false }
        guard let ambient = account.profiles?.first(where: {
            $0.id == AgentAccountsRows.systemProfileId
        }) else { return account.signedIn == true }
        return ambient.signedIn == true
    }

    private func ambientProfileId(_ device: SteerDevice, agent: String) -> String? {
        ambientSignedIn(device, agent: agent) ? nil : AgentAccountsRows.systemProfileId
    }

    /// `Claude Code account 2` — one past the profiles the machine reports for
    /// the agent (the ambient login counts as the first). Clamped at the
    /// server's 64.
    private func newProfileLabel(_ device: SteerDevice, agent: String) -> String? {
        guard ambientSignedIn(device, agent: agent) else { return nil }
        let profiles = device.agentAccounts?[agent]?.profiles ?? []
        let count = max(profiles.count, 1)
        let label = "\(LaunchVocabulary.agentLabel(agent)) account \(count + 1)"
        return String(label.prefix(64))
    }
}
