import ExpUI
import SwiftUI

/// Shared unified-diff line classification + coloring — the ONE place both the
/// dedicated Changes page (EXP-34) and the agent-session "Latest changes" sheet
/// (EXP-32) get their diff look from, so patches render identically everywhere.
enum DiffLineKind {
    case hunk
    case addition
    case deletion
    /// File headers (`diff --git`, `index`, `+++`/`---`, mode lines) — plain.
    case meta
    case context
}

enum DiffRendering {
    struct Line: Identifiable {
        let id: Int
        let text: String
        let kind: DiffLineKind
    }

    struct FileSection: Identifiable {
        let id: Int
        /// Parsed from the `diff --git a/… b/…` header; nil for a headerless
        /// single-patch blob (e.g. a bare GitHub `patch` fragment).
        let filename: String?
        let patch: String
    }

    static func kind(of line: some StringProtocol) -> DiffLineKind {
        if line.hasPrefix("@@") { return .hunk }
        if line.hasPrefix("+++") || line.hasPrefix("---") { return .meta }
        if line.hasPrefix("diff --git") || line.hasPrefix("index ")
            || line.hasPrefix("new file") || line.hasPrefix("deleted file")
            || line.hasPrefix("rename ") || line.hasPrefix("similarity ")
            || line.hasPrefix("old mode") || line.hasPrefix("new mode")
            || line.hasPrefix("Binary files") {
            return .meta
        }
        if line.hasPrefix("+") { return .addition }
        if line.hasPrefix("-") { return .deletion }
        return .context
    }

    static func color(_ kind: DiffLineKind) -> Color {
        switch kind {
        case .hunk: Accent.indigo
        case .addition: .green
        case .deletion: .red
        case .meta: .white.opacity(TextOpacity.tertiary)
        case .context: .white.opacity(TextOpacity.secondary)
        }
    }

    static func background(_ kind: DiffLineKind) -> Color {
        switch kind {
        case .addition: Color.green.opacity(0.08)
        case .deletion: Color.red.opacity(0.08)
        default: .clear
        }
    }

    /// `+A −D` counts: body `+`/`-` lines, excluding the `+++`/`---` headers.
    static func stats(of diff: String) -> (additions: Int, deletions: Int) {
        var additions = 0
        var deletions = 0
        for line in diff.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("+++") || line.hasPrefix("---") { continue }
            if line.hasPrefix("+") { additions += 1 } else if line.hasPrefix("-") { deletions += 1 }
        }
        return (additions, deletions)
    }

    /// A patch's lines, classified and right-padded to a common width so the
    /// per-line background tints form a uniform block inside the horizontal
    /// scroller (the font is monospaced, so equal character count == equal
    /// width). Capped at `maxLines` to keep huge diffs from choking layout.
    static func lines(of patch: String, maxLines: Int = 600) -> (lines: [Line], truncated: Bool) {
        var raw = patch.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        let truncated = raw.count > maxLines
        if truncated { raw = Array(raw.prefix(maxLines)) }
        let maxLen = raw.map(\.count).max() ?? 0
        let lines = raw.enumerated().map { index, line in
            let padded = line.count < maxLen
                ? line + String(repeating: " ", count: maxLen - line.count)
                : line
            return Line(id: index, text: padded.isEmpty ? " " : padded, kind: kind(of: line))
        }
        return (lines, truncated)
    }

    /// Split a multi-file unified diff on `diff --git` boundaries. A diff with
    /// no such header comes back as a single unnamed section.
    static func splitFiles(_ diff: String) -> [FileSection] {
        let allLines = diff.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        var sections: [FileSection] = []
        var current: [String] = []
        var currentName: String?

        func flush() {
            let body = current.joined(separator: "\n")
            guard !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
            sections.append(FileSection(id: sections.count, filename: currentName, patch: body))
        }

        for line in allLines {
            if line.hasPrefix("diff --git ") {
                flush()
                current = [line]
                currentName = filename(fromDiffGitHeader: line)
            } else {
                current.append(line)
            }
        }
        flush()
        return sections
    }

    /// `diff --git a/path b/path` → `path` (the post-image side).
    private static func filename(fromDiffGitHeader line: String) -> String? {
        guard let range = line.range(of: " b/", options: .backwards) else { return nil }
        let name = String(line[range.upperBound...])
        return name.isEmpty ? nil : name
    }
}

/// One patch rendered as a colored, monospaced block. Horizontal panning stays
/// INSIDE this block (each line is single-line + fixed-size, the scroller owns
/// the horizontal axis) — the page around it only ever scrolls vertically.
struct DiffPatchBlock: View {
    let patch: String

    var body: some View {
        let rendered = DiffRendering.lines(of: patch)
        VStack(alignment: .leading, spacing: 0) {
            ScrollView(.horizontal, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(rendered.lines) { line in
                        Text(line.text)
                            .font(.system(.caption2, design: .monospaced))
                            .foregroundStyle(DiffRendering.color(line.kind))
                            .background(DiffRendering.background(line.kind))
                            .lineLimit(1)
                            .fixedSize(horizontal: true, vertical: false)
                    }
                }
                .padding(8)
                .textSelection(.enabled)
            }
            if rendered.truncated {
                Text("Diff truncated — showing the first \(rendered.lines.count) lines.")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 8)
                    .padding(.bottom, 8)
            }
        }
        .background(Color.white.opacity(0.03))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}
