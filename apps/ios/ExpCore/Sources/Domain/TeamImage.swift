import Foundation

/// #EXP-1645 (packages/domain-contract/fixtures/team-image.json): THE team mark
/// and the team image. Replays the TS reference
/// `@exp/domain-contract/team-image` (desktop `domain::team_image`, Android
/// `TeamImage.kt`).
public enum TeamImage {
    public enum Copy {
        public static let row = "Team image"
        public static let upload = "Upload"
        public static let remove = "Remove image"
    }

    public enum TestIds {
        public static let row = "team-image"
        public static let upload = "team-image-upload"
        public static let remove = "team-image-remove"
    }

    /// `mark`: the image's corner radius as a fraction of its side, and the
    /// glyph concept drawn without one.
    public enum MarkStyle {
        public static let radiusFraction: Double = 0.25
        public static let glyph = "ui-team"
    }

    public enum Row {
        public static let markSize: Double = 28
    }

    public enum Upload {
        public static let path = "/api/teams/{teamId}/icon"
        public static let maxBytes = 10_485_760
        public static let side = 512
        public static let accept = ["image/png", "image/jpeg", "image/webp", "image/gif", "image/svg+xml"]
    }

    public enum Mark: Equatable, Sendable {
        case image(src: String)
        case glyph
    }

    /// The team's image when `icon_url` holds one, else the group glyph.
    public static func mark(iconUrl: String?) -> Mark {
        guard let src = iconUrl?.trimmingCharacters(in: .whitespacesAndNewlines), !src.isEmpty else {
            return .glyph
        }
        return .image(src: src)
    }

    /// The upload route of a team.
    public static func uploadPath(teamId: String) -> String {
        Upload.path.replacingOccurrences(of: "{teamId}", with: teamId)
    }
}
