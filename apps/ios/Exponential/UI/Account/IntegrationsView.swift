import ExpCore
import ExpUI
import SwiftUI

struct IntegrationsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.scenePhase) private var scenePhase
    @State private var status: GithubStatusResult?
    @State private var loading = true
    @State private var error: String?

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // GitHub App card (mirrors web /account/integrations)
                    VStack(alignment: .leading, spacing: 12) {
                        HStack(spacing: 10) {
                            Image(systemName: "chevron.left.forwardslash.chevron.right")
                                .font(.title2)
                                .foregroundStyle(.white)

                            VStack(alignment: .leading, spacing: 2) {
                                Text("GitHub")
                                    .font(.body.weight(.medium))
                                    .foregroundStyle(.white)

                                Text("Install the Exponential GitHub App on the repos you want to code on. It opens pull requests, reads diffs, and lets your desktop coding sessions clone + push — scoped to just those repos.")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                        }

                        if loading && status == nil {
                            ProgressView().tint(.white)
                        } else if let status {
                            if !status.configured {
                                Text("GitHub isn't configured for this server.")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            } else if status.installed {
                                HStack(spacing: 6) {
                                    Circle()
                                        .fill(.green)
                                        .frame(width: 8, height: 8)
                                    Text(installedLabel(status))
                                        .font(.caption)
                                        .foregroundStyle(.green)
                                }

                                if status.installUrl != nil {
                                    Button {
                                        openInstall(status)
                                    } label: {
                                        Text("Manage / add repos")
                                            .font(.subheadline)
                                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                            .padding(.horizontal, 14)
                                            .padding(.vertical, 8)
                                    }
                                    .glassButton()
                                    .buttonStyle(.plain)
                                }
                            } else {
                                // The install flow lives on github.com — open the
                                // browser (matches macOS); the status refreshes when
                                // the app returns to the foreground.
                                VStack(alignment: .leading, spacing: 8) {
                                    Text("Not installed")
                                        .font(.caption)
                                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    if status.installUrl != nil {
                                        Button {
                                            openInstall(status)
                                        } label: {
                                            Text("Install GitHub App")
                                                .font(.subheadline)
                                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                                .padding(.horizontal, 14)
                                                .padding(.vertical, 8)
                                        }
                                        .glassButton()
                                        .buttonStyle(.plain)
                                    }
                                }
                            }
                        }

                        if let error {
                            Text(error)
                                .font(.caption)
                                .foregroundStyle(.red)
                        }
                    }
                    .padding(16)
                    .glassSection()
                }
                .padding(16)
            }
        }
        .navigationTitle("Integrations")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task { await loadStatus() }
        .onChange(of: scenePhase) { _, newPhase in
            // Re-check after returning from the GitHub install flow.
            if newPhase == .active { Task { await loadStatus() } }
        }
    }

    private func installedLabel(_ status: GithubStatusResult) -> String {
        status.accounts.isEmpty
            ? "Installed"
            : "Installed · \(status.accounts.joined(separator: ", "))"
    }

    private func openInstall(_ status: GithubStatusResult) {
        if let urlString = status.installUrl, let url = URL(string: urlString) {
            Platform.open(url)
        }
    }

    private func loadStatus() async {
        do {
            status = try await deps.integrationsApi.githubStatus(accountId: accountId)
            // Clear any stale failure banner — this view re-queries on
            // foreground, so a past error must not outlive a successful load.
            error = nil
            loading = false
        } catch {
            self.error = error.localizedDescription
            loading = false
        }
    }
}
