import Foundation

/// #EXP-1611: CI on a pull request, the UI ×4 (packages/domain-contract
/// fixtures/pr-checks.json MERGE PILL / CHECKS LIST is the spec; TS reference
/// `src/pr-checks.ts`, desktop `domain::pr_checks`, Android `PrChecks`). Pure:
/// reads the synced `pull_request_checks` rows + `pull_requests.checks_state`,
/// replayed by `PrChecksTests`.
public enum PrChecks {
    // MARK: - Copy (fixture `copy` / `testIds`)

    public enum Copy {
        public static let checks = "Checks"
        public static let mergeFailing = "Checks failing"
        public static let summaryFailing = "{count} failing"
        public static let summaryRunning = "{count} running"
        public static let summaryPassed = "{count} passed"
    }

    public enum TestIds {
        public static let checksMenu = "pr-checks-menu"
        public static let checkRow = "pr-check-row"
    }

    // MARK: - Rows

    /// The fields the UI reads off one check row.
    public struct Check: Equatable, Sendable {
        public let name: String
        public let status: String
        public let conclusion: String?
        public let startedAt: String?
        public let completedAt: String?
        public let detailsUrl: String?

        public init(
            name: String,
            status: String,
            conclusion: String? = nil,
            startedAt: String? = nil,
            completedAt: String? = nil,
            detailsUrl: String? = nil
        ) {
            self.name = name
            self.status = status
            self.conclusion = conclusion
            self.startedAt = startedAt
            self.completedAt = completedAt
            self.detailsUrl = detailsUrl
        }

        public init(_ entity: PullRequestCheckEntity) {
            self.init(
                name: entity.name, status: entity.status, conclusion: entity.conclusion,
                startedAt: entity.startedAt, completedAt: entity.completedAt, detailsUrl: entity.detailsUrl
            )
        }
    }

    public enum Glyph: String, Sendable, Equatable {
        case queued = "ci-queued"
        case running = "ci-running"
        case passed = "ci-passed"
        case failed = "ci-failed"
        case skipped = "ci-skipped"

        /// GLYPH ORDER: failed, running, queued, passed, skipped.
        var rank: Int {
            switch self {
            case .failed: return 0
            case .running: return 1
            case .queued: return 2
            case .passed: return 3
            case .skipped: return 4
            }
        }
    }

    public enum Tone: String, Sendable, Equatable {
        case muted, green, red
    }

    public struct ListRow: Equatable, Sendable {
        public let name: String
        public let glyph: Glyph
        public let tone: Tone
        /// nil = no value.
        public let duration: String?
        /// The logs (details_url); nil = the row is disabled.
        public let url: String?
    }

    public struct Summary: Equatable, Sendable {
        public let glyph: Glyph
        public let tone: Tone
        public let value: String
    }

    private static let failing = Set(DomainContract.prChecksFailingConclusions)

    public static func isFailing(status: String, conclusion: String?) -> Bool {
        guard status == DomainContract.prCheckStatusCompleted, let conclusion else { return false }
        return failing.contains(conclusion)
    }

    public static func isFailing(_ check: Check) -> Bool {
        isFailing(status: check.status, conclusion: check.conclusion)
    }

    /// none (no rows) · failure (wins over pending) · pending · success.
    public static func aggregate(_ checks: [Check]) -> String {
        if checks.isEmpty { return DomainContract.prChecksStateNone }
        if checks.contains(where: isFailing) { return DomainContract.prChecksStateFailure }
        if checks.contains(where: { $0.status != DomainContract.prCheckStatusCompleted }) {
            return DomainContract.prChecksStatePending
        }
        return DomainContract.prChecksStateSuccess
    }

    // MARK: - Duration

    /// `42s` · `3m 5s` · `1h 2m` (floored; negative = 0s).
    public static func formatDuration(ms: Double) -> String {
        let seconds = max(0, Int((ms / 1000).rounded(.down)))
        if seconds < 60 { return "\(seconds)s" }
        let minutes = seconds / 60
        if minutes < 60 { return "\(minutes)m \(seconds % 60)s" }
        return "\(minutes / 60)h \(minutes % 60)m"
    }

    private static func ms(_ value: String?) -> Double? {
        guard let value, !value.isEmpty, let date = WireTimestamps.parse(value) else { return nil }
        return date.timeIntervalSince1970 * 1000
    }

    /// completed = completedAt - startedAt; in_progress = now - startedAt;
    /// else nil (queued, or no start).
    public static func checkDuration(_ check: Check, now: Date) -> String? {
        guard let started = ms(check.startedAt) else { return nil }
        if check.status == DomainContract.prCheckStatusCompleted {
            guard let completed = ms(check.completedAt) else { return nil }
            return formatDuration(ms: completed - started)
        }
        if check.status == DomainContract.prCheckStatusInProgress {
            return formatDuration(ms: now.timeIntervalSince1970 * 1000 - started)
        }
        return nil
    }

    // MARK: - Glyphs, list, summary

    public static func checkGlyph(status: String, conclusion: String?) -> (glyph: Glyph, tone: Tone) {
        if status == DomainContract.prCheckStatusQueued { return (.queued, .muted) }
        if status != DomainContract.prCheckStatusCompleted { return (.running, .muted) }
        if isFailing(status: status, conclusion: conclusion) { return (.failed, .red) }
        if conclusion == DomainContract.prCheckConclusionSuccess { return (.passed, .green) }
        return (.skipped, .muted)
    }

    /// The `…` menu's Checks rows: failed, running, queued, passed, skipped;
    /// ties by name (code-point order).
    public static func checksList(_ checks: [Check], now: Date) -> [ListRow] {
        checks
            .map { check -> ListRow in
                let (glyph, tone) = checkGlyph(status: check.status, conclusion: check.conclusion)
                let url = check.detailsUrl.flatMap { $0.isEmpty ? nil : $0 }
                return ListRow(
                    name: check.name, glyph: glyph, tone: tone,
                    duration: checkDuration(check, now: now), url: url
                )
            }
            .sorted { a, b in
                if a.glyph.rank != b.glyph.rank { return a.glyph.rank < b.glyph.rank }
                return Array(a.name.unicodeScalars.map(\.value))
                    .lexicographicallyPrecedes(b.name.unicodeScalars.map(\.value))
            }
    }

    /// The Checks entry's glyph + value; nil = no rows (no entry).
    public static func checksSummary(_ checks: [Check]) -> Summary? {
        func fill(_ template: String, _ count: Int) -> String {
            template.replacingOccurrences(of: "{count}", with: "\(count)")
        }
        switch aggregate(checks) {
        case DomainContract.prChecksStateFailure:
            return Summary(glyph: .failed, tone: .red, value: fill(Copy.summaryFailing, checks.filter(isFailing).count))
        case DomainContract.prChecksStatePending:
            let running = checks.filter { $0.status != DomainContract.prCheckStatusCompleted }.count
            return Summary(glyph: .running, tone: .muted, value: fill(Copy.summaryRunning, running))
        case DomainContract.prChecksStateSuccess:
            return Summary(glyph: .passed, tone: .green, value: fill(Copy.summaryPassed, checks.count))
        default:
            return nil
        }
    }

    // MARK: - Merge pill

    public struct MergePill: Equatable, Sendable {
        public enum Glyph: String, Sendable { case prMerged = "pr-merged", spinner, ciFailed = "ci-failed" }
        public enum Confirm: String, Sendable { case merge, stack, failing }

        public let label: String
        /// `.spinner` = THE spinner in place of a glyph.
        public let glyph: Glyph
        public let primary: Bool
        /// Which card a tap opens: the plain merge confirm, the stack
        /// confirm, or prompts.json `merge-failing-pr`.
        public let confirm: Confirm
    }

    /// THE header Merge pill over the PR's `checks_state`.
    public static func mergePill(state: String?, stack: Bool) -> MergePill {
        if state == DomainContract.prChecksStateFailure {
            return MergePill(label: Copy.mergeFailing, glyph: .ciFailed, primary: false, confirm: .failing)
        }
        return MergePill(
            label: stack ? DomainContract.diffUiMergeStack : DomainContract.diffUiMergePr,
            glyph: state == DomainContract.prChecksStatePending ? .spinner : .prMerged,
            primary: true,
            confirm: stack ? .stack : .merge
        )
    }

    /// prompts.json `merge-failing-pr` body: the failing rows' names in list
    /// order (five, then +N).
    public static func failingBody(_ checks: [Check]) -> String {
        let names = checksList(checks, now: Date(timeIntervalSince1970: 0))
            .filter { $0.glyph == .failed }
            .map(\.name)
        let shown = names.prefix(5).joined(separator: ", ")
        let list = names.count > 5 ? "\(shown) +\(names.count - 5)" : shown
        return Prompts.fill(Prompts.MergeFailingPr.entry.texts["body"] ?? "", ["names": list])
    }
}
