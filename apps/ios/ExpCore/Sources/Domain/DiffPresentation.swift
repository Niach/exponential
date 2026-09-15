import Foundation

/// EXP-895 — the pure decisions the ONE diff view makes ABOUT a `Diff.File`,
/// kept out of the SwiftUI layer so they can be tested and so the strings stay
/// byte-identical with web's `@exp/ui` (`diff-counts.tsx`, `file-diff-card.tsx`,
/// `file-diff-nav.tsx`, `changes-file-sheet.tsx`).
///
/// The model and its labels (`Diff.summaryLabel`, `additionsLabel`,
/// `deletionsLabel`, `unchangedLabel`) live in `Diff`; only what a VIEW needs
/// on top of them is here.
public enum DiffPresentation {
    /// The phone file sheet's title (web `CHANGED_FILES_TITLE`).
    public static let changedFilesTitle = "Changed files"
    /// Its filter field (web `DIFF_FILTER_PLACEHOLDER`).
    public static let filterPlaceholder = "Filter files"
    /// The word a Changes surface heads its summary with (web `CHANGES_TITLE`).
    public static let changesTitle = "Changes"

    /// The basename of a path — a file list's primary label.
    public static func pathBase(_ path: String) -> String {
        guard let slash = path.lastIndex(of: "/") else { return path }
        return String(path[path.index(after: slash)...])
    }

    /// The directory of a path with its trailing slash DROPPED — the dimmed
    /// crumb a file-list row carries after the basename.
    public static func pathDir(_ path: String) -> String {
        guard let slash = path.lastIndex(of: "/") else { return "" }
        return String(path[path.startIndex..<slash])
    }

    /// The directory of a path WITH its trailing slash — the dimmed run a file
    /// CARD header puts in front of the basename.
    public static func pathDirPrefix(_ path: String) -> String {
        guard let slash = path.lastIndex(of: "/") else { return "" }
        return String(path[path.startIndex...slash])
    }

    /// The one letter a file row leads with.
    public static func statusLetter(_ status: Diff.Status) -> String {
        switch status {
        case .added: "A"
        case .removed: "D"
        case .modified: "M"
        case .renamed: "R"
        case .copied: "C"
        }
    }

    /// What a file with NO hunks says instead of rows. A binary blob, a pure
    /// rename, an empty new file and a patch GitHub refused to send all land
    /// here, and the reader has to be told which.
    public static func noHunksNote(_ file: Diff.File) -> String {
        if file.binary { return "Binary file" }
        switch file.status {
        case .added: return "Empty file added"
        case .removed: return "File removed"
        case .renamed: return "Renamed without content changes"
        case .copied: return "Copied without content changes"
        case .modified: return "No textual diff (binary or too large)"
        }
    }

    /// The file sheet's filter: a case-insensitive substring of the PATH, the
    /// query trimmed. An empty query keeps everything, in order.
    public static func filter(_ files: [Diff.File], query: String) -> [Diff.File] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if needle.isEmpty { return files }
        return files.filter { $0.path.lowercased().contains(needle) }
    }
}

extension PrFile {
    /// One GitHub PullFile → one `Diff.File` (web `fromPullFile`). When the
    /// patch carries no hunks (absent, empty, or a pure rename) GitHub's OWN
    /// additions/deletions are kept — they are the only counts there are.
    public var diffFile: Diff.File {
        var file = Diff.parsePatch(
            path: filename,
            status: Diff.Status.fromPullFile(status),
            patch: patch
        )
        if let previousFilename, !previousFilename.isEmpty {
            file.previousPath = previousFilename
        }
        if file.hunks.isEmpty {
            file.additions = max(0, additions)
            file.deletions = max(0, deletions)
        }
        return file
    }
}
