import Foundation

/// URL linkification for agent-feed narration text (EXP-430).
///
/// Narration used to render as plain text, which made the claude sign-in URL
/// the emitter publishes during a remote `/login` untappable. A deliberately
/// tiny tokenizer, not a markdown pass — hand-mirrored with the web
/// (`lib/linkify.ts`) and Android (`Linkify.kt`) implementations.
/// Foundation-only on purpose (ShareExtension source-copy rule).

public struct LinkSegment: Equatable {
    public let text: String
    /// Present iff this segment renders as a link.
    public let href: String?

    public init(text: String, href: String? = nil) {
        self.text = text
        self.href = href
    }
}

private let urlPattern = try! NSRegularExpression(pattern: #"https?://\S+"#)

private let trailingProse: Set<Character> = [".", ",", ";", ":", "!", "?"]

/// Trailing punctuation is prose, not URL — `(see https://x.dev).` must not
/// link the `).`. A BALANCED closing paren/bracket is kept, so
/// `https://x.dev/a(b)` stays whole.
private func trimTrailingPunctuation(_ url: String) -> String {
    var out = url
    while let last = out.last {
        if trailingProse.contains(last) {
            out.removeLast()
        } else if last == ")",
            out.filter({ $0 == "(" }).count < out.filter({ $0 == ")" }).count
        {
            out.removeLast()
        } else if last == "]",
            out.filter({ $0 == "[" }).count < out.filter({ $0 == "]" }).count
        {
            out.removeLast()
        } else {
            break
        }
    }
    return out
}

/// Split `text` into plain/link segments; concatenating the segments' text
/// always reproduces the input.
public func linkSegments(_ text: String) -> [LinkSegment] {
    var segments: [LinkSegment] = []
    let ns = text as NSString
    var cursor = 0
    let matches = urlPattern.matches(in: text, range: NSRange(location: 0, length: ns.length))
    for match in matches {
        let url = trimTrailingPunctuation(ns.substring(with: match.range))
        if url.isEmpty { continue }
        if match.range.location > cursor {
            segments.append(
                LinkSegment(
                    text: ns.substring(
                        with: NSRange(location: cursor, length: match.range.location - cursor))))
        }
        segments.append(LinkSegment(text: url, href: url))
        cursor = match.range.location + (url as NSString).length
    }
    if cursor < ns.length || segments.isEmpty {
        segments.append(
            LinkSegment(text: ns.substring(from: cursor)))
    }
    return segments
}

/// Narration text with its URLs carrying the `.link` attribute — SwiftUI's
/// `Text` renders those runs tappable in the accent color.
public func linkifiedNarration(_ text: String) -> AttributedString {
    var out = AttributedString()
    for segment in linkSegments(text) {
        var run = AttributedString(segment.text)
        if let href = segment.href, let url = URL(string: href) {
            // `.link` only — underline lives in the SwiftUI attribute scope,
            // and this file must stay Foundation-only.
            run.link = url
        }
        out += run
    }
    return out
}
