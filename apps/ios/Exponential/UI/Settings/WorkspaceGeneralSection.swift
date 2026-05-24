import SwiftUI

// Mirrors the General section the web app exposes under workspace settings:
// toggle the workspace public/private, and (when public) select the write
// policy that gates non-member create/edit access.
struct WorkspaceGeneralSection: View {
    let workspace: WorkspaceEntity?
    let workspacesApi: WorkspacesApi

    @State private var saving = false
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("General")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            if let workspace {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Public workspace")
                            .font(.subheadline)
                            .foregroundStyle(.white)
                        Text("Anyone with the link can read this workspace.")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                    Spacer()
                    Toggle("", isOn: Binding(
                        get: { workspace.isPublic },
                        set: { newValue in
                            Task { await setPublic(newValue) }
                        }
                    ))
                    .labelsHidden()
                    .disabled(saving)
                }
                .padding(.vertical, 6)

                if workspace.isPublic {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Who can create issues?")
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))

                        Picker(
                            "Write policy",
                            selection: Binding(
                                get: { workspace.publicWritePolicy ?? DomainContract.publicWritePolicyMembers },
                                set: { newValue in
                                    Task { await setPolicy(newValue) }
                                }
                            )
                        ) {
                            Text("Members only").tag(DomainContract.publicWritePolicyMembers)
                            Text("Anyone signed in").tag(DomainContract.publicWritePolicyEveryone)
                        }
                        .pickerStyle(.segmented)
                        .disabled(saving)

                        Text(
                            (workspace.publicWritePolicy ?? DomainContract.publicWritePolicyMembers) == DomainContract.publicWritePolicyEveryone
                                ? "Signed-in users can create issues; non-members may only set title, description, and labels."
                                : "Only workspace members can create or edit issues."
                        )
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
            } else {
                Text("Loading…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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

    private func setPublic(_ isPublic: Bool) async {
        guard let workspace else { return }
        saving = true
        defer { saving = false }
        error = nil
        do {
            try await workspacesApi.update(UpdateWorkspaceInput(
                id: workspace.id,
                name: nil,
                isPublic: isPublic,
                // First-time enable defaults to members-only; matches web.
                publicWritePolicy: isPublic ? DomainContract.publicWritePolicyMembers : nil,
                iconUrl: nil
            ))
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func setPolicy(_ policy: String) async {
        guard let workspace else { return }
        saving = true
        defer { saving = false }
        error = nil
        do {
            try await workspacesApi.update(UpdateWorkspaceInput(
                id: workspace.id,
                name: nil,
                isPublic: nil,
                publicWritePolicy: policy,
                iconUrl: nil
            ))
        } catch {
            self.error = error.localizedDescription
        }
    }
}
