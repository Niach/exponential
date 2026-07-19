import ExpCore
import SwiftUI

public struct TeamAvatar: View {
    let team: TeamEntity
    var size: CGFloat = 24

    public init(team: TeamEntity, size: CGFloat = 24) {
        self.team = team
        self.size = size
    }

    public var body: some View {
        Group {
            if let urlString = team.iconUrl,
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
        Text(team.name.prefix(1).uppercased())
            .font(.caption.weight(.bold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Accent.indigo.opacity(0.6))
    }
}
