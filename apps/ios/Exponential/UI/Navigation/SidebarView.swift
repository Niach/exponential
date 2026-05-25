import SwiftUI

struct SidebarView: View {
    let groups: [ServerWorkspaceGroup]
    let activeAccountId: String?
    let activeWorkspaceId: String?
    let onSelectWorkspace: (_ accountId: String, _ workspaceId: String) -> Void

    var body: some View {
        ZStack {
            AppBackground()

            VStack(spacing: 0) {
                Text("Switch workspace")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.top, 16)
                    .padding(.bottom, 12)

                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(groups) { group in
                            ServerGroupHeader(hostname: group.hostname, userEmail: group.userEmail)

                            VStack(spacing: 2) {
                                ForEach(group.workspaces) { workspace in
                                    Button {
                                        onSelectWorkspace(group.accountId, workspace.id)
                                    } label: {
                                        HStack(spacing: 12) {
                                            WorkspaceAvatar(workspace: workspace)

                                            Text(workspace.name)
                                                .font(.body)
                                                .foregroundStyle(.white)

                                            Spacer()

                                            if group.accountId == activeAccountId
                                                && workspace.id == activeWorkspaceId {
                                                Image(systemName: "checkmark")
                                                    .font(.caption.weight(.bold))
                                                    .foregroundStyle(.blue)
                                            }
                                        }
                                        .padding(.horizontal, 12)
                                        .padding(.vertical, 10)
                                        .contentShape(Rectangle())
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                            .padding(.bottom, 8)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.bottom, 16)
                }
            }
        }
    }
}

private struct ServerGroupHeader: View {
    let hostname: String
    let userEmail: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(hostname)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.white.opacity(0.85))
            if let userEmail, !userEmail.isEmpty {
                Text(userEmail)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(0.5))
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 4)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct WorkspaceAvatar: View {
    let workspace: WorkspaceEntity
    var size: CGFloat = 24

    var body: some View {
        Group {
            if let urlString = workspace.iconUrl,
               !urlString.isEmpty,
               let url = URL(string: urlString) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case let .success(image):
                        image.resizable().scaledToFill()
                    default:
                        initialsChip
                    }
                }
            } else {
                initialsChip
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: size / 4))
    }

    private var initialsChip: some View {
        Text(workspace.name.prefix(1).uppercased())
            .font(.caption.weight(.bold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Color.blue.opacity(0.6))
    }
}
