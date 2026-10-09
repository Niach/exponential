import Foundation

/// Polish round P14: the status groups of a list that spans TEAMS (phone My
/// issues). Each issue resolves against ITS team's status rows — so a custom
/// status ("In QA") gets its own group and the started clocks follow the
/// team's ramp (¼ · 2/4 · ¾ for three started rows), exactly as the board
/// list and web/desktop draw them. Rows of different teams that share a
/// category and a name ("In Progress" in two teams) MERGE into one group, so a
/// cross-team list never shows the same status twice.
///
/// Group order: category `displayOrder`, then the row's position in its
/// team's resolver order, then name. A merged group takes the glyph, colour
/// and name of its first-ranked row. In-group order = `IssueSorting` on the
/// group's category.
public enum CrossTeamStatusGroups {
    public struct Group: Identifiable, Equatable {
        /// The merge key: `<category>|<lowercased name>`.
        public let id: String
        public let status: ResolvedIssueStatus
        public let issues: [IssueEntity]

        public static func == (lhs: Group, rhs: Group) -> Bool {
            lhs.id == rhs.id && lhs.status == rhs.status && lhs.issues.map(\.id) == rhs.issues.map(\.id)
        }
    }

    /// - Parameters:
    ///   - issues: the list's issues, any teams.
    ///   - teamIdOf: an issue's team (via its board); nil = unknown, which
    ///     resolves against the constructed builtin defaults.
    ///   - statusRows: every synced `issue_statuses` row (all teams).
    public static func groups(
        issues: [IssueEntity],
        teamIdOf: (IssueEntity) -> String?,
        statusRows: [IssueStatusEntity],
        today: String = IssueSorting.todayString()
    ) -> [Group] {
        var teamCache: [String: [ResolvedIssueStatus]] = [:]
        func team(_ teamId: String?) -> [ResolvedIssueStatus] {
            guard let teamId else { return IssueStatusResolver.builtinFallbackTeam }
            if let cached = teamCache[teamId] { return cached }
            let resolved = IssueStatusResolver.teamStatusesOrFallback(
                statusRows.filter { $0.teamId == teamId }
            )
            teamCache[teamId] = resolved
            return resolved
        }

        struct Bucket {
            var status: ResolvedIssueStatus
            var rank: (Int, Int)
            var issues: [IssueEntity]
        }
        var buckets: [String: Bucket] = [:]
        for issue in issues {
            let statuses = team(teamIdOf(issue))
            let status = IssueStatusResolver.resolve(issue, team: statuses)
            let key = mergeKey(status)
            let categoryRank = IssueStatusCategory.displayOrder.firstIndex(of: status.category)
                ?? IssueStatusCategory.displayOrder.count
            let position = statuses.firstIndex(where: { $0.id == status.id }) ?? statuses.count
            let rank = (categoryRank, position)
            if var bucket = buckets[key] {
                if rank < bucket.rank {
                    bucket.status = status
                    bucket.rank = rank
                }
                bucket.issues.append(issue)
                buckets[key] = bucket
            } else {
                buckets[key] = Bucket(status: status, rank: rank, issues: [issue])
            }
        }

        return buckets
            .sorted { a, b in
                if a.value.rank != b.value.rank { return a.value.rank < b.value.rank }
                return a.key < b.key
            }
            .map { entry in
                Group(
                    id: entry.key,
                    status: entry.value.status,
                    issues: IssueSorting.sorted(
                        entry.value.issues, category: entry.value.status.category, today: today
                    )
                )
            }
    }

    static func mergeKey(_ status: ResolvedIssueStatus) -> String {
        "\(status.category.rawValue)|\(status.name.lowercased())"
    }
}
