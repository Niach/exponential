import ExpUI
import ExpCore
import SwiftUI

struct HomeView: View {
    var syncing: Bool = false
    var onProjectTap: (_ accountId: String, _ projectId: String) -> Void = { _, _ in }
    var projectLoader: MultiAccountProjectLoader? = nil

    @Environment(AppDependencies.self) private var deps

    var body: some View {
        ZStack {
            AppBackground()

            let groups = projectLoader?.groups ?? []
            if groups.isEmpty {
                if syncing {
                    VStack(spacing: 12) {
                        ProgressView()
                            .tint(.white)
                        Text("Syncing...")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                } else {
                    VStack(spacing: 8) {
                        Image(systemName: "tray")
                            .font(.title2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        Text("No projects yet")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                }
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 18) {
                        ForEach(groups) { group in
                            serverSection(group)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 12)
                    .padding(.bottom, 24)
                }
            }
        }
        .navigationTitle("Projects")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                inboxButton
            }
            ToolbarItem(placement: .topBarTrailing) {
                settingsButton
            }
        }
    }

    private var inboxButton: some View {
        NavigationLink(value: AppRoute.inbox) {
            Image(systemName: "tray")
        }
    }

    @ViewBuilder
    private func serverSection(_ group: ServerProjectGroup) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(group.hostname)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white.opacity(0.9))
                if let email = group.userEmail, !email.isEmpty {
                    Text(email)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .padding(.horizontal, 4)

            ForEach(group.workspaceBlocks) { block in
                workspaceBlock(accountId: group.accountId, block: block)
            }
        }
    }

    @ViewBuilder
    private func workspaceBlock(accountId: String, block: WorkspaceBlock) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                WorkspaceAvatar(workspace: block.workspace, size: 18)
                Text(block.workspace.name)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white.opacity(0.85))
                Spacer()
                Text("\(block.projects.count)")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 4)

            VStack(spacing: 6) {
                ForEach(block.projects) { project in
                    Button {
                        onProjectTap(accountId, project.id)
                    } label: {
                        projectRow(project)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var settingsButton: some View {
        NavigationLink(value: AppRoute.settings) {
            Image(systemName: "gearshape")
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 32, height: 32)
                .contentShape(Circle())
        }
    }

    @ViewBuilder
    private func projectRow(_ project: ProjectEntity) -> some View {
        HStack(spacing: 12) {
            Circle()
                .fill(Color(hex: project.color ?? "#888888") ?? .gray)
                .frame(width: 10, height: 10)

            Text(project.name)
                .font(.body)
                .foregroundStyle(.white)

            Spacer()

            Text(project.prefix)
                .font(.caption.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Image(systemName: "chevron.right")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .glassRow()
    }
}
