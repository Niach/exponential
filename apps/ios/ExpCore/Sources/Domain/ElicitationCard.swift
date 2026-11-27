import Foundation

/// #NEW-146: THE elicitation card ×4 (`packages/domain-contract/fixtures/
/// elicitation-card.json`, TS reference `src/elicitation-card.ts`; desktop
/// `domain::elicitation_card`, Android `domain/ElicitationCard.kt`): a
/// THIRD-PARTY MCP server's elicitation as the question card. The device
/// builds the card (form surface or URL review); the renderers replay
/// `missingCases` (Submit waits for the required fields), `actionCases`
/// (what an answer carries back) and `feedCardCases` (a live frame draws
/// the posted card). `ElicitationCardTests` pins the constants.
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

    /// The specimen's device-built cards (`cardCases`' expected values,
    /// whole: `ElicitationCardTests` locks them to the fixture): the waiting
    /// form (every field kind, defaults prefilled), the clean url review and
    /// the warned one. The answered case = the form, submitted.
    public enum Specimen {
        public static let form = #"""
        {"kind":"question","question":"Where should the issue go?","options":["Submit","Decline","Cancel"],"custom":false,"multi":false,"surface":{"version":"1.0","root":{"id":"root","component":"Stack","props":{"direction":"vertical","gap":"md"},"children":[{"id":"f_title","component":"Input","props":{"label":"Title","name":"title","type":"text","value":{"@path":"/form/title"}}},{"id":"f_team","component":"Select","props":{"label":"Team","name":"team","options":[{"label":"Web","value":"web"},{"label":"iOS","value":"ios"}],"value":{"@path":"/form/team"}}},{"id":"f_labels","component":"Select","props":{"label":"Labels","name":"labels","options":[{"label":"bug","value":"bug"},{"label":"crash","value":"crash"},{"label":"p1","value":"p1"}],"value":{"@path":"/form/labels"},"multiple":true}},{"id":"f_estimate","component":"NumberField","props":{"label":"Estimate","name":"estimate","value":{"@path":"/form/estimate"},"min":1,"max":8,"step":1,"precision":0}},{"id":"f_points","component":"NumberField","props":{"label":"points","name":"points","value":{"@path":"/form/points"},"placeholder":"Confidence"}},{"id":"f_due","component":"DatePicker","props":{"label":"Due","name":"due","value":{"@path":"/form/due"}}},{"id":"f_email","component":"Input","props":{"label":"Reporter","name":"email","type":"email","value":{"@path":"/form/email"},"placeholder":"Who reported it"}},{"id":"f_link","component":"Input","props":{"label":"Link","name":"link","type":"url","value":{"@path":"/form/link"}}},{"id":"f_notify","component":"Switch","props":{"label":"Notify the team","name":"notify","checked":{"@path":"/form/notify"},"description":"Posts in the team channel."}}]},"data":{"form":{"title":"Crash on launch","team":"ios","labels":["bug"],"estimate":null,"points":null,"due":"","email":"","link":"","notify":true}}},"elicitation":{"server":"linear","mode":"form","required":["title","team"],"url":null}}
        """#
        public static let url = #"""
        {"kind":"question","question":"Sign in to GitHub","options":["I'm done","Decline"],"custom":false,"multi":false,"surface":{"version":"1.0","root":{"id":"root","component":"Stack","props":{"direction":"vertical","gap":"sm"},"children":[{"id":"domain","component":"Heading","props":{"text":"github.com","level":"h4"}},{"id":"url","component":"Text","props":{"text":"https://github.com/login/oauth/authorize?client_id=abc&state=xyz","variant":"code"}}]},"data":{}},"elicitation":{"server":"github","mode":"url","required":[],"url":"https://github.com/login/oauth/authorize?client_id=abc&state=xyz"}}
        """#
        public static let warned = #"""
        {"kind":"question","question":"Pay the invoice","options":["I'm done","Decline"],"custom":false,"multi":false,"surface":{"version":"1.0","root":{"id":"root","component":"Stack","props":{"direction":"vertical","gap":"sm"},"children":[{"id":"domain","component":"Heading","props":{"text":"xn--pple-43d.co.uk","level":"h4"}},{"id":"url","component":"Text","props":{"text":"http://pay.xn--pple-43d.co.uk/invoice/42","variant":"code"}},{"id":"warn_punycode","component":"Alert","props":{"title":"This address uses look-alike characters.","type":"warning"}},{"id":"warn_insecure","component":"Alert","props":{"title":"This link is not encrypted (no https).","type":"warning"}}]},"data":{}},"elicitation":{"server":"billing","mode":"url","required":[],"url":"http://pay.xn--pple-43d.co.uk/invoice/42"}}
        """#
        /// The answered form's stamped data model (web specimen: estimate 3,
        /// due 2026-12-01 over the defaults).
        public static let answeredData: [String: CardJSON] = [
            "estimate": .number(3),
            "due": .string("2026-12-01"),
        ]
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

    /// How Open link opens `elicitation.url` on iOS (the fixture's OPEN
    /// LINK): an OAuth authorize URL (a path segment naming `oauth`, or an
    /// OAuth query: `response_type` + `client_id`) in
    /// ASWebAuthenticationSession, any other http(s) URL in
    /// SFSafariViewController; nil = nothing opens (not an absolute http(s)
    /// URL). Never an in-app WebView, never before the press.
    public enum OpenTarget: Equatable, Sendable {
        case safari(URL)
        case oauth(URL)
    }

    public static func openTarget(_ raw: String?) -> OpenTarget? {
        guard let raw, let components = URLComponents(string: raw),
              let scheme = components.scheme?.lowercased(), scheme == "https" || scheme == "http",
              let host = components.host, !host.isEmpty,
              let url = components.url
        else { return nil }
        let segments = components.path.lowercased().split(separator: "/")
        let query = Set((components.queryItems ?? []).map { $0.name.lowercased() })
        let oauth = segments.contains { $0.hasPrefix("oauth") }
            || (query.contains("response_type") && query.contains("client_id"))
        return oauth ? .oauth(url) : .safari(url)
    }

    /// The live feed's view of THE card (`feedCardCases`, TS
    /// `elicitationFeedCard`): an engine ask reaches a live session view
    /// twice, as the relay's question frame (question, options, header: no
    /// surface, no sidecar) and as the posted `messages.card`. A live view
    /// draws the POSTED card (its form or URL review, answered via
    /// `sessions.answer`) for a frame of the same `questionId`: the id of the
    /// first message whose card is a `question` with that `questionId` and an
    /// `elicitation` sidecar. nil = the frame stays: no such card synced yet,
    /// or the frame already resolved while the card never got an answer
    /// (withdrawn: the frame names it Dismissed).
    public static func feedCard(_ messages: [TreeMessage], questionId: String, resolved: Bool) -> String? {
        for message in messages {
            guard let card = message.card, card.kind == "question",
                  card.questionId == questionId, card.elicitation != nil
            else { continue }
            if resolved && message.answeredAt == nil { return nil }
            return message.id
        }
        return nil
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
