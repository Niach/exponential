import Foundation

// Options for a `pr`-typed action input (EXP-259, mobile parity EXP-270;
// EXP-1291: off the synced `pull_requests` + `pull_request_issues` rows): the
// team's OPEN pull requests, one row each, carrying the pull request's id as
// the value the `pr` input submits.

public struct StartPullRequestOption: Identifiable, Sendable, Equatable {
    /// The `pull_requests` row id — the value the `pr` input submits.
    public let id: String
    public let number: Int
    /// Every issue identifier linked to this pull request, sorted.
    public let identifiers: [String]

    public init(id: String, number: Int, identifiers: [String]) {
        self.id = id
        self.number = number
        self.identifiers = identifiers
    }

    /// `#42 · EXP-1, EXP-2` — the number, then the linked issues.
    public var label: String {
        identifiers.isEmpty ? "#\(number)" : "#\(number) · \(identifiers.joined(separator: ", "))"
    }

    /// One synced pull request as the picker reads it.
    public struct Candidate: Sendable, Equatable {
        public let id: String
        public let teamId: String
        public let number: Int
        /// Contract `prStateValues`.
        public let state: String
        /// The linked issues' identifiers (`pull_request_issues`).
        public let identifiers: [String]

        public init(id: String, teamId: String, number: Int, state: String, identifiers: [String]) {
            self.id = id
            self.teamId = teamId
            self.number = number
            self.state = state
            self.identifiers = identifiers
        }
    }

    /// The team's OPEN pull requests. The open-state guarantee lives HERE,
    /// not in the caller's SQL. Ordering is stable — by label, then id — so
    /// the list doesn't reshuffle as sync lands rows.
    public static func build(from pullRequests: [Candidate], teamId: String) -> [StartPullRequestOption] {
        pullRequests
            .filter { $0.teamId == teamId && $0.state == DomainContract.prStateOpen }
            .map {
                StartPullRequestOption(
                    id: $0.id,
                    number: $0.number,
                    identifiers: $0.identifiers.filter { !$0.isEmpty }.sorted()
                )
            }
            .sorted { ($0.label, $0.id) < ($1.label, $1.id) }
    }
}
