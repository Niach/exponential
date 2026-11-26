import Foundation

/// #NEW-146: THE elicitation card ×4 (`packages/domain-contract/fixtures/
/// elicitation-card.json`, TS reference `src/elicitation-card.ts`; desktop
/// `domain::elicitation_card`, Android `domain/ElicitationCard.kt`): a
/// THIRD-PARTY MCP server's elicitation as the question card. The device
/// builds the card (form surface or URL review); the renderers replay
/// `missingCases` (Submit waits for the required fields) and `actionCases`
/// (what an answer carries back). `ElicitationCardTests` pins the constants.
public enum ElicitationCard {
    public enum Copy {
        public static let submit = "Submit"
        public static let decline = "Decline"
        public static let cancel = "Cancel"
        public static let openLink = "Open link"
        public static let done = "I'm done"
        public static let missing = "Fill in every required field."
        public static let unknownServer = "MCP server"

        /// `copy.specimen`: the specimen's four captions ×4.
        public enum Specimen {
            public static let form = "Form"
            public static let url = "URL"
            public static let warned = "URL with warnings"
            public static let answered = "Answered"
        }
    }

    /// Concept icons (`icons.json`).
    public enum Icons {
        public static let server = "settings-mcp"
        public static let url = "globe"
    }

    public enum TestIds {
        public static let card = "elicitation-card"
        public static let server = "elicitation-server"
        public static let submit = "elicitation-submit"
        public static let decline = "elicitation-decline"
        public static let cancel = "elicitation-cancel"
        public static let openLink = "elicitation-open-link"
        public static let done = "elicitation-done"
    }

    /// The surface data's form object key.
    public static let formKey = "form"

    public enum Mode: String, Equatable, Sendable {
        case form, url
    }

    /// The question card's `elicitation` sidecar: who asks (the configured
    /// server name), form or url, the required field names, the url.
    public struct Sidecar: Equatable, Sendable {
        public let server: String
        public let mode: Mode
        public let required: [String]
        public let url: String?

        public init(server: String, mode: Mode, required: [String] = [], url: String? = nil) {
            self.server = server
            self.mode = mode
            self.required = required
            self.url = url
        }

        /// session-tree.ts `elicitationField`: nil unless well formed.
        public static func parse(_ raw: Any?) -> Sidecar? {
            guard let object = raw as? [String: Any],
                  let server = object["server"] as? String,
                  let mode = (object["mode"] as? String).flatMap(Mode.init(rawValue:))
            else { return nil }
            return Sidecar(
                server: server,
                mode: mode,
                required: (object["required"] as? [Any])?.compactMap { $0 as? String } ?? [],
                url: object["url"] as? String
            )
        }
    }

    public enum Action: String, Equatable, Sendable {
        case accept, decline, cancel
    }

    /// A value counts as given: a non-blank string, a finite number, any
    /// boolean, a non-empty list.
    private static func given(_ value: CardJSON?) -> Bool {
        switch value {
        case let .string(text): !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        case let .number(number): number.isFinite
        case .bool: true
        case let .array(items): !items.isEmpty
        default: false
        }
    }

    /// The required fields still empty (Submit is disabled while any is,
    /// `copy.missing` under the buttons).
    public static func missing(required: [String], surfaceData: CardJSON?) -> [String] {
        let form = surfaceData?.objectValue?[formKey]?.objectValue ?? [:]
        return required.filter { !given(form[$0]) }
    }

    /// The action an answered (or dismissed) card carries back: Submit /
    /// I'm done = accept, Decline = decline, anything else = cancel. Never
    /// an accept by default.
    public static func action(answers: [String]?, answer: String?, dismissed: Bool) -> Action {
        if dismissed { return .cancel }
        let pick = (answers?.first ?? answer ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        if pick == Copy.submit || pick == Copy.done { return .accept }
        if pick == Copy.decline { return .decline }
        return .cancel
    }
}
