import ExpCore
import ExpUI
import SwiftUI

/// Accept a workspace invite opened via `exp://invite/<token>`. macOS mirror of
/// the iOS `InviteAcceptView`; presented as a sheet from `MacRootView`.
struct MacInviteAcceptView: View {
    let accountId: String
    let token: String

    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @State private var loading = true
    @State private var accepted = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 20) {
            Image(systemName: accepted ? "checkmark.circle.fill" : "person.2")
                .font(.system(size: 48))
                .foregroundStyle(accepted ? .green : .secondary)

            if loading {
                ProgressView()
                Text("Accepting invite…").foregroundStyle(.secondary)
            } else if accepted {
                Text("Welcome!").font(.title2.weight(.bold))
                Text("You've joined the workspace.").foregroundStyle(.secondary)
            } else if let error {
                Text("Invite failed").font(.title2.weight(.bold))
                Text(error).foregroundStyle(.red).multilineTextAlignment(.center)
            }

            if !loading {
                Button("Close") { dismiss() }.keyboardShortcut(.defaultAction)
            }
        }
        .padding(40)
        .frame(width: 360)
        .task {
            do {
                try await deps.workspaceInvitesApi.accept(accountId: accountId, token: token)
                accepted = true
                loading = false
                try? await Task.sleep(for: .seconds(1.5))
                dismiss()
            } catch {
                self.error = error.localizedDescription
                loading = false
            }
        }
    }
}
