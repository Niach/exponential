import SwiftUI

enum BottomTab: Hashable {
    case projects
    case settings
}

struct BottomBar: View {
    @Binding var selectedTab: BottomTab
    let workspace: WorkspaceEntity?
    let onWorkspaceTap: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            tabsCapsule
            Spacer(minLength: 8)
            workspacePill
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
    }

    private var tabsCapsule: some View {
        HStack(spacing: 4) {
            tabButton(.projects, systemName: "folder")
            tabButton(.settings, systemName: "gearshape")
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 6)
        .background(.ultraThinMaterial)
        .clipShape(Capsule())
        .overlay(
            Capsule().stroke(Color.white.opacity(0.1), lineWidth: 0.5)
        )
        .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
    }

    private func tabButton(_ tab: BottomTab, systemName: String) -> some View {
        let isActive = selectedTab == tab
        return Button {
            selectedTab = tab
        } label: {
            Image(systemName: systemName)
                .font(.system(size: 17, weight: isActive ? .semibold : .regular))
                .foregroundStyle(.white.opacity(isActive ? TextOpacity.primary : TextOpacity.tertiary))
                .frame(width: 40, height: 32)
                .background(
                    Capsule().fill(Color.white.opacity(isActive ? 0.15 : 0))
                )
        }
        .buttonStyle(.plain)
    }

    private var workspacePill: some View {
        Button(action: onWorkspaceTap) {
            HStack(spacing: 8) {
                workspaceAvatar
                Text(workspace?.name ?? "Workspace")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.leading, 6)
            .padding(.trailing, 12)
            .padding(.vertical, 6)
            .background(.ultraThinMaterial)
            .clipShape(Capsule())
            .overlay(
                Capsule().stroke(Color.white.opacity(0.1), lineWidth: 0.5)
            )
            .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
        }
        .buttonStyle(.plain)
    }

    private var workspaceAvatar: some View {
        let initial = (workspace?.name.prefix(1).uppercased()) ?? "?"
        return Text(initial)
            .font(.caption.weight(.bold))
            .foregroundStyle(.white)
            .frame(width: 22, height: 22)
            .background(Color.blue.opacity(0.6))
            .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}
