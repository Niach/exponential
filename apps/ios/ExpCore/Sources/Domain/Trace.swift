import Foundation

// EXP-1324: THE Trace patterns, ONE model ×4. The transcript's tool rows and
// groups, the reasoning line, the subagent line and the failed-tests line are
// all Trace lines. Hand-mirrored from the TS reference
// `packages/domain-contract/src/trace.ts` (desktop `steer::trace`, Android
// `domain/Trace.kt`) and byte-locked by
// `packages/domain-contract/fixtures/trace.json`, which `TraceTests` replays
// (whole objects). ExpUI `TraceLineView` paints a line off `Trace.Geometry`.

/// A Trace line's status: `running` shimmers the title, `error` tints the
/// line destructive, `pending` fades it to `pendingOpacity`.
public enum TraceStatus: String, Equatable, Sendable {
    case running
    case error
    case pending
}

/// One painted Trace line.
public struct TraceLineModel: Equatable, Sendable {
    /// A concept icon (packages/icons/icons.json).
    public let icon: String
    public let title: String
    public let description: String?
    public let status: TraceStatus?

    public init(icon: String, title: String, description: String?, status: TraceStatus?) {
        self.icon = icon
        self.title = title
        self.description = description
        self.status = status
    }
}

/// A command's output read as a test run's summary.
public struct TestResults: Equatable, Sendable {
    public let passed: Int
    public let failed: Int
    public let total: Int
    /// The ONE suite every failure names; nil when none or several.
    public let suite: String?

    public init(passed: Int, failed: Int, total: Int, suite: String?) {
        self.passed = passed
        self.failed = failed
        self.total = total
        self.suite = suite
    }
}

public enum Trace {
    /// Fixture `geometry` (pt = px).
    public struct LineGeometry: Equatable, Sendable {
        public let fontSize: Double
        public let lineHeight: Double
        public let icon: Double
        public let gap: Double
        public let lineGap: Double
        public let chevronGap: Double
        public let contentTop: Double
        public let contentBottom: Double
        public let railInset: Double
        public let railWidth: Double
        public let contentInset: Double
        public let contentTrailing: Double
    }

    public enum Geometry {
        public static let `default` = LineGeometry(
            fontSize: 14, lineHeight: 20, icon: 16, gap: 8, lineGap: 8, chevronGap: 4,
            contentTop: 8, contentBottom: 4, railInset: 7, railWidth: 1, contentInset: 24,
            contentTrailing: 4
        )
        public static let sm = LineGeometry(
            fontSize: 12, lineHeight: 18, icon: 14, gap: 6, lineGap: 6, chevronGap: 4,
            contentTop: 6, contentBottom: 4, railInset: 6, railWidth: 1, contentInset: 20,
            contentTrailing: 4
        )
        public static let descriptionAlpha: Double = 0.7
        public static let pendingOpacity: Double = 0.6

        /// Fixture `geometry.codeBlock` (radius token `lg`).
        public enum CodeBlock {
            public static let border: Double = 1
            public static let fillAlpha: Double = 0.3
            public static let padding: Double = 10
            public static let contentTopWithHeader: Double = 6
            public static let titleSize: Double = 12
            public static let fontSize: Double = 12
            public static let lineHeight: Double = 19.5
            public static let copyButton: Double = 24
            public static let copyIcon: Double = 12
            public static let headerActionsOutset: Double = 4
            public static let maxLines: Int = 16
            public static let destructiveBorderAlpha: Double = 0.4
        }
    }

    /// Fixture `icons`.
    public enum Icons {
        public static let tool = "trace-tool"
        public static let reasoning = "trace-reasoning"
        public static let subagent = "trace-subagent"
        public static let failed = "trace-failed"
        public static let chevron = "trace-chevron"
    }

    /// Fixture `copy`.
    public enum Copy {
        public static let output = "Output"
        public static let copy = "Copy"
        public static let copied = "Copied"
    }

    private struct Verbs {
        let running: String
        let settled: String
        let blank: String
        let icon: String
    }

    /// Fixture `tool.verbs`.
    private static let verbs: [String: Verbs] = [
        "execute": Verbs(running: "Running", settled: "Ran", blank: "a command", icon: "trace-terminal"),
        "read": Verbs(running: "Reading", settled: "Read", blank: "a file", icon: "trace-read"),
        "edit": Verbs(running: "Editing", settled: "Edited", blank: "a file", icon: "trace-edit"),
        "delete": Verbs(running: "Deleting", settled: "Deleted", blank: "a file", icon: "trace-edit"),
        "move": Verbs(running: "Moving", settled: "Moved", blank: "a file", icon: "trace-edit"),
        "search": Verbs(running: "Searching", settled: "Searched", blank: "the code", icon: "trace-search"),
        "fetch": Verbs(running: "Fetching", settled: "Fetched", blank: "a page", icon: "trace-fetch"),
        "think": Verbs(running: "Thinking", settled: "Thought", blank: "it through", icon: "trace-reasoning"),
        "wait": Verbs(running: "Waiting on", settled: "Waited on", blank: "a task", icon: "trace-background"),
    ]

    static func clean(_ value: String?) -> String {
        (value ?? "")
            .split(whereSeparator: { $0.isWhitespace })
            .joined(separator: " ")
    }

    // MARK: Tool line

    /// A tool call as its Trace line. Verb kinds read `{verb} {detail}`; a
    /// blank detail takes the kind's `blank` object. `other`/unknown kinds
    /// keep the tool's own name as the title, the detail as the description.
    public static func tool(
        toolKind: String, name: String, detail: String?, settled: Bool, failed: Bool
    ) -> TraceLineModel {
        let status: TraceStatus? = failed ? .error : (settled ? nil : .running)
        let cleanDetail = clean(detail)
        guard let verb = verbs[toolKind] else {
            let title = clean(name)
            return TraceLineModel(
                icon: Icons.tool,
                title: title.isEmpty ? "Tool" : title,
                description: cleanDetail.isEmpty ? nil : cleanDetail,
                status: status
            )
        }
        let word = settled || failed ? verb.settled : verb.running
        return TraceLineModel(
            icon: verb.icon,
            title: "\(word) \(cleanDetail.isEmpty ? verb.blank : cleanDetail)",
            description: nil,
            status: status
        )
    }

    // MARK: Reasoning line

    /// "Thinking" (shimmering) while the thought streams; "Thought for 5s"
    /// once it closed; "Thought for a moment" under one second or without a
    /// duration.
    public static func reasoning(running: Bool, durationMs: Int?) -> TraceLineModel {
        if running {
            return TraceLineModel(icon: Icons.reasoning, title: "Thinking", description: nil, status: .running)
        }
        let title: String
        if let ms = durationMs, ms >= 1000 {
            title = "Thought for \(WorkCards.durationLong(ms: ms))"
        } else {
            title = "Thought for a moment"
        }
        return TraceLineModel(icon: Icons.reasoning, title: title, description: nil, status: nil)
    }

    // MARK: Subagent line

    /// "Running the Explore agent" → "Ran the Explore agent" + a duration
    /// badge (`description`; clients paint the outline Badge) → "The Explore
    /// agent failed".
    public static func subagent(
        label: String?, running: Bool, failed: Bool, durationMs: Int?
    ) -> TraceLineModel {
        let name = clean(label)
        let title: String
        if name.isEmpty {
            title = failed ? "A subagent failed" : (running ? "Running a subagent" : "Ran a subagent")
        } else {
            title = failed
                ? "The \(name) agent failed"
                : (running ? "Running the \(name) agent" : "Ran the \(name) agent")
        }
        let description: String?
        if let ms = durationMs, ms >= 0 {
            description = WorkCards.durationLong(ms: ms)
        } else {
            description = nil
        }
        return TraceLineModel(
            icon: Icons.subagent,
            title: title,
            description: description,
            status: failed ? .error : (running ? .running : nil)
        )
    }

    // MARK: Failed tests line

    private static func regex(_ pattern: String) -> NSRegularExpression {
        // swiftlint:disable:next force_try
        try! NSRegularExpression(pattern: pattern, options: [.anchorsMatchLines])
    }

    private static let ansiCsi = regex("\u{1B}\\[[0-?]*[ -/]*[@-~]")
    private static let jestRe = regex(
        "^Tests:\\s+(?:(\\d+) failed, )?(?:\\d+ skipped, )?(?:\\d+ todo, )?(?:(\\d+) passed, )?(\\d+) total"
    )
    private static let vitestRe = regex("^\\s*Tests\\s+(?:(\\d+) failed)?(?:\\s*\\|\\s*)?(?:(\\d+) passed)?[^(\\n]*\\((\\d+)\\)")
    private static let cargoRe = regex("^test result: \\w+\\. (\\d+) passed; (\\d+) failed;")
    private static let xcodeRe = regex("^\\s*Executed (\\d+) tests?, with (\\d+) failures?")
    private static let gradleRe = regex("^(\\d+) tests? completed, (\\d+) failed")
    private static let pytestRe = regex(
        "^=+ (?:(\\d+) failed)?(?:, )?(?:(\\d+) passed)?[^=\\n]* in [\\d.]+s(?: \\([^)]*\\))? =+$"
    )
    private static let bunPassRe = regex("^\\s*(\\d+) pass$")
    private static let bunFailRe = regex("^\\s*(\\d+) fail$")

    private static let bunSuiteRe = regex("^\\(fail\\) (.+?) > ")
    private static let jestSuiteRe = regex("^\\s*FAIL\\s+(\\S+?)(?:\\s+>\\s+(.+?)\\s+>)?(?:\\s|$)")
    private static let gradleSuiteRe = regex("^(\\S+) > .* FAILED$")
    private static let pytestSuiteRe = regex("^FAILED (\\S+?)::")
    private static let xcodeSuiteRe = regex("^Test Case '-\\[(?:\\w+\\.)?(\\w+) \\w+\\]' failed")
    private static let cargoSuiteRe = regex("^---- (?:(\\S+)::)?\\S+ stdout ----$")

    /// Capture groups of a match (index 0 = the whole match); nil = absent.
    private static func groups(_ match: NSTextCheckingResult, in text: String) -> [String?] {
        (0..<match.numberOfRanges).map { index in
            let range = match.range(at: index)
            guard range.location != NSNotFound, let swiftRange = Range(range, in: text) else { return nil }
            return String(text[swiftRange])
        }
    }

    private static func first(_ re: NSRegularExpression, _ text: String) -> [String?]? {
        let range = NSRange(text.startIndex..., in: text)
        return re.firstMatch(in: text, range: range).map { groups($0, in: text) }
    }

    private static func all(_ re: NSRegularExpression, _ text: String) -> [[String?]] {
        let range = NSRange(text.startIndex..., in: text)
        return re.matches(in: text, range: range).map { groups($0, in: text) }
    }

    private static func int(_ value: String?) -> Int { value.flatMap { Int($0) } ?? 0 }

    /// Parses a command's output as a test run's summary (fixture
    /// `tests.$comment`): jest, vitest, cargo, xcodebuild, gradle, pytest, bun
    /// — the FIRST runner whose summary matches decides. Nil = not test output.
    public static func parseTestResults(_ output: String?) -> TestResults? {
        guard let output, !output.isEmpty else { return nil }
        let stripped = ansiCsi.stringByReplacingMatches(
            in: output, range: NSRange(output.startIndex..., in: output), withTemplate: ""
        )
        let text = stripped
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
        var counts: (passed: Int, failed: Int, total: Int)?

        if let m = first(jestRe, text) {
            counts = (int(m[2]), int(m[1]), int(m[3]))
        }
        if counts == nil, let m = first(vitestRe, text), m[1] != nil || m[2] != nil {
            counts = (int(m[2]), int(m[1]), int(m[3]))
        }
        if counts == nil {
            let hits = all(cargoRe, text)
            if !hits.isEmpty {
                let passed = hits.reduce(0) { $0 + int($1[1]) }
                let failed = hits.reduce(0) { $0 + int($1[2]) }
                counts = (passed, failed, passed + failed)
            }
        }
        if counts == nil, let last = all(xcodeRe, text).last {
            let total = int(last[1])
            let failed = int(last[2])
            counts = (total - failed, failed, total)
        }
        if counts == nil, let m = first(gradleRe, text) {
            counts = (int(m[1]) - int(m[2]), int(m[2]), int(m[1]))
        }
        if counts == nil, let m = first(pytestRe, text), m[1] != nil || m[2] != nil {
            counts = (int(m[2]), int(m[1]), int(m[1]) + int(m[2]))
        }
        if counts == nil {
            let pass = first(bunPassRe, text)
            let fail = first(bunFailRe, text)
            if pass != nil || fail != nil {
                let passed = int(pass?[1] ?? nil)
                let failed = int(fail?[1] ?? nil)
                counts = (passed, failed, passed + failed)
            }
        }
        guard let counts else { return nil }
        return TestResults(
            passed: counts.passed, failed: counts.failed, total: counts.total,
            suite: failedSuite(text)
        )
    }

    /// `src/bench/StartupBenchmark.test.ts` → `StartupBenchmark`.
    private static func fileStem(_ path: String) -> String {
        let last = path.split(separator: "/", omittingEmptySubsequences: false).last.map(String.init) ?? ""
        return last.split(separator: ".", omittingEmptySubsequences: false).first.map(String.init) ?? ""
    }

    private static func failedSuite(_ text: String) -> String? {
        var suites: [String] = []
        func add(_ value: String?) {
            let name = clean(value)
            if !name.isEmpty, !suites.contains(name) { suites.append(name) }
        }
        for line in text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init) {
            if let m = first(bunSuiteRe, line) {
                add(m[1])
            } else if let m = first(jestSuiteRe, line) {
                add(m[2] ?? fileStem(m[1] ?? ""))
            } else if let m = first(gradleSuiteRe, line) {
                add((m[1] ?? "").split(separator: ".", omittingEmptySubsequences: false).last.map(String.init))
            } else if let m = first(pytestSuiteRe, line) {
                add(fileStem(m[1] ?? ""))
            } else if let m = first(xcodeSuiteRe, line) {
                add(m[1])
            } else if let m = first(cargoSuiteRe, line) {
                add(m[1])
            }
        }
        return suites.count == 1 ? suites[0] : nil
    }

    /// The failed-tests line for a SETTLED command whose output parsed with
    /// at least one failure; nil otherwise.
    public static func failedTests(_ results: TestResults?) -> TraceLineModel? {
        guard let results, results.failed > 0 else { return nil }
        let tests = results.total == 1 ? "test" : "tests"
        var title = "Failed: \(results.failed) of \(results.total) \(tests)"
        if let suite = results.suite { title += " in \(suite)" }
        return TraceLineModel(icon: Icons.failed, title: title, description: nil, status: .error)
    }
}
