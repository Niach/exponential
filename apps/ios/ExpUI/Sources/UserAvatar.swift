import ExpCore
import SwiftUI
internal import ExponentialUIPrimitives

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
    let image: String?
    let initials: String
    /// #EXP-1383: set = an AGENT where a person would be (an actor, an
    /// assignee): THE AGENT MARK, never initials.
    var agent: UserAvatarAgent?
    /// What the fallback hue is keyed on — the synced row's id when there is
    /// one, the raw id otherwise; both are the same string, so a member whose
    /// row lands mid-session keeps its colour.
    let hueKey: String?
    var size: CGFloat = 32

    public init(user: UserEntity?, id: String?, size: CGFloat = 32) {
        self.init(
            image: user?.image,
            initials: memberInitials(user, id: id),
            hueKey: user?.id ?? id,
            size: size
        )
    }

    /// EXP-1021: the same avatar from LOOSE fields — the shared assignee
    /// picker's rows carry a member's name/email/image, never a `users` row.
    public init(image: String?, initials: String, hueKey: String?, size: CGFloat = 32) {
        self.image = image
        self.initials = initials
        self.hueKey = hueKey
        self.size = size
    }

    /// #EXP-1383 (agent-settings.json `actor` / `assignee`): THE AGENT MARK
    /// at the avatar's size — the agent's glyph in its colour on the muted
    /// disc, the Chats agent bubbles' and Settings → Agents rows' mark.
    public init(agent: UserAvatarAgent, size: CGFloat = 32) {
        self.image = nil
        self.initials = ""
        self.hueKey = nil
        self.agent = agent
        self.size = size
    }

    /// SLOP-18 / VAPP-88: the drawing is the SDK's `AvatarView`, fed ExpUI's
    /// own hue (`avatarHueIndex`) and initials so no avatar changes colour.
    public var body: some View {
        if let agent {
            ZStack {
                Circle().fill(DesignTokens.Palette.muted)
                AppIcon(agent.iconName, size: size * UserAvatarTokens.agentGlyphScale, weight: .medium)
                    .foregroundStyle(agent.tint)
            }
            .frame(width: size, height: size)
            .accessibilityHidden(true)
        } else if let urlString = image,
           !urlString.isEmpty,
           let url = URL(string: urlString) {
            AvatarView(name: initials, size: size) {
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

    // The glyph has to scale with the avatar. A fixed .caption is wider than
    // the 16pt chip avatars on the issue detail, so SwiftUI truncated two
    // initials to a lone "…" — visible in the App Store screenshots
    // (EXP-393). 0.42 of the diameter matches the Android InitialsAvatar.
    private var initialsChip: some View {
        AvatarView(name: initials, size: size, fill: Self.fill(hueKey), ink: Self.ink(hueKey), fontSize: size * UserAvatarTokens.initialsScale, initials: initials, minimumScaleFactor: UserAvatarTokens.minimumScaleFactor)
    }

    /// The initials' colour: the avatar hue picked off the key.
    static func ink(_ hueKey: String?) -> Color {
        DesignTokens.Avatar.hues[avatarHueIndex(hueKey)]
    }

    /// The chip's fill: the hue at 20 %.
    static func fill(_ hueKey: String?) -> Color {
        ink(hueKey).opacity(UserAvatarTokens.fillOpacity)
    }
}

/// The pinned fallback-chip numbers of `UserAvatar`.
public enum UserAvatarTokens {
    /// The initials' size as a share of the diameter.
    public static let initialsScale: CGFloat = 0.42
    /// How far two initials shrink before they truncate.
    public static let minimumScaleFactor: CGFloat = 0.6
    /// The hue's alpha in the chip fill.
    public static let fillOpacity: Double = 0.2
    /// #EXP-1383: the agent glyph's size as a share of the diameter (web
    /// `USER_AVATAR_SIZES[*].glyph`: 12 of 20, 16 of 28).
    public static let agentGlyphScale: CGFloat = 0.6
}

/// #EXP-1383: an agent where a person would be — its curated `icon` and
/// `color` (an `agents` row), drawn as THE AGENT MARK.
public struct UserAvatarAgent: Equatable, Sendable {
    public let icon: String?
    public let color: String?

    public init(icon: String?, color: String?) {
        self.icon = icon
        self.color = color
    }

    public init(_ agent: AgentDefinitionEntity) {
        self.init(icon: agent.icon, color: agent.color)
    }

    public init(_ agent: AgentSettings.ActorAgent) {
        self.init(icon: agent.icon, color: agent.color)
    }

    /// The curated glyph, else the contract default (`bot`).
    public var iconName: String {
        BoardTypeDisplay.iconName(for: icon) ?? SessionTree.ListCopy.agentDefaultIcon
    }

    /// The agent's colour, else the contract default.
    public var tint: Color {
        Color(hex: color ?? SessionTree.ListCopy.agentDefaultColor)
            ?? Color(hex: SessionTree.ListCopy.agentDefaultColor) ?? .gray
    }
}
