import ExpCore
import SwiftUI

/// Circular member avatar. Mirrors `TeamAvatar` but is user-based and round:
/// renders `user.image` when set (Google-login photos), falling back to an
/// initials chip derived from the display name / email (`memberInitials`). The
/// initials chip also stands in while an async image loads or fails.
///
/// EXP-698 r4: the fallback chip is no longer one grey wash for everybody — it
/// takes a hue from `DesignTokens.Avatar.hues`, picked by `avatarHueIndex` off
/// the user id, so a picture-less team reads as distinct people on all four
/// clients. Fill is that hue at 20 %, the initials the hue at full alpha, and
/// there is no hairline: the colour IS the chrome.
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
        // The hue is keyed on the id the caller knows the person by — the
        // synced row's id when there is one, the raw id otherwise; both are the
        // same string, so a member whose row lands mid-session keeps its colour.
        let hue = DesignTokens.Avatar.hues[avatarHueIndex(user?.id ?? id)]
        // The glyph has to scale with the avatar. A fixed .caption is wider than
        // the 16pt chip avatars on the issue detail, so SwiftUI truncated two
        // initials to a lone "…" — visible in the App Store screenshots
        // (EXP-393). 0.42 of the diameter matches the Android InitialsAvatar.
        return Text(memberInitials(user, id: id))
            .font(.system(size: size * 0.42, weight: .medium))
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .foregroundStyle(hue)
            .frame(width: size, height: size)
            .background(hue.opacity(0.2))
    }
}
