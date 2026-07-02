import AppKit
import ExpCore
import ExpUI
import SwiftUI

/// Integrations panel mirroring the iOS `IntegrationsView`, adapted to a macOS
/// sheet. Shows the GitHub App install state (web /account/integrations parity);
/// the install/manage flow lives on github.com — the button opens the default
/// browser and the status re-queries when the window regains focus.
struct MacIntegrationsView: View {
    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    let accountId: String

    @State private var status: GithubStatusResult?
    @State private var loading = true
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Integrations").font(.headline)
                Spacer()
                Button("Done") { dismiss() }
            }
            .padding()
            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 10) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right").font(.title2)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("GitHub").font(.body.weight(.medium))
                            Text("Install the Exponential GitHub App on the repos you want to code on. It opens pull requests, reads diffs, and lets your coding sessions clone + push — scoped to just those repos.")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }

                    if loading && status == nil {
                        ProgressView()
                    } else if let status {
                        if !status.configured {
                            Text("GitHub isn't configured for this server.")
                                .font(.caption).foregroundStyle(.secondary)
                        } else if status.installed {
                            HStack(spacing: 6) {
                                Circle().fill(.green).frame(width: 8, height: 8)
                                Text(installedLabel(status)).font(.caption).foregroundStyle(.green)
                            }
                            if status.installUrl != nil {
                                Button("Manage / add repos…") { openInstall(status) }
                            }
                        } else {
                            Text("Not installed").font(.caption).foregroundStyle(.secondary)
                            if status.installUrl != nil {
                                Button("Install GitHub App…") { openInstall(status) }
                            }
                        }
                    }

                    if let error {
                        Text(error).font(.caption).foregroundStyle(.red)
                    }
                }
                .padding(16)
                .glassSection()
                .padding(16)
            }
        }
        .frame(width: 520, height: 360)
        .task { await loadStatus() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            // Re-check after returning from the browser install flow.
            Task { await loadStatus() }
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
            // Clear any stale failure banner — this view re-queries on window
            // focus, so a past error must not outlive a successful load.
            error = nil
            loading = false
        } catch {
            self.error = error.localizedDescription
            loading = false
        }
    }
}
