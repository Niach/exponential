import Foundation

/// EXP-1537: what a send leaves in THE composer. Pure and locked by
/// `packages/domain-contract/fixtures/composer-send.json` (`cases`), mirrored
/// from `src/composer-send.ts`. The field stays editable while a send is in
/// flight; on success the text clears ONLY if the field still equals what was
/// sent byte for byte, and only the attachments that went out leave; on
/// failure everything stays as it is now.
public enum ComposerSend {
    public enum Outcome: String, Equatable, Sendable {
        case sent
        case failed
    }

    /// The field's text and its attachments' ids (a client's own key per
    /// picked file).
    public struct Content: Equatable, Sendable {
        public var text: String
        public var attachments: [String]

        public init(text: String, attachments: [String] = []) {
            self.text = text
            self.attachments = attachments
        }
    }

    /// - Parameters:
    ///   - sent: the field when Send was pressed (UNTRIMMED) and what went out.
    ///   - current: the field when the send settled.
    public static func afterSend(outcome: Outcome, sent: Content, current: Content) -> Content {
        guard outcome == .sent else { return current }
        let gone = Set(sent.attachments)
        return Content(
            text: textAfterSend(sent: sent.text, current: current.text),
            attachments: current.attachments.filter { !gone.contains($0) }
        )
    }

    /// The text half alone (a composer without attachments). Swift `==` on
    /// String compares canonically equivalent Unicode, so the bytes decide.
    public static func textAfterSend(sent: String, current: String) -> String {
        current.utf8.elementsEqual(sent.utf8) ? "" : current
    }
}
