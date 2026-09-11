import Foundation

/// FEED-30: the ONE "owner/name" shape every repo-by-name entry point accepts
/// — mirror of the web `REPO_FULL_NAME_RE` (`/^[^/\s]+\/[^/\s]+$/`, in
/// `lib/repo-full-name.ts`): exactly one slash, both halves non-empty, no
/// whitespace anywhere. The picker's "Add by name" field validates against it
/// so a name the client lets through is never one the server rejects on
/// shape alone.
public enum RepoFullName {
    public static func isValid(_ value: String) -> Bool {
        let parts = value.split(separator: "/", omittingEmptySubsequences: false)
        guard parts.count == 2 else { return false }
        guard !parts[0].isEmpty, !parts[1].isEmpty else { return false }
        return value.unicodeScalars.allSatisfy { !CharacterSet.whitespacesAndNewlines.contains($0) }
    }
}

/// FEED-32 (web `board-repo-field.tsx` `triggerLabel`): the board form's
/// Repository select label, rendered EXPLICITLY and never blank. A linked
/// repo the local list doesn't know reads "Loading repository…" while the
/// one-shot re-list for that id is in flight (or the first load is), and
/// "Repository unavailable" once the list came back without it; an unlinked
/// board reads "No repository" (or "Loading…" before the first list).
public enum BoardRepoLabel {
    public static let noRepository = "No repository"
    public static let loading = "Loading…"
    public static let loadingRepository = "Loading repository…"
    public static let unavailable = "Repository unavailable"

    /// - Parameters:
    ///   - selectedName: the resolved (inline or registry) repo name, if any.
    ///   - repositoryId: the board's linked registry id, if any.
    ///   - loading: the first list hasn't arrived yet.
    ///   - resolving: the one-shot re-list for `repositoryId` is in flight.
    public static func trigger(
        selectedName: String?,
        repositoryId: String?,
        loading: Bool,
        resolving: Bool
    ) -> String {
        if let selectedName { return selectedName }
        guard repositoryId != nil else { return loading ? Self.loading : Self.noRepository }
        return loading || resolving ? Self.loadingRepository : Self.unavailable
    }
}
