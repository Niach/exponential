import ExpUI
import ExpCore
import SwiftUI

/// Bottom-sheet project picker for the Issues tab's inline switcher: the
/// server → workspace → project tree that used to be the Projects overview
/// screen, now presented modally. Selecting a project swaps the Issues tab's
/// list in place (the caller writes last-used and dismisses).
struct ProjectSwitcherSheet: View {
    let projectLoader: MultiAccountProjectLoader?
    let currentProject: CurrentProjectRef?
    let onSelect: (_ accountId: String, _ projectId: String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Switch project")
                .font(.headline)
                .foregroundStyle(.white)
                .padding(.horizontal, 20)
                .padding(.top, 18)
                .padding(.bottom, 10)

            let groups = projectLoader?.groups ?? []
            if groups.isEmpty {
                emptyHint
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 18) {
                        ForEach(groups) { group in
                            serverSection(group)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 4)
                    .padding(.bottom, 24)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var emptyHint: some View {
        VStack(spacing: 10) {
            Spacer()
            Image(systemName: "tray")
                .font(.title3)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("Create your first project on the web or desktop app.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
            Spacer()
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 40)
    }

    @ViewBuilder
    private func serverSection(_ group: ServerProjectGroup) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            // The hostname/email header only disambiguates when several
            // accounts are signed in — with a single account it's noise.
            if (projectLoader?.groups.count ?? 0) > 1 {
                HStack(alignment: .top) {
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
                    Spacer()
                }
                .padding(.horizontal, 4)
            }

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
                        onSelect(accountId, project.id)
                    } label: {
                        projectRow(project, isCurrent: isCurrent(accountId: accountId, projectId: project.id))
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private func isCurrent(accountId: String, projectId: String) -> Bool {
        currentProject?.accountId == accountId && currentProject?.projectId == projectId
    }

    @ViewBuilder
    private func projectRow(_ project: ProjectEntity, isCurrent: Bool) -> some View {
        HStack(spacing: 12) {
            // Project glyph (stored icon, else a shape-derived fallback) tinted
            // with the project color (replaces the plain color dot).
            Image(systemName: ProjectTypeDisplay.symbol(for: project))
                .font(.caption)
                .foregroundStyle(Color(hex: project.color ?? "#888888") ?? .gray)
                .frame(width: 16, height: 16)

            Text(project.name)
                .font(.body)
                .foregroundStyle(.white)

            Spacer()

            Text(project.prefix)
                .font(.caption.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            if isCurrent {
                Image(systemName: "checkmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Accent.indigo)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .glassRow()
    }
}
