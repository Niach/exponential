import ExpCore
import Foundation

/// EXP-824 — what the renderer needs to know about a synced attachment row to
/// upgrade a `[label](/api/attachments/{id})` block: the content type decides
/// video / audio / plain link, the dimensions size the aspect box, the length
/// feeds the duration chip and `hasPoster` says whether `?poster=1` exists.
/// Fed to `IssueEditorModel.attachmentResolver` by the host (issue detail
/// observes the rows live; comment cards and agent renders read the store).
public struct AttachmentMediaInfo: Equatable, Sendable {
    public let contentType: String
    public let width: Int?
    public let height: Int?
    public let durationMs: Int?
    public let hasPoster: Bool

    public init(contentType: String, width: Int?, height: Int?, durationMs: Int?, hasPoster: Bool) {
        self.contentType = contentType
        self.width = width
        self.height = height
        self.durationMs = durationMs
        self.hasPoster = hasPoster
    }

    public init(_ row: AttachmentEntity) {
        self.init(
            contentType: row.contentType,
            width: row.width,
            height: row.height,
            durationMs: row.durationMs,
            hasPoster: row.hasPoster
        )
    }

    public var isVideo: Bool { AttachmentFiles.isInlineVideo(contentType: contentType) }
    public var isAudio: Bool { AttachmentFiles.isInlineAudio(contentType: contentType) }

    /// Width / height for the aspect box; 16:9 when the row has no size.
    public var aspectRatio: Double {
        guard let width, let height, width > 0, height > 0 else { return 16.0 / 9.0 }
        return Double(width) / Double(height)
    }
}

/// EXP-824 — the attachment-link half of the markdown contract. Inline media
/// is stored as a PLAIN LINK on its own paragraph, `[clip.mp4](/api/attachments/{id})`
/// (never the image form), and only a paragraph consisting solely of one
/// such link becomes a media block; the same link inside running text stays
/// an ordinary link. Which links count is decided here, once, for the parser
/// and the renderer alike.
public enum AttachmentLinks {
    public static let pathPrefix = "/api/attachments/"

    /// The attachment id a stored URL references, or nil when it is not an
    /// attachment URL: the relative `/api/attachments/{id}` form, or an
    /// absolute URL on the SAME origin as `baseURL` (a foreign host's
    /// `/api/attachments/…` path is never ours). A `?w=480`-style query is
    /// ignored like it is on images; ids are lowercased like the server does.
    public static func attachmentId(fromUrl urlString: String, baseURL: URL?) -> String? {
        if MarkdownImageUtils.isDraft(urlString) { return nil }
        let path: String
        if let url = URL(string: urlString), url.scheme != nil {
            guard isSameOrigin(url, as: baseURL) else { return nil }
            path = url.path
        } else {
            path = String(urlString.prefix { $0 != "?" && $0 != "#" })
        }
        guard path.hasPrefix(pathPrefix) else { return nil }
        let id = path.dropFirst(pathPrefix.count)
        guard !id.isEmpty, !id.contains("/") else { return nil }
        return String(id).lowercased()
    }

    /// Whether a sole-link paragraph with this URL becomes an attachment-link
    /// BLOCK: a resolvable attachment URL, or an in-editor `draft://` upload
    /// placeholder (those never reach a saved body — see
    /// `MarkdownImageUtils.stripUnknownDraftLinks`).
    public static func isBlockURL(_ urlString: String, baseURL: URL?) -> Bool {
        MarkdownImageUtils.isDraft(urlString) || attachmentId(fromUrl: urlString, baseURL: baseURL) != nil
    }

    /// The poster frame's stored-form URL for an attachment id (relative, so
    /// the image loader resolves + authenticates it like any attachment).
    public static func posterUrl(attachmentId: String) -> String {
        "\(pathPrefix)\(attachmentId)?poster=1"
    }

    /// Link-text escaping for the serializer: `[`, `]` and `\` are the only
    /// characters that would change how cmark reads the label back (a
    /// filename with brackets), and a label with any OTHER markdown inside
    /// never becomes a block in the first place.
    public static func escapeLabel(_ label: String) -> String {
        guard label.contains(where: { $0 == "[" || $0 == "]" || $0 == "\\" }) else { return label }
        var out = ""
        out.reserveCapacity(label.count + 4)
        for character in label {
            if character == "[" || character == "]" || character == "\\" { out.append("\\") }
            out.append(character)
        }
        return out
    }

    // Mirrors the app's `AttachmentURL.isSameOrigin` (scheme + host + effective
    // port) — parse time has no access to the app target.
    static func isSameOrigin(_ url: URL, as base: URL?) -> Bool {
        guard let base,
              let scheme = url.scheme?.lowercased(),
              scheme == base.scheme?.lowercased(),
              let host = url.host?.lowercased(),
              host == base.host?.lowercased()
        else { return false }
        return effectivePort(of: url) == effectivePort(of: base)
    }

    private static func effectivePort(of url: URL) -> Int? {
        if let port = url.port { return port }
        switch url.scheme?.lowercased() {
        case "https": return 443
        case "http": return 80
        default: return nil
        }
    }
}
