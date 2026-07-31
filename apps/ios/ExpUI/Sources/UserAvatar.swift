import ExpCore
import SwiftUI

/// Circular member avatar. Mirrors `TeamAvatar` but is user-based and round:
/// renders `user.image` when set (Google-login photos), falling back to an
/// initials chip derived from the display name / email (`memberInitials`). The
/// initials chip also stands in while an async image loads or fails.
public struct UserAvatar: View {
    let user: UserEntity?
    let id: String?
    var size: CGFloat = 32

    public init(user: UserEntity?, id: String?, size: CGFloat = 32) {
        self.user = user
        self.id = id
        self.size = size
    }

    public var body: some View {
        Group {
            if let urlString = user?.image,
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
        .clipShape(Circle())
    }

    private var initialsChip: some View {
        // The glyph has to scale with the avatar. A fixed .caption is wider than
        // the 16pt chip avatars on the issue detail, so SwiftUI truncated two
        // initials to a lone "…" — visible in the App Store screenshots
        // (EXP-393). 0.42 of the diameter matches the Android InitialsAvatar.
        Text(memberInitials(user, id: id))
            .font(.system(size: size * 0.42, weight: .medium))
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Color.white.opacity(0.15))
    }
}
