import Foundation

/// EXP-895 — the pure decisions the ONE diff view makes ABOUT a `Diff.File`,
/// kept out of the SwiftUI layer so they can be tested and so the strings stay
/// byte-identical with web's `@exp/ui` (`diff-counts.tsx`, `file-diff-card.tsx`,
/// `file-diff-tree.tsx`, `changes-file-sheet.tsx`).
///
/// The model and its labels (`Diff.summaryLabel`, `additionsLabel`,
/// `deletionsLabel`, `unchangedLabel`) live in `Diff`; only what a VIEW needs
/// on top of them is here.
public enum DiffPresentation {
    /// The phone file sheet's title. EXP-916: the contract's word, ×4.
    public static let changedFilesTitle = DomainContract.diffUiChangedFilesTitle
    /// Its filter field (contract `diffUi.filterPlaceholder`).
    public static let filterPlaceholder = DomainContract.diffUiFilterPlaceholder
    /// The word a Changes surface heads its summary with (web `CHANGES_TITLE`).
    public static let changesTitle = "Changes"

    /// EXP-916: a file with MORE hunk lines than this starts collapsed.
    public static let collapseThresholdLines = DomainContract.diffUiCollapseThresholdLines

    /// The hunk lines of a file — what the size rule is measured in.
    public static func lineCount(_ file: Diff.File) -> Int {
        file.hunks.reduce(0) { $0 + $1.lines.count }
    }

    /// EXP-916 — whether a file card opens by itself at this list's setting.
    /// Every Changes surface now starts EXPANDED; only sheer size folds a file
    /// away, and a review list that asks for collapsed still gets it.
    /// Mirrored ×4 (web/Android `diffOpensByDefault`, desktop
    /// `diff_opens_by_default`).
    public static func diffOpensByDefault(
        _ file: Diff.File, defaultCollapsed: Bool
    ) -> Bool {
        !defaultCollapsed && lineCount(file) <= collapseThresholdLines
    }

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
}

extension PrFile {
    /// One GitHub PullFile → one `Diff.File`, through the ONE mapping the
    /// contract fixture's `pullFile` cases lock (`Diff.fromPullFile`).
    public var diffFile: Diff.File {
        Diff.fromPullFile(
            filename: filename,
            previousFilename: previousFilename,
            status: status,
            additions: additions,
            deletions: deletions,
            patch: patch
        )
    }
}
