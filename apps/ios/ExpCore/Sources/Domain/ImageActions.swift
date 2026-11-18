import Foundation

/// #EXP-1926 / #EXP-1949 (packages/domain-contract/fixtures/image-actions.json,
/// TS `src/image-actions.ts`, Android `ImageActions.kt`): "Copy image" + Share
/// on every content picture, and which picture sources load at all.
/// `ImageActionsTests` replays `menuCases`, `pressCases`, `encodingCases`,
/// `shareNameCases` and `sourceCases`.
public enum ImageActions {
    public enum Copy {
        public static let copyImage = DomainContract.imageActionsCopyImage
        public static let share = DomainContract.imageActionsShare
        public static let copied = DomainContract.imageActionsCopied
        public static let copyFailed = DomainContract.imageActionsCopyFailed
        public static let shareFailed = DomainContract.imageActionsShareFailed
        public static let linkHost = DomainContract.imageActionsLinkHost
    }

    public enum Icons {
        public static let copyImage = "ui-copy"
        public static let share = "ui-share"
    }

    public enum TestIds {
        public static let copyImage = "imageCopy"
        public static let share = "imageShare"
    }

    public enum Input: String, Sendable { case pointer, touch }
    public enum Item: String, Sendable { case copyImage, share }
    public enum PressMenu: String, Sendable { case image, owner, none }
    public enum Encoding: String, Sendable { case passthrough, reencode }

    /// What a picture's src renders as: `load` (ours, data or local), a `link`
    /// (another website: never fetched) or plain `text` (the alt, maybe empty).
    public enum Source: Equatable, Sendable {
        case load
        case link(href: String, text: String)
        case text(String)
    }

    /// The menu's items in order: pointer = Copy image; touch = Copy image
    /// then Share (only when a file can be shared).
    public static func imageMenu(_ input: Input, canShareFile: Bool) -> [Item] {
        switch input {
        case .pointer: [.copyImage]
        case .touch: canShareFile ? [.copyImage, .share] : [.copyImage]
        }
    }

    /// Which menu a press opens: the image's own on a rendered picture, else
    /// the element's around it, else none. Never merged.
    public static func pressMenu(onImage: Bool, imageReady: Bool, ownerMenu: String?) -> PressMenu {
        if onImage && imageReady { return .image }
        return ownerMenu == nil ? .none : .owner
    }

    static let pngSignature: [UInt8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]

    /// PNG bytes go as they are; anything else is decoded and re-encoded to
    /// PNG (an animation = its first frame).
    public static func copyEncoding(_ head: Data) -> Encoding {
        guard head.count >= pngSignature.count else { return .reencode }
        return Array(head.prefix(pngSignature.count)) == pngSignature ? .passthrough : .reencode
    }

    /// The shared PNG's file name: the last extension becomes `.png`, path
    /// separators `-`, an empty stem (or no name) = `image.png`.
    public static func shareFileName(_ name: String?) -> String {
        let clean = trimAscii(name ?? "").replacingOccurrences(of: "/", with: "-").replacingOccurrences(of: "\\", with: "-")
        let scalars = Array(clean.unicodeScalars)
        let dot = scalars.lastIndex(of: ".")
        let stemScalars = dot.map { $0 > 0 ? Array(scalars[..<$0]) : scalars } ?? scalars
        let stem = trimAscii(String(String.UnicodeScalarView(stemScalars)))
        return "\(stem.isEmpty ? "image" : stem).png"
    }

    private static let attachmentPath = "/api/attachments/"
    private static let uuid = "[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
    // swiftlint:disable:next force_try
    private static let attachmentRe = try! NSRegularExpression(pattern: "^/api/attachments/(\(uuid))(?:\\?[^#]*)?(?:#.*)?$")
    private static let localSchemes = ["blob:", "app-file:"]
    private static let linkSchemes = ["https:", "http:"]
    private static let edgeSpace: Set<Unicode.Scalar> = [" ", "\t", "\n", "\u{0C}", "\r"]

    /// Surrounding ASCII whitespace (space, tab, LF, FF, CR) trimmed, by scalar.
    static func trimAscii(_ s: String) -> String {
        var v = Substring(s).unicodeScalars
        while let f = v.first, edgeSpace.contains(f) { v.removeFirst() }
        while let l = v.last, edgeSpace.contains(l) { v.removeLast() }
        return String(v)
    }

    private static func attachmentMatch(_ path: String) -> String? {
        let range = NSRange(path.startIndex..., in: path)
        guard let m = attachmentRe.firstMatch(in: path, range: range), let r = Range(m.range(at: 1), in: path) else { return nil }
        return String(path[r])
    }

    private static func isLoadableData(_ lower: String) -> Bool {
        guard let comma = lower.firstIndex(of: ",") else { return false }
        let mime = lower[lower.index(lower.startIndex, offsetBy: 5)..<comma].split(separator: ";", omittingEmptySubsequences: false).first.map(String.init) ?? ""
        return mime.hasPrefix("image/") && mime.count > "image/".count && mime != "image/svg+xml"
    }

    /// The label host of a `scheme://` or `//` url: lowercased, userinfo and
    /// port dropped, IPv6 brackets kept; empty = none.
    private static func linkHost(_ afterSlashes: String) -> String {
        var authority = afterSlashes.firstIndex { "/?#\\".contains($0) }.map { String(afterSlashes[..<$0]) } ?? afterSlashes
        if let at = authority.lastIndex(of: "@") { authority = String(authority[authority.index(after: at)...]) }
        let close = authority.lastIndex(of: "]")
        if let colon = authority.lastIndex(of: ":"), close.map({ colon > $0 }) ?? true { authority = String(authority[..<colon]) }
        return authority.lowercased()
    }

    /// The src with its origin prefix removed when it starts with `origin/`
    /// (scheme + host compare case-insensitively), else nil.
    private static func path(_ s: String, onOrigin origin: String) -> String? {
        var base = origin
        while base.hasSuffix("/") { base.removeLast() }
        guard !base.isEmpty, s.lowercased().hasPrefix(base.lowercased() + "/") else { return nil }
        return String(s.unicodeScalars.dropFirst(base.unicodeScalars.count))
    }

    /// What a picture's src renders as against the viewer's instance `origin`.
    public static func imageSource(_ src: String, alt: String, origin: String) -> Source {
        let altText = trimAscii(alt)
        let plain = Source.text(altText)
        let s = trimAscii(src)
        if s.isEmpty || s.unicodeScalars.contains(where: { $0.value <= 0x20 || $0.value == 0x7f }) { return plain }
        let lower = s.lowercased()
        if lower.hasPrefix("data:") { return isLoadableData(lower) ? .load : plain }
        if localSchemes.contains(where: { lower.hasPrefix($0) }) { return .load }
        if s.hasPrefix("/") && !s.hasPrefix("//") { return attachmentMatch(s) != nil ? .load : plain }
        if let p = path(s, onOrigin: origin), attachmentMatch(p) != nil { return .load }
        let href: String
        let rest: String
        if s.hasPrefix("//") {
            href = "https:\(s)"
            rest = String(s.dropFirst(2))
        } else {
            guard let scheme = linkSchemes.first(where: { lower.hasPrefix("\($0)//") }) else { return plain }
            href = s
            rest = String(s.dropFirst(scheme.count + 2))
        }
        let host = linkHost(rest)
        if host.isEmpty { return plain }
        return .link(href: href, text: altText.isEmpty ? Copy.linkHost.replacingOccurrences(of: "{host}", with: host) : altText)
    }

    /// True when the picture may be fetched at all.
    public static func imageSourceAllowed(_ src: String, origin: String) -> Bool {
        imageSource(src, alt: "", origin: origin) == .load
    }

    /// The attachment id of an `/api/attachments/<uuid>` src (relative or on
    /// `origin`); nil = not ours.
    public static func attachmentId(_ src: String, origin: String) -> String? {
        let s = trimAscii(src)
        if s.hasPrefix("/") && !s.hasPrefix("//") { return attachmentMatch(s) }
        return path(s, onOrigin: origin).flatMap(attachmentMatch)
    }

    /// `src` without the thumbnail width `w` in its query
    /// (`copyFormat.downscale: false`); a `data:` src as it is.
    public static func originalSrc(_ src: String) -> String {
        if src.lowercased().hasPrefix("data:") { return src }
        guard let q = src.firstIndex(of: "?") else { return src }
        let hash = src[q...].firstIndex(of: "#") ?? src.endIndex
        let kept = src[src.index(after: q)..<hash].split(separator: "&").filter { $0 != "w" && !$0.hasPrefix("w=") }
        return String(src[..<q]) + (kept.isEmpty ? "" : "?" + kept.joined(separator: "&")) + String(src[hash...])
    }
}
