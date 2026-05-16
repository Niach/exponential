import SwiftUI

struct SidebarView: View {
    let workspaces: [WorkspaceEntity]
    let activeWorkspaceId: String?
    let onSelectWorkspace: (String) -> Void

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
                    VStack(spacing: 2) {
                        ForEach(workspaces) { workspace in
                            Button {
                                onSelectWorkspace(workspace.id)
                            } label: {
                                HStack(spacing: 12) {
                                    Text(workspace.name.prefix(1).uppercased())
                                        .font(.caption.weight(.bold))
                                        .foregroundStyle(.white)
                                        .frame(width: 24, height: 24)
                                        .background(Color.blue.opacity(0.6))
                                        .clipShape(RoundedRectangle(cornerRadius: 6))

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
