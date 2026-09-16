import Foundation

/// EXP-876 — how a BATCH run is NAMED.
///
/// Every batch row used to read "Batch run" — one string for every batch this
/// team ever ran, so two of them in a list (or one running beside last
/// night's) could not be told apart at all.
///
/// A batch names itself after the issues it covers: the first one's identifier
/// with `+N` for the rest in the mono slot, that issue's title as the subject —
/// the same two-part row an issue run renders, so one layout keeps serving
/// every kind (EXP-874). The trailing control is still none: EXP-893 took the
/// open-issue circle off every session row on every client, and it was exactly
/// the control a multi-issue run could never answer.
///
/// The covered set has two sources, in this order:
///   1. `coding_sessions.batch_issue_ids` — written at start, so the name is
///      right from the run's first second (the composer's order, preserved).
///   2. the issues sharing the row's `branch` — what `pr_open` stamped on both
///      sides (EXP-545), which names batches started before the column existed
///      or by a client too old to send it, from the moment their PR opens.
///
/// The twin of web `lib/batch-run.ts`, desktop `domain::batch_run` and Android
/// `BatchRun.kt`: same order, same `+N`, same fallback string, same test names.
public enum BatchRun {
    /// The one string a batch with no knowable issues shows. Byte-identical ×4.
    public static let fallback = "Batch run"

    /// The launcher's batch branch marker (`exp/batch-<id8>`), deliberately
    /// lowercase so it can never parse as an issue branch.
    public static let branchPrefix = "exp/batch-"

    /// What a batch row shows.
    public struct Name: Equatable {
        /// The mono lead-in — nil when no covered issue is known.
        public let identifier: String?
        public let subject: String

        public init(identifier: String?, subject: String) {
            self.identifier = identifier
            self.subject = subject
        }
    }

    /// Read `coding_sessions.batch_issue_ids`. Same tolerance as every other
    /// jsonb column here: the store hands it over as raw TEXT, and anything
    /// that is not a list of non-empty strings names nothing rather than
    /// throwing.
    public static func issueIds(_ raw: String?) -> [String] {
        guard let raw, let data = raw.data(using: .utf8),
            let parsed = try? JSONSerialization.jsonObject(with: data),
            let items = parsed as? [Any]
        else { return [] }
        return items.compactMap { $0 as? String }.filter { !$0.isEmpty }
    }

    /// An issue-less, action-less run — the batch. (A chat run carries the
    /// reserved `Chat` snapshot, an action run its own, EXP-615.)
    public static func isBatch(_ session: CodingSessionEntity) -> Bool {
        session.issueId == nil && session.actionName == nil
    }

    /// The issues a batch run covers, in NAMING order: the stored order when
    /// the row recorded it, else the branch-mates oldest first (a
    /// deterministic order every client reaches the same way — `created_at` is
    /// on every issue row, identifiers break the tie).
    public static func issues(
        _ session: CodingSessionEntity,
        issues: [IssueEntity]
    ) -> [IssueEntity] {
        guard isBatch(session) else { return [] }
        let ids = issueIds(session.batchIssueIds)
        if !ids.isEmpty {
            let byId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            return ids.compactMap { byId[$0] }
        }
        guard let branch = session.branch, branch.hasPrefix(branchPrefix) else { return [] }
        return
            issues
            .filter { $0.branch == branch }
            .sorted { a, b in
                if a.createdAt != b.createdAt { return a.createdAt < b.createdAt }
                return (a.identifier ?? "") < (b.identifier ?? "")
            }
    }

    /// Name a batch run. `issues` is whatever the caller has synced; only the
    /// covered ones are read. A batch whose issues are all unknown (no stored
    /// ids, no PR yet — or a row whose issues left the viewer's teams) keeps
    /// the old generic label rather than inventing one.
    public static func name(
        _ session: CodingSessionEntity,
        issues: [IssueEntity]
    ) -> Name {
        let covered = Self.issues(session, issues: issues)
        guard let first = covered.first else {
            return Name(identifier: nil, subject: fallback)
        }
        // The STORED count wins over the resolved one: a batch of three whose
        // middle issue has not synced is still a batch of three, and "+1"
        // would quietly understate what the run is working on.
        let total = max(issueIds(session.batchIssueIds).count, covered.count)
        let title = first.title.trimmingCharacters(in: .whitespacesAndNewlines)
        // `identifier` is optional in this client's schema only (server-side
        // a trigger always writes one) — a row that somehow lacks it still
        // names the run by its title rather than falling back to "Batch run".
        let identifier = first.identifier ?? ""
        return Name(
            identifier: identifier.isEmpty
                ? nil
                : (total > 1 ? "\(identifier) +\(total - 1)" : identifier),
            subject: title.isEmpty ? "Untitled issue" : title
        )
    }
}
