import AuthenticationServices
import Combine
import ExpUI
import ExpCore
import SwiftUI

// Installed-repo picker (web github-repo-picker.tsx): lists the repos the user's
// GitHub App is installed on and returns the chosen one to the caller. v4: it no
// longer links a repo to a board directly — instead it feeds the create-board
// inline-connect path (`repository: { fullName }`). Handles not-configured /
// not-installed (in-app install flow + auto re-query) / installed (searchable
// list). The link/upsert happens server-side in `boards.create`.
//
// EXP-8: the install URL opens in an ASWebAuthenticationSession (mobile-width
// page, in-app) instead of kicking out to system Safari. The server's
// post-install page fires `exponential://github-connected`, which auto-dismisses
// the session; either way the completion re-queries with `refresh: true` so the
// newly connected repos appear without any manual step.
struct GithubRepoPicker: View {
    let accountId: String
    /// Scopes the repo query + connect hop to this team's linked GitHub
    /// accounts (per-team installation claiming).
    let teamId: String
    let integrationsApi: IntegrationsApi
    /// Called with the picked repo; the sheet dismisses itself afterwards.
    var onPick: (GithubPickerRepo) -> Void

    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @State private var result: GithubReposResult?
    @State private var loading = true
    @State private var query = ""
    @State private var error: String?
    @State private var installSession = InstallWebAuthSession()

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        content
                        if let error {
                            Text(error).font(.caption).foregroundStyle(.red)
                        }
                    }
                    .padding(16)
                }
            }
            .navigationTitle("Add repository")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button("Cancel") { dismiss() } }
            }
            .task { await load() }
            .onChange(of: scenePhase) { _, phase in
                // Self-heal after any trip through another app/browser (e.g. an
                // install finished externally); bypass the server cache so a
                // just-granted repo shows up.
                if phase == .active { Task { await load(refresh: true) } }
            }
            // The app-level deep-link path for `exponential://github-connected` — covers
            // an install that finishes outside the in-app auth session.
            .onReceive(NotificationCenter.default.publisher(for: .githubConnected)) { _ in
                Task { await load(refresh: true) }
            }
        }
    }

    @ViewBuilder private var content: some View {
        if loading && result == nil {
            HStack(spacing: 10) {
                ProgressView().controlSize(.small).tint(.white)
                Text("Loading your GitHub repositories…")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .padding(.vertical, 24)
        } else if let data = result, data.configured {
            if data.installed {
                installedList(data)
            } else {
                notInstalled(data)
            }
        } else {
            Text("GitHub isn't configured on this server, so repositories can't be connected.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    @ViewBuilder private func notInstalled(_ data: GithubReposResult) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Connect the Exponential GitHub App to pick a repository. You'll come right back here.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Button {
                openConnect(data)
            } label: {
                HStack(spacing: 6) {
                    AppIcon(AppIcons.uiGithub, size: AppIcon.Size.medium)
                    Text("Connect GitHub")
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            Button {
                Task { await load(refresh: true) }
            } label: {
                Text("I've connected — refresh").frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
        }
    }

    // Grant model: the list shows exactly the repos the user's last OAuth
    // connect proved access to — never the installation-wide selection. So a
    // repo created or shared since that connect only appears after re-running
    // the connect hop (`openConnect`), and a team linked before grants
    // existed (`needsReauth`) yields zero repos until someone reconnects.
    @ViewBuilder private func installedList(_ data: GithubReposResult) -> some View {
        let repos = data.repos.filter {
            query.isEmpty || $0.fullName.localizedCaseInsensitiveContains(query.trimmingCharacters(in: .whitespaces))
        }
        VStack(alignment: .leading, spacing: 8) {
            if data.repos.isEmpty {
                emptyState(data)
            } else {
                if data.installations.contains(where: { $0.isSuspended }) {
                    suspendedNotice(data)
                }
                if data.installations.contains(where: { $0.needsReauth && !$0.isSuspended }) {
                    reconnectNotice(data)
                }

                HStack(spacing: 8) {
                    AppIcon(AppIcons.navSearch, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    TextField("Search repositories…", text: $query)
                        .textFieldStyle(.plain)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .foregroundStyle(.white)
                }
                .padding(12)
                .background(Color.white.opacity(0.05))
                .clipShape(RoundedRectangle(cornerRadius: 8))

                if repos.isEmpty {
                    Text("No repositories found.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.vertical, 8)
                }

                ForEach(repos) { repo in
                    Button {
                        onPick(repo)
                        dismiss()
                    } label: {
                        HStack(spacing: 10) {
                            AppIcon(AppIcons.uiRepository, size: AppIcon.Size.small)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            Text(repo.fullName)
                                .font(.subheadline.monospaced())
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            Spacer()
                            if repo.`private` {
                                AppIcon(AppIcons.uiPrivate, size: 11)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .glassRow()
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    // Installed but zero repos — three DISTINCT states (EXP-365, web parity):
    // a suspended installation needs an UNSUSPEND on GitHub (a reconnect
    // cannot fix it — never nudge the wrong fix), a needs-reauth one needs the
    // OAuth reconnect, and an account that genuinely has no reachable repos
    // needs neither.
    @ViewBuilder private func emptyState(_ data: GithubReposResult) -> some View {
        let suspended = data.installations.filter { $0.isSuspended }
        let needsReauth = data.installations.contains { $0.needsReauth && !$0.isSuspended }
        if !suspended.isEmpty {
            suspendedNotice(data)
        } else if needsReauth {
            VStack(alignment: .leading, spacing: 12) {
                Text("Reconnect GitHub to load the repositories you can access\(reauthAccountSuffix(data)).")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Button {
                    openConnect(data)
                } label: {
                    HStack(spacing: 6) {
                        AppIcon(AppIcons.uiRefresh, size: AppIcon.Size.medium)
                        Text("Reconnect GitHub")
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
            }
        } else {
            Text("No repositories found for your connected GitHub accounts.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    // GitHub-side App suspension (REV2-29): the installation lists no repos
    // and mints no tokens until it's unsuspended on GitHub.
    @ViewBuilder private func suspendedNotice(_ data: GithubReposResult) -> some View {
        let names = data.installations
            .filter { $0.isSuspended }
            .map { $0.accountLogin ?? "a connected account" }
            .joined(separator: ", ")
        Text("GitHub suspended the Exponential app for \(names) — its repositories can't be connected until you unsuspend it on GitHub.")
            .font(.caption)
            .foregroundStyle(.red.opacity(0.8))
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
    }

    // " from a, b" when the stale accounts are known — names make the fix
    // actionable when several accounts are linked.
    private func reauthAccountSuffix(_ data: GithubReposResult, preposition: String = "from") -> String {
        let names = data.installations
            .filter { $0.needsReauth && !$0.isSuspended }
            .compactMap { $0.accountLogin }
        return names.isEmpty ? "" : " \(preposition) \(names.joined(separator: ", "))"
    }

    // A linked account whose grants were never captured — its repos are missing
    // from the (non-empty) list until the user reconnects.
    @ViewBuilder private func reconnectNotice(_ data: GithubReposResult) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Reconnect GitHub\(reauthAccountSuffix(data, preposition: "for")) to refresh — repos created or shared with you since your last connect won't appear until you do.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Button {
                openConnect(data)
            } label: {
                HStack(spacing: 6) {
                    AppIcon(AppIcons.uiRefresh, size: AppIcon.Size.medium)
                    Text("Reconnect GitHub")
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // Connect action: claim a GitHub account for this team. Prefer the
    // mobile-friendly OAuth `connectUrl` (single consent screen) and fall back
    // to the GitHub App install page when it's absent.
    private func openConnect(_ data: GithubReposResult) {
        openInBrowser(data.connectUrl ?? data.installUrl)
    }

    // Web parity (github-repo-picker.tsx): the old `/account/integrations`
    // fallback was removed in v5 (repo management lives in team settings →
    // Repositories). Opened in an ASWebAuthenticationSession: mobile-width
    // rendering, and the server's `exponential://github-connected` redirect
    // dismisses it and hands control back.
    private func openInBrowser(_ urlString: String?) {
        guard let urlString, let url = URL(string: urlString) else { return }
        installSession.start(url: url) {
            Task { await load(refresh: true) }
        }
    }

    private func load(refresh: Bool = false) async {
        await MainActor.run { loading = true }
        do {
            let r = try await integrationsApi.githubRepos(accountId: accountId, teamId: teamId, refresh: refresh)
            await MainActor.run {
                result = r
                error = nil
                loading = false
            }
        } catch {
            await MainActor.run { self.error = error.trpcUserMessage; loading = false }
        }
    }
}

/// Presents the GitHub App install page in an ASWebAuthenticationSession so it
/// (a) renders as a phone-sized in-app page instead of desktop Safari and
/// (b) auto-dismisses when the server's post-install page fires the
/// `exponential://github-connected` deep link (callback scheme `exponential`).
/// The completion fires on callback AND on manual dismissal — the install may
/// have landed either way, so callers should re-query regardless.
@MainActor
final class InstallWebAuthSession: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    func start(url: URL, onFinished: @escaping @MainActor () -> Void) {
        session?.cancel()
        let session = ASWebAuthenticationSession(
            url: url,
            callbackURLScheme: "exponential"
        ) { [weak self] _, _ in
            Task { @MainActor in
                self?.session = nil
                onFinished()
            }
        }
        session.presentationContextProvider = self
        // The user is signing in to GitHub — share the persistent cookie jar so
        // an existing GitHub session is reused instead of forcing a fresh login.
        session.prefersEphemeralWebBrowserSession = false
        self.session = session
        session.start()
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows)
                .first { $0.isKeyWindow } ?? ASPresentationAnchor()
        }
    }
}
