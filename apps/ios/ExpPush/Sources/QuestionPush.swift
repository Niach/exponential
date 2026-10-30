import Foundation

// #VAPP-45 (packages/domain-contract/fixtures/question-push.json, its
// `$comment` is the spec; TS reference src/question-push.ts): the push of a
// question or plan card carries the question TEXT, sealed to the session tree
// (`data.sealedText`, `data.rootId`). The Notification Service Extension opens
// it with the tree secret the app keeps in the shared keychain (TreeKeyStore)
// and shows THE chat ask preview as the body; anything it cannot open keeps
// the push's own body. The preview is ExpCore `Chats.askPreview`'s ask-mirror
// branch + `plainLine`, mirrored here because the extension links no ExpCore
// (no GRDB in ~24 MB); `textCases` lock both.
public enum QuestionPush {
    /// The shown body's cap in code points (longer = cut, ending in `…`).
    public static let bodyMax = 200
    public static let sealedTextKey = "sealedText"
    public static let rootIdKey = "rootId"

    private static let questionCopy = "Question: {text}"
    private static let planCopy = "Plan: {text}"
    private static let askQuestionLead = "Asks: "
    private static let askPlanLead = "Proposes a plan:"

    /// `text` cut to `bodyMax` code points (`…` last).
    public static func clip(_ text: String) -> String {
        let scalars = text.unicodeScalars
        guard scalars.count > bodyMax else { return text }
        var out = String.UnicodeScalarView(scalars.prefix(bodyMax - 1))
        out.append("…")
        return String(out)
    }

    /// The shown body of an ask text (the bus's `askText`): THE chat ask
    /// preview of an `ask` mirror with that body, clipped.
    public static func text(askText: String) -> String {
        let body = askText.trimmingCharacters(in: .whitespacesAndNewlines)
        if body.hasPrefix(askPlanLead) {
            let rest = String(body.dropFirst(askPlanLead.count)).trimmingCharacters(in: .whitespacesAndNewlines)
            let block = rest.range(of: #"\n\s*\n"#, options: .regularExpression)
                .map { String(rest[..<$0.lowerBound]) } ?? rest
            return clip(fill(planCopy, plainLine(block)))
        }
        let first = body.components(separatedBy: "\n").first(where: { !plainLine($0).isEmpty }) ?? ""
        let question = first.hasPrefix(askQuestionLead) ? String(first.dropFirst(askQuestionLead.count)) : first
        return clip(fill(questionCopy, plainLine(question)))
    }

    /// A delivered push's body: `data.sealedText` opened with the tree
    /// secrets of `data.rootId` (`keysFor`: epoch → 32 bytes, nil = none
    /// held), else `fallback` (the push's own body).
    public static func body(
        data: [AnyHashable: Any],
        keysFor: (String) -> [UInt32: Data]?,
        fallback: String
    ) -> String {
        guard let sealed = data[sealedTextKey] as? String, !sealed.isEmpty,
              let rootId = data[rootIdKey] as? String, !rootId.isEmpty,
              let keys = keysFor(rootId), !keys.isEmpty,
              let opened = open(sealed, keys: keys) else { return fallback }
        let shown = text(askText: opened)
        return shown.trimmingCharacters(in: .whitespaces).isEmpty ? fallback : shown
    }

    /// The opened text of a body envelope, nil on any status but `opened`.
    static func open(_ envelope: String, keys: [UInt32: Data]) -> String? {
        #if ENET_PUSH
        let epochKeys = keys.map { EnetEpochKey(epoch: $0.key, secret: $0.value) }
        guard let opened = try? openBody(body: envelope, keys: epochKeys), opened.status == .opened else { return nil }
        return opened.text
        #else
        return nil
        #endif
    }

    private static func fill(_ template: String, _ text: String) -> String {
        template.replacingOccurrences(of: "{text}", with: text)
    }

    private static func replace(_ text: String, _ pattern: String, _ template: String) -> String {
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return text }
        return regex.stringByReplacingMatches(
            in: text, range: NSRange(text.startIndex..., in: text), withTemplate: template
        )
    }

    /// Markdown → one plain line (ExpCore `Chats.plainLine`).
    static func plainLine(_ markdown: String) -> String {
        var text = replace(markdown, #"!\[[^\]]*\]\([^)]*\)"#, " ")
        text = replace(text, #"\[([^\]]*)\]\([^)]*\)"#, "$1")
        text = text.components(separatedBy: "\n")
            .map { replace($0, #"^\s*(?:#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+)"#, "") }
            .joined(separator: " ")
        text = replace(text, #"\*\*|__|~~|`"#, "")
        text = replace(text, #"(^|\s)[*_](\S)"#, "$1$2")
        text = replace(text, #"(\S)[*_](?=\s|$)"#, "$1")
        text = replace(text, #"\s+"#, " ")
        return text.trimmingCharacters(in: .whitespaces)
    }
}

extension TreeKeyStore {
    /// The held secrets of one tree by epoch, nil when none.
    public func keys(rootId: String) -> [UInt32: Data]? {
        load()[rootId.lowercased()]
    }
}
