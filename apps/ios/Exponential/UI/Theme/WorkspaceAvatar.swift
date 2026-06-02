import ExpCore
import SwiftUI

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
