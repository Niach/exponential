import AuthenticationServices
import Combine
import ExpUI
import ExpCore
import SwiftUI

// Installed-repo picker (web github-repo-picker.tsx): lists the repos the user's
// GitHub App is installed on and returns the chosen one to the caller. v4: it no
// longer links a repo to a project directly — instead it feeds the create-project
// inline-connect path (`repository: { fullName }`). Handles not-configured /
// not-installed (in-app install flow + auto re-query) / installed (searchable
// list). The link/upsert happens server-side in `projects.create`.
//
// EXP-8: the install URL opens in an ASWebAuthenticationSession (mobile-width
// page, in-app) instead of kicking out to system Safari. The server's
// post-install page fires `exp://github-connected`, which auto-dismisses the
// session; either way the completion re-queries with `refresh: true` so the
// newly connected repos appear without any manual step.
struct GithubRepoPicker: View {
    let accountId: String
    /// Scopes the repo query + connect hop to this workspace's linked GitHub
    /// accounts (per-workspace installation claiming).
    let workspaceId: String
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
            // The app-level deep-link path for `exp://github-connected` — covers
            // an install that finishes outside the in-app auth session.
            .onReceive(NotificationCenter.default.publisher(for: .githubConnected)) { _ in
                Task { await load(refresh: true) }
            }
        }
    }

    @ViewBuilder private var content: some View {
        if loading && result == nil {
            HStack { Spacer(); ProgressView().tint(.white); Spacer() }.padding(.vertical, 24)
        } else if let data = result, data.configured {
            if data.installed {
                installedList(data)
            } else {
                notInstalled(data)
            }
        } else {
            Text("GitHub isn't configured for this server.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    @ViewBuilder private func notInstalled(_ data: GithubReposResult) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Install the Exponential GitHub App to pick a repository. You'll come right back here.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Button {
                openConnect(data)
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.left.forwardslash.chevron.right")
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

    @ViewBuilder private func installedList(_ data: GithubReposResult) -> some View {
        let repos = data.repos.filter {
            query.isEmpty || $0.fullName.localizedCaseInsensitiveContains(query.trimmingCharacters(in: .whitespaces))
        }
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .font(.caption)
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

            ForEach(repos) { repo in
                Button {
                    onPick(repo)
                    dismiss()
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Text(repo.fullName)
                            .font(.subheadline.monospaced())
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        Spacer()
                        if repo.`private` {
                            Image(systemName: "lock.fill")
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .glassRow()
                }
                .buttonStyle(.plain)
            }

            if data.hasMore {
                Button { openManage(data) } label: {
                    Text("Don't see your repo? Manage repositories on GitHub.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .buttonStyle(.plain)
            }
        }
    }

    // Connect action: claim a GitHub account for this workspace. Prefer the
    // mobile-friendly OAuth `connectUrl` (single consent screen) and fall back
    // to the GitHub App install page when it's absent.
    private func openConnect(_ data: GithubReposResult) {
        openInBrowser(data.connectUrl ?? data.installUrl)
    }

    // Manage action: grant more repos to an existing installation — always the
    // GitHub App install/configure page.
    private func openManage(_ data: GithubReposResult) {
        openInBrowser(data.installUrl)
    }

    // Web parity (github-repo-picker.tsx): the old `/account/integrations`
    // fallback was removed in v5 (repo management lives in workspace settings →
    // Repositories). Opened in an ASWebAuthenticationSession: mobile-width
    // rendering, and the server's `exp://github-connected` redirect dismisses it
    // and hands control back.
    private func openInBrowser(_ urlString: String?) {
        guard let urlString, let url = URL(string: urlString) else { return }
        installSession.start(url: url) {
            Task { await load(refresh: true) }
        }
    }

    private func load(refresh: Bool = false) async {
        await MainActor.run { loading = true }
        do {
            let r = try await integrationsApi.githubRepos(accountId: accountId, workspaceId: workspaceId, refresh: refresh)
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
/// `exp://github-connected` deep link (callback scheme `exp`). The completion
/// fires on callback AND on manual dismissal — the install may have landed
/// either way, so callers should re-query regardless.
@MainActor
final class InstallWebAuthSession: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    func start(url: URL, onFinished: @escaping @MainActor () -> Void) {
        session?.cancel()
        let session = ASWebAuthenticationSession(
            url: url,
            callbackURLScheme: "exp"
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
