import ExpUI
import ExpCore
import SwiftUI

struct WorkspaceInvitesSection: View {
    let accountId: String
    let workspaceId: String
    let invites: [WorkspaceInviteEntity]
    let invitesApi: WorkspaceInvitesApi

    @State private var generatedLink: String?
    @State private var copied = false
    @State private var generating = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Invite Members")
                .font(.headline)
                .foregroundStyle(.white)

            Text("Generate a link to invite someone to this workspace.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            // Generate link button
            Button {
                Task { await generateLink() }
            } label: {
                HStack(spacing: 6) {
                    if generating {
                        ProgressView().tint(.white)
                    } else {
                        Image(systemName: "link")
                    }
                    Text("Generate invite link")
                }
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
            }
            .glassButton()
            .disabled(generating)
            .buttonStyle(.plain)

            // Generated link
            if let link = generatedLink {
                HStack {
                    Text(link)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .truncationMode(.middle)

                    Button {
                        UIPasteboard.general.string = link
                        copied = true
                        Task {
                            try? await Task.sleep(for: .seconds(2))
                            copied = false
                        }
                    } label: {
                        Image(systemName: copied ? "checkmark" : "doc.on.doc")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .glassRow()
            }

            // Pending invites
            if !invites.isEmpty {
                Text("Pending")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.top, 8)

                ForEach(invites, id: \.id) { invite in
                    HStack {
                        Text(invite.role)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .padding(.horizontal, 8)
                            .padding(.vertical, 3)
                            .glassButton()

                        Text("Expires \(invite.expiresAt.prefix(10))")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                        Spacer()

                        Button {
                            Task { try? await invitesApi.revoke(accountId: accountId, inviteId: invite.id) }
                        } label: {
                            Image(systemName: "trash")
                                .font(.caption)
                                .foregroundStyle(.red.opacity(0.7))
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassRow()
                }
            }
        }
    }

    private func generateLink() async {
        generating = true
        do {
            let result = try await invitesApi.create(accountId: accountId, workspaceId: workspaceId, role: DomainContract.workspaceRoleMember)
            generatedLink = "exponential://invite/\(result.token)"
        } catch {}
        generating = false
    }
}
