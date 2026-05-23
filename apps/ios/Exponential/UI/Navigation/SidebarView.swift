import SwiftUI

struct SidebarView: View {
    let workspaces: [WorkspaceEntity]
    let activeWorkspaceId: String?
    let onSelectWorkspace: (String) -> Void

    var body: some View {
        ZStack {
            AppBackground()

            // Workspace tile shows the icon when set (icon_url), otherwise
            // falls back to the first letter of the workspace name on a
            // colored chip.
            VStack(spacing: 0) {
                Text("Switch workspace")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.top, 16)
                    .padding(.bottom, 12)

                ScrollView {
                    VStack(spacing: 2) {
                        ForEach(workspaces) { workspace in
                            Button {
                                onSelectWorkspace(workspace.id)
                            } label: {
                                HStack(spacing: 12) {
                                    WorkspaceAvatar(workspace: workspace)

                                    Text(workspace.name)
                                        .font(.body)
                                        .foregroundStyle(.white)

                                    Spacer()

                                    if workspace.id == activeWorkspaceId {
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
                    .padding(.horizontal, 12)
                    .padding(.bottom, 16)
                }
            }
        }
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
