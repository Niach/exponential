import Foundation

/// EXP-1280 custom plan mode: THE plan / question card's behaviour ×4
/// (`packages/domain-contract/fixtures/plan-card.json`, TS reference
/// `src/plan-card.ts`; desktop `domain::plan_card`, Android
/// `domain/PlanCard.kt`). The render rules live in the fixture's `$comment`;
/// `PlanCardTests` replays `answerIssueCases`, `pictureForCases` and
/// `optionRowsCases` and pins the constants below to the fixture.
/// (`answerText` / `resolveUpload` are the server's and the device's.)
public enum PlanCard {
    // MARK: - Fixture constants

    public enum Copy {
        public static let answers = "Answers"
        public static let feedback = "Feedback"
        public static let chooseOne = "Choose one"
        public static let chooseAny = "Choose any"
        public static let custom = "Other…"
        public static let surface = "Preview"
        public static let answer = "Answer"
        /// A closed card's caption / an in-flight answer's (ONE string ×4).
        public static let answered = "Answered"
        public static let answering = "Answering…"
        /// `copy.feedbackLine`: an answered plan's feedback on ONE line.
        public static let feedbackLine = "Feedback: {feedback}"

        public static func feedbackLine(_ text: String) -> String {
            feedbackLine.replacingOccurrences(of: "{feedback}", with: text)
        }

        /// `copy.specimen`: the styleguide entry's three captions ×4.
        public enum Specimen {
            public static let pending = "A plan waiting on you: pictures, a surface, two questions"
            public static let answered = "A question answered with several picks"
            public static let approved = "An approved plan"
        }

        /// `copy.answerIssues`, keyed by `AnswerIssue`.
        public static func issue(_ issue: AnswerIssue) -> String {
            switch issue {
            case .decision: "Pick a decision"
            case .missing: "Answer every question to approve"
            case .multi: "This question takes one answer"
            case .unknown: "Pick one of the options"
            }
        }
    }

    public enum TestIds {
        public static let card = "plan-card"
        public static let question = "plan-card-question"
        public static let option = "plan-card-option"
        public static let tile = "plan-card-tile"
        public static let custom = "plan-card-custom"
        public static let surface = "plan-card-surface"
        public static let feedback = "plan-card-feedback"
        public static let issue = "plan-card-issue"
    }

    /// The picture tiles' geometry (pt).
    public enum Tiles {
        public static let columns = 2
        public static let phoneColumns = 2
        public static let gap: Double = 8
        public static let pictureAspect: Double = 1.6
        public static let radius: Double = 10
        public static let selectedRing: Double = 2
        public static let padding: Double = 8
        public static let pictureRadius: Double = 6
        /// The tile's border width.
        public static let stroke: Double = 1
        /// The `editor-image` placeholder glyph (at 0.6).
        public static let placeholderGlyph: Double = 20
        /// Picture → label and label → description.
        public static let labelGap: Double = 4
        /// A closed card's unpicked tiles.
        public static let inertOpacity: Double = 0.5
    }

    /// The rest of the card's geometry (pt; plan-card.json `card`).
    public enum Card {
        /// The body's inline picture: radius + stroke, full content width at
        /// the probed aspect, no max height.
        public static let pictureRadius: Double = 12
        public static let pictureStroke: Double = 1
        /// The inline surface's frame under its `copy.surface` label.
        public static let surfaceRadius: Double = 12
        public static let surfacePadding: Double = 12
        public static let surfaceLabelGap: Double = 6
        /// Question blocks apart; prompt → caption; caption → options.
        public static let blockGap: Double = 12
        public static let blockInnerGap: Double = 2
        public static let optionsGap: Double = 6
        /// THE option row (and the `custom` field's height).
        public static let rowMinHeight: Double = 32
        public static let rowRadius: Double = 10
        /// The plan's feedback field.
        public static let fieldMinHeight: Double = 36
        /// A closed card's unpicked rows.
        public static let inertOpacity: Double = 0.5
        /// The chat markdown: list text indent, h1 = `headingScale` × body.
        public static let listIndent: Double = 24
        public static let headingScale: Double = 1.2
    }

    /// An embed whose upload has not landed yet (`![alt](upload:<label>)`)
    /// renders nothing.
    public static let uploadScheme = "upload:"

    /// The longest label an answer may carry (the server's
    /// `PLAN_CARD_LABEL_MAX`, `packages/db-schema/src/domain.ts`): a typed
    /// `custom` answer is capped here so it never fails the wire schema.
    public static let labelMax = 500

    /// A press on `label` (web `togglePick`): a single choice picks it (again
    /// = unpicks), a multiple one toggles it, the picks kept in OPTION order
    /// (`order` = the labels as the card lists them; an unknown pick sorts
    /// last, in pick order).
    public static func togglePick(_ picked: [String], label: String, multi: Bool, order: [String]) -> [String] {
        if !multi { return picked.contains(label) ? [] : [label] }
        let next = picked.contains(label) ? picked.filter { $0 != label } : picked + [label]
        let rank = { (pick: String) -> Int in order.firstIndex(of: pick) ?? Int.max }
        return next.enumerated()
            .sorted { a, b in
                let ra = rank(a.element), rb = rank(b.element)
                return ra != rb ? ra < rb : a.offset < b.offset
            }
            .map(\.element)
    }

    /// A closed question card's picks (web `closedPicks`): `answers`, else a
    /// legacy bare `answer` that names an option (a free answer, or an old
    /// joined multi answer "A, B", is no pick — it reads as the answer line).
    public static func closedPicks(options: [TreeQuestionOption], answer: String?, answers: [String]?) -> [String] {
        if let answers { return answers }
        if let answer, options.contains(where: { $0.label == answer }) { return [answer] }
        return []
    }

    /// Why an answer cannot be sent (nil = fine).
    public enum AnswerIssue: String, Equatable, Sendable, CaseIterable {
        case decision, missing, multi, unknown
    }

    public enum OptionRows: String, Equatable, Sendable {
        case list, tiles
    }

    /// The person's answer as it would be sent.
    public struct AnswerInput: Equatable, Sendable {
        public var answer: String?
        /// A plan: question id → labels.
        public var planAnswers: [String: [String]]?
        /// A question card: the picked labels.
        public var labels: [String]?
        public var decision: String?

        public init(answer: String? = nil, planAnswers: [String: [String]]? = nil, labels: [String]? = nil, decision: String? = nil) {
            self.answer = answer
            self.planAnswers = planAnswers
            self.labels = labels
            self.decision = decision
        }
    }

    // MARK: - Model

    /// A plan needs a decision; approving needs every question answered; a
    /// single-choice question takes one answer; without `custom` every
    /// answer must be an option's label.
    public static func answerIssue(questions: [ThreadPlanQuestion], input: AnswerInput) -> AnswerIssue? {
        guard input.decision == "approved" || input.decision == "rejected" else { return .decision }
        let answers = input.planAnswers ?? [:]
        for id in answers.keys.sorted() where !questions.contains(where: { $0.id == id }) {
            return .unknown
        }
        for question in questions {
            let picked = trimmed(answers[question.id] ?? [])
            if picked.isEmpty {
                if input.decision == "approved" { return .missing }
                continue
            }
            if let issue = optionIssue(
                labels: question.options.map(\.label), multi: question.multi, custom: question.custom, picked: picked
            ) {
                return issue
            }
        }
        return nil
    }

    /// A question card needs one answer (`labels`, else `answer`).
    public static func answerIssue(options: [String], multi: Bool, custom: Bool, input: AnswerInput) -> AnswerIssue? {
        let picked: [String]
        if let labels = input.labels {
            picked = trimmed(labels)
        } else if let answer = input.answer?.trimmingCharacters(in: .whitespacesAndNewlines), !answer.isEmpty {
            picked = [answer]
        } else {
            picked = []
        }
        if picked.isEmpty { return .missing }
        return optionIssue(labels: options, multi: multi, custom: custom, picked: picked)
    }

    private static func trimmed(_ labels: [String]) -> [String] {
        labels.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
    }

    private static func optionIssue(labels: [String], multi: Bool, custom: Bool, picked: [String]) -> AnswerIssue? {
        if !multi && picked.count > 1 { return .multi }
        if !custom && picked.contains(where: { label in
            !labels.contains { $0.trimmingCharacters(in: .whitespacesAndNewlines) == label }
        }) {
            return .unknown
        }
        return nil
    }

    /// The attachment a picture label resolved to (nil = none / not yet).
    public static func pictureFor(_ pictures: [PlanCardPicture]?, label: String?) -> String? {
        guard let label, let pictures else { return nil }
        return pictures.first { $0.label == label }?.attachmentId
    }

    /// Text-only options = THE option list; any option with a picture = tiles.
    public static func optionRows(_ attachmentIds: [String?]) -> OptionRows {
        attachmentIds.contains { ($0 ?? "").isEmpty == false } ? .tiles : .list
    }

    public static func optionRows(_ options: [TreeQuestionOption]) -> OptionRows {
        optionRows(options.map(\.attachmentId))
    }

    /// The question card's compat `answer` string for a list of picks.
    public static func joinAnswers(_ answers: [String]) -> String {
        answers.joined(separator: ", ")
    }

    /// The markdown a client renders: an embed still pointing at
    /// `upload:<label>` renders NOTHING until its upload lands.
    public static func renderableMarkdown(_ markdown: String) -> String {
        guard markdown.contains("](\(uploadScheme)") else { return markdown }
        let pattern = #"!\[[^\]]*\]\(upload:[^)]*\)"#
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return markdown }
        return regex.stringByReplacingMatches(
            in: markdown, range: NSRange(markdown.startIndex..., in: markdown), withTemplate: ""
        ).trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// One `pictures` entry (the card's upload namespace).
public struct PlanCardPicture: Equatable, Sendable {
    public let label: String
    public let attachmentId: String?
    public let width: Double?
    public let height: Double?

    public init(label: String, attachmentId: String?, width: Double? = nil, height: Double? = nil) {
        self.label = label
        self.attachmentId = attachmentId
        self.width = width
        self.height = height
    }

    /// A card's `pictures` in a tolerant read (no string label = dropped).
    public static func list(_ raw: Any?) -> [PlanCardPicture] {
        guard let entries = raw as? [Any] else { return [] }
        return entries.compactMap { entry in
            guard let object = entry as? [String: Any], let label = object["label"] as? String else { return nil }
            return PlanCardPicture(
                label: label,
                attachmentId: object["attachmentId"] as? String,
                width: CardJSON.number(object["width"]),
                height: CardJSON.number(object["height"])
            )
        }
    }
}

/// EXP-1280: one choice question of a plan card in the row form.
public struct ThreadPlanQuestion: Equatable, Sendable, Identifiable {
    public let id: String
    public let prompt: String
    public let multi: Bool
    /// Free text allowed (default true).
    public let custom: Bool
    public let options: [TreeQuestionOption]

    public init(id: String, prompt: String, multi: Bool = false, custom: Bool = true, options: [TreeQuestionOption]) {
        self.id = id
        self.prompt = prompt
        self.multi = multi
        self.custom = custom
        self.options = options
    }

    /// A plan card's `questions`: a question without a string id/prompt, or
    /// an option without a string label, drops; an option's picture = its
    /// `picture` label resolved through `pictures`.
    public static func list(_ raw: Any?, pictures: [PlanCardPicture]) -> [ThreadPlanQuestion] {
        guard let entries = raw as? [Any] else { return [] }
        return entries.compactMap { entry in
            guard let object = entry as? [String: Any],
                  let id = object["id"] as? String,
                  let prompt = object["prompt"] as? String
            else { return nil }
            let options = ((object["options"] as? [Any]) ?? []).compactMap { raw -> TreeQuestionOption? in
                guard let option = raw as? [String: Any], let label = option["label"] as? String else { return nil }
                let description = (option["description"] as? String).flatMap { $0.isEmpty ? nil : $0 }
                return TreeQuestionOption(
                    label: label,
                    description: description,
                    attachmentId: PlanCard.pictureFor(pictures, label: option["picture"] as? String)
                )
            }
            return ThreadPlanQuestion(
                id: id,
                prompt: prompt,
                multi: CardJSON.isTrue(object["multi"]),
                custom: !CardJSON.isFalse(object["custom"]),
                options: options
            )
        }
    }
}

/// EXP-1280: the inline Exponential UI core surface an agent authored.
public struct CardSurface: Equatable, Sendable {
    /// The NestedNode root (always an object).
    public let root: CardJSON
    /// The initial data model (an object), nil = none.
    public let data: CardJSON?

    public init(root: CardJSON, data: CardJSON?) {
        self.root = root
        self.data = data
    }

    /// `{root, data}` only when `root` is an object; `data` only an object.
    public static func parse(_ raw: Any?) -> CardSurface? {
        guard let object = raw as? [String: Any], let root = object["root"] as? [String: Any] else { return nil }
        let data = (object["data"] as? [String: Any]).flatMap(CardJSON.init(any:))
        return CardSurface(root: CardJSON(any: root) ?? .object([:]), data: data)
    }

    /// The NestedNode the renderer's `setNested` takes (the data model is
    /// written beside it, key by key).
    public var rootJSON: String { root.compactString }
}

/// A JSON value carried through the Sendable card model (the surface, its
/// data model).
public indirect enum CardJSON: Equatable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([CardJSON])
    case object([String: CardJSON])

    /// From a `JSONSerialization` value.
    public init?(any value: Any?) {
        switch value {
        case nil, is NSNull: self = .null
        case let number as NSNumber:
            self = CFGetTypeID(number) == CFBooleanGetTypeID() ? .bool(number.boolValue) : .number(number.doubleValue)
        case let string as String: self = .string(string)
        case let array as [Any]: self = .array(array.map { CardJSON(any: $0) ?? .null })
        case let object as [String: Any]:
            var out: [String: CardJSON] = [:]
            for (key, inner) in object { out[key] = CardJSON(any: inner) ?? .null }
            self = .object(out)
        default: return nil
        }
    }

    /// A `JSONSerialization`-ready value.
    public var any: Any {
        switch self {
        case .null: NSNull()
        case let .bool(value): value
        case let .number(value): value
        case let .string(value): value
        case let .array(values): values.map(\.any)
        case let .object(values): values.mapValues(\.any)
        }
    }

    /// Compact JSON with keys sorted at every depth.
    public var compactString: String {
        switch self {
        case .null: return "null"
        case let .bool(value): return value ? "true" : "false"
        case let .number(value):
            if value.isFinite, value == value.rounded(), abs(value) < 1e15 { return String(Int64(value)) }
            return value.isFinite ? String(value) : "null"
        case let .string(value):
            let data = (try? JSONSerialization.data(withJSONObject: [value], options: [.fragmentsAllowed])) ?? Data()
            let wrapped = String(data: data, encoding: .utf8) ?? "[\"\"]"
            return String(wrapped.dropFirst().dropLast())
        case let .array(values):
            return "[" + values.map(\.compactString).joined(separator: ",") + "]"
        case let .object(values):
            return "{" + values.keys.sorted().map { key in
                CardJSON.string(key).compactString + ":" + (values[key] ?? .null).compactString
            }.joined(separator: ",") + "}"
        }
    }

    public var objectValue: [String: CardJSON]? {
        if case let .object(values) = self { return values }
        return nil
    }

    static func number(_ value: Any?) -> Double? {
        guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID() else { return nil }
        return number.doubleValue
    }

    static func isTrue(_ value: Any?) -> Bool {
        guard let number = value as? NSNumber, CFGetTypeID(number) == CFBooleanGetTypeID() else { return false }
        return number.boolValue
    }

    static func isFalse(_ value: Any?) -> Bool {
        guard let number = value as? NSNumber, CFGetTypeID(number) == CFBooleanGetTypeID() else { return false }
        return !number.boolValue
    }
}
