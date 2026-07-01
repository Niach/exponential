import ExpCore
import ExpUI
import SwiftUI

/// First-run wizard (web + Android onboarding parity): create your first
/// project, then your first issue, gated once by onboardingCompletedAt.
/// `workspaces.ensureDefault` resolves the workspace the project goes into —
/// server-side it never picks a public workspace (e.g. the cloud feedback
/// workspace), creating a personal one instead. On finish/skip it calls
/// onboarding.complete and flips the local account flag so the nav gate in
/// AppNavigator stops showing the wizard.
struct OnboardingView: View {
    @Environment(AppDependencies.self) private var deps

    @State private var step = 0 // 0 = project, 1 = first issue
    @State private var workspaceId: String?
    @State private var projectId: String?
    @State private var busy = false
    @State private var error: String?

    // Project step
    @State private var projectName = ""
    @State private var prefix = ""
    @State private var prefixEdited = false
    @State private var color = DEFAULT_LABEL_COLOR

    // Issue step
    @State private var issueTitle = ""

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(spacing: 0) {
                    Text("Welcome to Exponential")
                        .font(.system(size: 28, weight: .bold))
                        .foregroundStyle(.white)
                        .multilineTextAlignment(.center)

                    Spacer().frame(height: 8)

                    Text("Let's set up your workspace.")
                        .font(.body)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .multilineTextAlignment(.center)

                    Spacer().frame(height: 20)

                    progressDots

                    Spacer().frame(height: 24)

                    if step == 0 {
                        projectStep
                    } else {
                        issueStep
                    }

                    if let error {
                        Spacer().frame(height: 12)
                        Text(error)
                            .font(.caption)
                            .foregroundStyle(.red)
                            .multilineTextAlignment(.center)
                    }

                    Spacer().frame(height: 16)

                    Button {
                        Task { await skip() }
                    } label: {
                        Text("Skip setup entirely")
                            .font(.body)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.vertical, 12)
                    }
                    .buttonStyle(.plain)
                    .disabled(busy)
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 48)
                .frame(maxWidth: .infinity)
            }
        }
        .task {
            await reconcileWithServer()
            if deps.auth.needsOnboarding { await ensureWorkspace() }
        }
    }

    // MARK: - Steps

    private var projectStep: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Create your first project")
                .font(.headline)
                .foregroundStyle(.white)

            field("Project name", text: $projectName)
                .onChange(of: projectName) { _, newValue in
                    // Auto-suggest a prefix from the name until edited.
                    if !prefixEdited {
                        prefix = String(newValue.filter(\.isLetter).prefix(3)).uppercased()
                    }
                }

            field("Issue prefix (e.g. ENG)", text: $prefix, monospaced: true)
                .textInputAutocapitalization(.characters)
                .autocorrectionDisabled()
                .onChange(of: prefix) { _, _ in prefixEdited = true }

            Text("Color")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            ColorSwatchGrid(selection: $color)

            primaryButton(busy ? "Creating…" : "Continue", enabled: canCreateProject) {
                Task { await createProject() }
            }
        }
        .padding(24)
        .glassCard()
    }

    private var issueStep: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Create your first issue")
                .font(.headline)
                .foregroundStyle(.white)

            field("Issue title", text: $issueTitle)

            HStack(spacing: 12) {
                Button {
                    Task { await skip() }
                } label: {
                    Text("Skip")
                        .font(.body)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 14)
                }
                .buttonStyle(.plain)
                .disabled(busy)

                primaryButton(busy ? "Creating…" : "Create issue", enabled: canCreateIssue) {
                    Task { await createIssue() }
                }
            }
        }
        .padding(24)
        .glassCard()
    }

    private var progressDots: some View {
        HStack(spacing: 8) {
            ForEach(0..<2, id: \.self) { i in
                Circle()
                    .fill(i <= step ? Color.white : Color.white.opacity(0.2))
                    .frame(width: i == step ? 10 : 8, height: i == step ? 10 : 8)
                if i < 1 {
                    Rectangle()
                        .fill(Color.white.opacity(0.2))
                        .frame(width: 20, height: 2)
                }
            }
        }
    }

    // MARK: - Building blocks

    private func field(_ placeholder: String, text: Binding<String>, monospaced: Bool = false) -> some View {
        TextField(placeholder, text: text)
            .textFieldStyle(.plain)
            .font(monospaced ? .body.monospaced() : .body)
            .foregroundStyle(.white)
            .padding(12)
            .background(Color.white.opacity(0.06))
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
            )
    }

    private func primaryButton(_ title: String, enabled: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.body.weight(.medium))
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
        }
        .disabled(!enabled)
        .background(enabled ? Color.white.opacity(0.15) : Color.white.opacity(0.06))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
        )
    }

    private var canCreateProject: Bool {
        !busy && workspaceId != nil
            && !projectName.trimmingCharacters(in: .whitespaces).isEmpty
            && !prefix.trimmingCharacters(in: .whitespaces).isEmpty
    }

    private var canCreateIssue: Bool {
        !busy && !issueTitle.trimmingCharacters(in: .whitespaces).isEmpty
    }

    // MARK: - Actions

    /// The server backfills onboardingCompletedAt on session reads for users
    /// who already have a project in a non-public workspace (the unified rule
    /// in lib/auth/onboarding.ts). Re-read the session before making the user
    /// walk the wizard, so an account whose flag was still null at login
    /// self-heals here instead of re-onboarding.
    private func reconcileWithServer() async {
        guard let accountId = deps.auth.activeAccountId,
              let user = await deps.authApi.fetchSession(accountId: accountId),
              let completedAt = user.onboardingCompletedAt
        else { return }
        deps.auth.markOnboardingCompleted(completedAt)
    }

    private func ensureWorkspace() async {
        guard workspaceId == nil, let accountId = deps.auth.activeAccountId else { return }
        do {
            workspaceId = try await deps.workspacesApi.ensureDefault(accountId: accountId).id
        } catch {
            self.error = "Couldn't load your workspace: \(error.localizedDescription)"
        }
    }

    private func createProject() async {
        guard !busy, let workspaceId, let accountId = deps.auth.activeAccountId else { return }
        busy = true
        error = nil
        do {
            projectId = try await deps.projectsApi.create(accountId: accountId, CreateProjectInput(
                workspaceId: workspaceId,
                name: projectName.trimmingCharacters(in: .whitespaces),
                prefix: prefix.trimmingCharacters(in: .whitespaces).uppercased(),
                color: color
            ))
            step = 1
        } catch {
            self.error = error.localizedDescription
        }
        busy = false
    }

    private func createIssue() async {
        guard !busy, let projectId, let accountId = deps.auth.activeAccountId else { return }
        busy = true
        error = nil
        do {
            _ = try await deps.issuesApi.create(
                accountId: accountId,
                CreateIssueInput(projectId: projectId, title: issueTitle.trimmingCharacters(in: .whitespaces))
            )
            await finish()
        } catch {
            self.error = error.localizedDescription
            busy = false
        }
    }

    /// Skip the rest of setup (still marks onboarding complete, like web).
    private func skip() async {
        guard !busy else { return }
        busy = true
        error = nil
        await finish()
    }

    // Deliberately leaves `busy` set: flipping needsOnboarding swaps this view
    // out, and re-enabling the buttons first would open a double-submit window.
    private func finish() async {
        if let accountId = deps.auth.activeAccountId {
            try? await deps.onboardingApi.complete(accountId: accountId)
        }
        deps.auth.markOnboardingCompleted(ISO8601DateFormatter().string(from: Date()))
    }
}
