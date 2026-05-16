import SwiftUI

struct SidebarView: View {
    let workspaces: [WorkspaceEntity]
    let activeWorkspaceId: String?
    let projects: [ProjectEntity]
    let onSelectWorkspace: (String) -> Void
    let onSignOut: () -> Void

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()

                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        // Workspace switcher
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Workspaces")
                                .font(.caption.weight(.medium))
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .padding(.horizontal, 4)

                            ForEach(workspaces) { workspace in
                                Button {
                                    onSelectWorkspace(workspace.id)
                                } label: {
                                    HStack(spacing: 10) {
                                        Text(workspace.name.prefix(1).uppercased())
                                            .font(.caption.weight(.bold))
                                            .foregroundStyle(.white)
                                            .frame(width: 28, height: 28)
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
                                    .glassRow()
                                }
                                .buttonStyle(.plain)
                            }
                        }

                        // Projects
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Projects")
                                .font(.caption.weight(.medium))
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .padding(.horizontal, 4)

                            ForEach(projects) { project in
                                HStack(spacing: 10) {
                                    Circle()
                                        .fill(Color(hex: project.color ?? "#888888") ?? .gray)
                                        .frame(width: 8, height: 8)

                                    Text(project.name)
                                        .font(.subheadline)
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                                .padding(.horizontal, 12)
                                .padding(.vertical, 6)
                            }
                        }

                        Divider()
                            .background(Color.white.opacity(0.1))

                        // Sign out
                        Button(action: onSignOut) {
                            HStack(spacing: 10) {
                                Image(systemName: "rectangle.portrait.and.arrow.right")
                                    .font(.body)
                                Text("Sign out")
                                    .font(.body)
                            }
                            .foregroundStyle(.red.opacity(0.8))
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(16)
                }
            }
            .navigationTitle("Menu")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        }
    }
}
