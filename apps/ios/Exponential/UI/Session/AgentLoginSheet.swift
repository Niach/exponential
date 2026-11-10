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
///
/// WHERE the login lands is the machine's call, by EMAIL: the profile already
/// holding the address the person signed in as is refreshed (and only it), a
/// new address becomes a new profile. Signing in as somebody the machine
/// already has raises the `alreadyAdded` warning toast.
struct AgentLoginTarget: Identifiable {
    let deviceId: String
    let deviceLabel: String
    let agent: String
    /// The login whose Sign in was tapped (the INTENDED profile); nil = "Add
    /// account". It only decides whether the landing is a duplicate.
    var profileId: String? = nil
    /// EXP-1626 PERSIST LOGIN: "Persist login" / "Renew persisted login" on
    /// an EXISTING signed-in claude login (`profileId` set) — the queued
    /// `agent_login` mints a one-year `claude setup-token` login attached to
    /// that profile. Every other sign-in (Add account, Sign in) is the
    /// agent's normal browser login and leaves this false.
    var longLived = false

    var id: String {
        "\(deviceId):\(agent):\(profileId ?? "new")\(longLived ? ":persist" : "")"
    }
}

/// EXP-1626: the sheet's in-flight login, held by REFERENCE so the polling
/// task and the dismissal agree on it after the view itself is gone.
@MainActor
private final class LoginFlight {
    /// The queued `agent_login` row, once the server answered.
    var commandId: String?
    /// That row came back failed (or was never created): nothing to cancel.
    var failed = false
    /// The login landed: closing is the success path, not a walk-away.
    var landed = false
    /// The sheet is gone: polls stop, and a row created after that is
    /// cancelled at once.
    var closed = false
}

struct AgentLoginSheet: View {
    let viewModel: AgentsViewModel
    let target: AgentLoginTarget
    /// What "the login landed" means for whoever opened the sheet. Nil (every
    /// chip menu) = just close this sheet; the "+ Add account" flow closes its
    /// own sheet as well, so a finished sign-in leaves nothing behind.
    var onSignedIn: (() -> Void)? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @Environment(\.toaster) private var toaster

    /// The queued `agent_login` command id, while one is in flight.
    @State private var pendingLogin = false
    /// The sign-in payload the finished command carried (the machine completes
    /// it EARLY, the moment the link is on its own screen).
    @State private var loginResult: String?
    /// EXP-765: the code typed back from the browser, and what the machine said
    /// about it.
    @State private var codeDraft = ""
    @State private var pendingCode = false
    @State private var codeResult: String?
    @State private var started = false
    /// Every profile's `lastLoginAt` when the command was queued — what a
    /// landing is measured against (`AgentAccountsRows.loginLanding`).
    @State private var baseline: [String: String]?
    /// EXP-1626: what a dismissal needs to know to cancel the login.
    @State private var flight = LoginFlight()

    var body: some View {
        GlassSheetChrome(title: "Sign in") {
            VStack(alignment: .leading, spacing: 12) {
                statusLine
                persistNote
                link
                if let codeResult {
                    Text(codeResult)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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
        // EXP-1626: closed before the login landed (and unless the machine
        // already failed it) = `agent_login_cancel`: the machine stops that
        // run instead of leaving its sign-in waiting for 10 minutes.
        .onDisappear { sheetClosed() }
        // Closes itself on success: the machine stamps `lastLoginAt` on the
        // profile the login COMMITTED into and its next heartbeat reports it
        // (`AgentAccountsRows.loginLanding`). Landing on a login the machine
        // already had (not the one tapped) says so.
        .onChange(of: landing) { was, now in
            guard was == nil, let now else { return }
            if now.duplicate {
                toaster.warning(AgentAccountsRows.alreadyAddedToast(now))
            }
            flight.landed = true
            finish()
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

    /// EXP-1626: the persist sign-in's ONE muted line (`copy.persistNote`);
    /// the browser login carries no mode line and no mode button.
    @ViewBuilder
    private var persistNote: some View {
        if target.longLived {
            Text(DevicesList.Copy.persistNote)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("agent-login-persist-note")
        }
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
                                .font(.expMono(.caption))
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
                .font(.expMono(.caption))
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

    /// The machine's live report for this agent.
    private var liveAccount: AgentAccount? {
        viewModel.devices?.first { $0.deviceId == target.deviceId }?.agentAccounts?[target.agent]
    }

    /// Where the login landed, once it has — the shared rule, measured
    /// against the baseline captured when the command was queued.
    private var landing: AgentAccountsRows.LoginLanding? {
        guard let baseline else { return nil }
        return AgentAccountsRows.loginLanding(
            account: liveAccount,
            baseline: baseline,
            intendedProfileId: target.profileId
        )
    }

    /// The login landed: hand back to whoever opened the sheet, else close.
    private func finish() {
        if let onSignedIn {
            onSignedIn()
        } else {
            dismiss()
        }
    }

    private func queueLogin() {
        loginResult = nil
        codeResult = nil
        codeDraft = ""
        pendingLogin = true
        baseline = AgentAccountsRows.loginBaseline(liveAccount)
        flight.commandId = nil
        flight.failed = false
        let queued = target
        Task {
            let outcome = await run(
                kind: "agent_login",
                profileId: queued.profileId,
                longLived: queued.longLived
            )
            pendingLogin = false
            if case .failed = outcome { flight.failed = true }
            switch outcome {
            case let .done(result): loginResult = result
            // FEED-79: a refusal is the error toast, never a red line.
            case let .failed(message):
                toaster.error(
                    FeedbackStates.Errors.title(verb: "start", thing: "the sign-in"),
                    description: message
                )
            case .unanswered: break
            }
        }
    }

    private func submitCode() {
        let code = codeDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty, !pendingCode else { return }
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
            // EXP-1626: a refused code leaves the link (and the field) on
            // screen for another try; the failure is the error toast.
            case let .failed(message):
                toaster.error(
                    FeedbackStates.Errors.title(verb: "send", thing: "the code"),
                    description: message
                )
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

    /// EXP-1626: how long a sign-in's poll watches its row — a person
    /// reads the link, signs in in a browser and pastes the code back, so it
    /// matches the machine's own 10-minute limit on an unfinished run.
    private static let pollInterval: Duration = .seconds(2)
    private static let pollAttempts = 300

    /// Queue the command, then poll its row ~2s until terminal (bounded).
    private func run(
        kind: String,
        code: String? = nil,
        profileId: String? = nil,
        longLived: Bool = false
    ) async -> CommandOutcome {
        let flight = flight
        do {
            let created = try await deps.devicesApi.createCommand(
                accountId: accountId,
                deviceId: target.deviceId,
                kind: kind,
                agent: target.agent,
                code: code,
                profileId: profileId,
                longLived: longLived ? true : nil
            )
            if kind == "agent_login" {
                flight.commandId = created.id
                // Closed while the row was being created: cancel it now.
                if flight.closed {
                    cancelLogin(created.id)
                    return .unanswered
                }
            }
            for _ in 0..<Self.pollAttempts {
                try? await Task.sleep(for: Self.pollInterval)
                if Task.isCancelled || flight.closed { return .unanswered }
                guard let command = try? await deps.devicesApi.getCommand(
                    accountId: accountId, commandId: created.id
                ) else { continue }
                guard !command.isPending else { continue }
                return command.isFailed
                    ? .failed(command.result ?? "The device refused the command.")
                    : .done(command.result)
            }
            return .unanswered
        } catch {
            return .failed(error.userFacingMessage)
        }
    }

    /// EXP-1626: the sheet went away. Unless the login landed, failed or was
    /// never created, ask the machine to stop it.
    private func sheetClosed() {
        flight.closed = true
        guard !flight.landed, !flight.failed, let id = flight.commandId else { return }
        cancelLogin(id)
    }

    /// EXP-1626: `agent_login_cancel {agent, loginCommandId}`, fire and
    /// forget — the sheet is closing, so nobody is left to show an error,
    /// and a run nobody finishes stops itself after 10 minutes anyway.
    private func cancelLogin(_ loginCommandId: String) {
        let api = deps.devicesApi
        let accountId = accountId
        let deviceId = target.deviceId
        let agent = target.agent
        Task.detached {
            _ = try? await api.createCommand(
                accountId: accountId,
                deviceId: deviceId,
                kind: DevicesApi.agentLoginCancelKind,
                agent: agent,
                loginCommandId: loginCommandId
            )
        }
    }
}

/// EXP-862/EXP-909: "Add account" — the mobile twin of web's
/// `AddAccountDialog`, now DEVICE-BOUND: it is opened from the row under ONE
/// machine, so the machine is decided before the sheet exists and its picker
/// is gone. Pick the agent, and the login runs there: the machine signs in
/// fresh and lands it by email (a new address = a new profile, a known one
/// refreshes that profile and warns).
struct AddAccountSheet: View {
    let viewModel: AgentsViewModel
    /// The machine the sign-in lands on — the row that opened this sheet only
    /// renders on one that can actually take it.
    let device: SteerDevice

    @Environment(\.dismiss) private var dismiss

    @State private var agent = ""
    @State private var loginTarget: AgentLoginTarget?

    /// Every agent INSTALLED on the machine — runnable or signed out; either
    /// can take a login.
    private var agents: [String] {
        let set = Set(device.agentIds).union(device.unauthedAgentIds)
        return DomainContract.codingAgentValues.filter { set.contains($0) }
    }

    private var resolvedAgent: String {
        agents.contains(agent) ? agent : (agents.first ?? "")
    }

    var body: some View {
        GlassSheetChrome(
            title: "Add account",
            content: {
                // EXP-994: ONE grouped card with a hairline between its rows,
                // the Settings idiom — not a gapped stack of bordered rows.
                VStack(spacing: 0) {
                    // The machine is named, not picked: this sheet is opened
                    // from its own row.
                    HStack(spacing: 8) {
                        Text("Device")
                            .foregroundStyle(.white.opacity(TextOpacity.primary))
                        Spacer(minLength: 8)
                        Text(LaunchVocabulary.deviceCaption(device))
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 12)

                    GlassDivider()

                    // EXP-862: the SHARED agent picker (brand mark + chevron
                    // over marked menu rows). The ONE surface that still picks
                    // an AGENT rather than an account (EXP-872): this sheet
                    // creates the login an account option is made of.
                    HStack(spacing: 8) {
                        Text("Agent")
                            .foregroundStyle(.white.opacity(TextOpacity.primary))
                        Spacer(minLength: 8)
                        AgentPickerMenu(
                            agents: agents,
                            selection: resolvedAgent,
                            label: { LaunchVocabulary.agentLabel($0) },
                            mark: { AgentBrandMark.image($0) },
                            onSelect: { agent = $0 }
                        )
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 12)
                }
                .glassSection()
                .padding(.horizontal, GlassSheetTokens.headerHPadding)
                .padding(.bottom, 16)
            },
            primaryAction: {
                GlassSubmitButton("Sign in", enabled: !resolvedAgent.isEmpty) {
                    // No intended profile: "Add account" names none, and it is
                    // the agent's normal browser login (EXP-1626).
                    loginTarget = AgentLoginTarget(
                        deviceId: device.deviceId,
                        deviceLabel: LaunchVocabulary.deviceName(device),
                        agent: resolvedAgent
                    )
                }
            }
        )
        .accessibilityIdentifier("add-account-sheet")
        .sheet(item: $loginTarget) { target in
            // A landed login closes the whole flow, not just the login sheet:
            // web's dialog is gone by then too.
            AgentLoginSheet(
                viewModel: viewModel,
                target: target,
                onSignedIn: {
                    loginTarget = nil
                    dismiss()
                }
            )
        }
    }
}
