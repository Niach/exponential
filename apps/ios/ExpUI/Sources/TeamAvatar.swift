import ExpCore
import SwiftUI
internal import ExponentialUIPrimitives

public struct TeamAvatar: View {
    let team: TeamEntity
    var size: CGFloat = 24

    public init(team: TeamEntity, size: CGFloat = 24) {
        self.team = team
        self.size = size
    }

    /// SLOP-18 / VAPP-88: the drawing is the SDK's `AvatarView`, clipped to
    /// the team mark's rounded square (radius a quarter of the size).
    public var body: some View {
        if let urlString = team.iconUrl,
           !urlString.isEmpty,
           let url = URL(string: urlString) {
            AvatarView(name: team.name, size: size, initials: initial, cornerRadius: size / 4) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case let .success(image):
                        image.resizable().scaledToFill()
                    default:
                        initialsChip
                    }
                }
            }
        } else {
            initialsChip
        }
    }

    private var initial: String { team.name.prefix(1).uppercased() }

    /// EXP-698 r5: the WHITE chip, black initial — the same team mark web,
    /// iOS and Android draw (`bg-primary text-primary-foreground`). It used to
    /// be a `fillActive` wash, which vanished against the glass rows it sits
    /// on and read as a different avatar to the web sidebar's.
    private var initialsChip: some View {
        AvatarView(
            name: team.name,
            size: size,
            fill: DesignTokens.Palette.primary,
            ink: DesignTokens.Palette.primaryForeground,
            initials: initial,
            cornerRadius: size / 4,
            font: .caption.weight(.bold),
            minimumScaleFactor: 1
        )
    }
}
