import SwiftUI

struct SettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(WorkspaceState.self) private var workspaceState

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    accountSection
                    generalSection
                    if deps.auth.isAdmin {
                        adminSection
                    }
                    signOutSection
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 96)
            }
        }
        .navigationTitle("Settings")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
    }

    private var accountSection: some View {
        sectionStack(title: "Account") {
            HStack(spacing: 10) {
                Image(systemName: "person.crop.circle")
                    .font(.title3)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                VStack(alignment: .leading, spacing: 2) {
                    if let name = deps.auth.userName, !name.isEmpty {
                        Text(name)
                            .font(.body)
                            .foregroundStyle(.white)
                    }
                    Text(deps.auth.userEmail ?? "Signed in")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                Spacer()
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .glassRow()
        }
    }

    private var generalSection: some View {
        sectionStack(title: "General") {
            VStack(spacing: 6) {
                NavigationLink(value: AppRoute.integrations) {
                    settingsRow(icon: "puzzlepiece.extension", title: "Integrations")
                }
                .buttonStyle(.plain)

                if let workspaceId = workspaceState.activeWorkspace?.id {
                    NavigationLink(value: AppRoute.workspaceSettings(workspaceId: workspaceId)) {
                        settingsRow(icon: "building.2", title: "Workspace settings")
                    }
                    .buttonStyle(.plain)
                }

                if let url = feedbackUrl() {
                    Button {
                        UIApplication.shared.open(url)
                    } label: {
                        settingsRow(icon: "envelope", title: "Send feedback")
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    // The web `/feedback` route redirects to the workspace+project both
    // slugged "feedback". Mobile opens that URL on the configured instance
    // so feedback lands in the same shared workspace the web app uses.
    private func feedbackUrl() -> URL? {
        guard let baseUrl = deps.auth.instanceUrl else { return nil }
        return URL(string: "\(baseUrl)/w/feedback/projects/feedback")
    }

    private var adminSection: some View {
        sectionStack(title: "Admin") {
            VStack(spacing: 6) {
                NavigationLink(value: AppRoute.adminUsers) {
                    settingsRow(icon: "person.2", title: "Users")
                }
                .buttonStyle(.plain)

                NavigationLink(value: AppRoute.adminWorkspaces) {
                    settingsRow(icon: "rectangle.stack", title: "Workspaces")
                }
                .buttonStyle(.plain)
            }
        }
    }

    private var signOutSection: some View {
        Button {
            deps.auth.clearToken()
            Task { await deps.syncManager.signOut() }
        } label: {
            HStack(spacing: 10) {
                Image(systemName: "rectangle.portrait.and.arrow.right")
                    .font(.body)
                Text("Sign out")
                    .font(.body)
                Spacer()
            }
            .foregroundStyle(.red.opacity(0.85))
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .glassRow()
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func sectionStack<Content: View>(title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 4)
            content()
        }
    }

    private func settingsRow(icon: String, title: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 22)
            Text(title)
                .font(.body)
                .foregroundStyle(.white)
            Spacer()
            Image(systemName: "chevron.right")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .glassRow()
    }
}
