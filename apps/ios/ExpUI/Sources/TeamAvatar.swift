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

    /// EXP-698 r5: the WHITE chip, black initial — the same team mark web,
    /// iOS and Android draw (`bg-primary text-primary-foreground`). It used to
    /// be a `fillActive` wash, which vanished against the glass rows it sits
    /// on and read as a different avatar to the web sidebar's.
    private var initialsChip: some View {
        Text(team.name.prefix(1).uppercased())
            .font(.caption.weight(.bold))
            .foregroundStyle(DesignTokens.Palette.primaryForeground)
            .frame(width: size, height: size)
            .background(DesignTokens.Palette.primary)
    }
}
