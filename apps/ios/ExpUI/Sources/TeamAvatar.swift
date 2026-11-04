import ExpCore
import SwiftUI

/// #EXP-1645: how a team image is fetched. `ExpUI` cannot reach the app's
/// signed-in loader (a relative `/api/attachments/{id}` needs the instance and
/// the bearer), so the app's root injects it through the environment. Without
/// one a mark stays on its glyph.
public struct TeamImageLoader: Sendable {
    /// `(src, accountId)`, synchronously from the decoded-image cache.
    public var cached: @MainActor @Sendable (String, String) -> UIImage?
    /// `(src, accountId)`; nil = failed.
    public var load: @MainActor @Sendable (String, String) async -> UIImage?

    public init(
        cached: @escaping @MainActor @Sendable (String, String) -> UIImage?,
        load: @escaping @MainActor @Sendable (String, String) async -> UIImage?
    ) {
        self.cached = cached
        self.load = load
    }
}

extension EnvironmentValues {
    @Entry public var teamImageLoader: TeamImageLoader?
}

/// #EXP-1645 (fixtures/team-image.json), THE team mark ×4 (web `team-avatar.tsx`,
/// Android `Avatars.kt`, desktop `team_avatar`): the team's image in a square of
/// `size`, cover, clipped at `mark.radiusFraction` of the side, no border and no
/// fill behind it (a transparent image stays transparent); no image, loading or
/// failed = the group glyph at the side in the surrounding text colour.
public struct TeamAvatar: View {
    let iconUrl: String?
    var size: CGFloat = 24

    @Environment(\.teamImageLoader) private var loader
    @Environment(\.accountId) private var accountId
    /// The last image this mark fetched, with the `src` it belongs to.
    @State private var loaded: (src: String, image: UIImage)?

    public init(iconUrl: String?, size: CGFloat = 24) {
        self.iconUrl = iconUrl
        self.size = size
    }

    private var src: String? {
        if case let .image(src) = TeamImage.mark(iconUrl: iconUrl) { return src }
        return nil
    }

    private var image: UIImage? {
        guard let src else { return nil }
        if let loaded, loaded.src == src { return loaded.image }
        return loader?.cached(src, accountId)
    }

    public var body: some View {
        Group {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
                    .frame(width: size, height: size)
                    .clipShape(RoundedRectangle(
                        cornerRadius: size * CGFloat(TeamImage.MarkStyle.radiusFraction),
                        style: .continuous
                    ))
            } else {
                AppIcon(AppIcons.uiTeam, size: size)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
        .task(id: "\(accountId)/\(src ?? "")") {
            guard let src, let loader, image == nil else { return }
            if let fetched = await loader.load(src, accountId) {
                loaded = (src, fetched)
            }
        }
    }
}
