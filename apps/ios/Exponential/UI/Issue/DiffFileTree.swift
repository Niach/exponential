import ExpCore
import ExpUI
import SwiftUI

/// EXP-916 — the file TREE of a Changes surface, ×4 (web `@exp/ui`
/// `FileDiffTree`, desktop `diff::file_tree`, Android `DiffFileTree`).
///
/// A phone has no room for a column beside the cards, so the tree lives in the
/// file SHEET: folders (VS Code's compact chain, `apps/web/src` as one row)
/// over their files, every folder open, a tap folding one away. A non-blank
/// query drops the tree for the FLAT match list — a filter is a search result,
/// not a pruned tree. The shape is `DiffTree.fileTree`'s and nothing here
/// sorts, sums or compacts anything itself.
///
/// A pick reports the path; the sheet closes and the card list expands that
/// file and scrolls to it.
struct DiffFileTree: View {
    let files: [Diff.File]
    /// The sheet's filter, already bound to its search field.
    var query: String = ""
    var selected: String?
    let onSelect: (String) -> Void

    /// Folders the reader FOLDED away — everything else is open.
    @State private var collapsed: Set<String> = []
    /// The built tree, kept across body passes — folding one row must not
    /// re-split, re-sort and re-sum every path in the diff.
    @State private var memo = DiffTreeMemo()

    var body: some View {
        let rows = flatten(memo.tree(files, query: query))
        VStack(alignment: .leading, spacing: 0) {
            if rows.isEmpty {
                Text(files.isEmpty ? "No changed files." : "No matching files.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, GlassSheetTokens.headerHPadding)
                    .padding(.vertical, 12)
            }
            ForEach(Array(rows.enumerated()), id: \.element.node.id) { index, row in
                if index > 0 { GlassDivider() }
                if row.node.kind == .dir {
                    folderRow(row)
                } else {
                    fileRow(row)
                }
            }
        }
    }

    // MARK: - Rows

    private func folderRow(_ row: Row) -> some View {
        let open = !collapsed.contains(row.node.path)
        return Button {
            if open {
                collapsed.insert(row.node.path)
            } else {
                collapsed.remove(row.node.path)
            }
        } label: {
            HStack(spacing: 8) {
                AppIcon(open ? AppIcons.uiFolderOpen : AppIcons.uiFolder, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(row.node.name)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                DiffCountsLabel(
                    additions: row.node.additions, deletions: row.node.deletions
                )
                AppIcon(open ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.leading, indent(row.depth))
            .padding(.trailing, 12)
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("changes-file-tree-folder")
    }

    /// `letter · name · counts` — the basename leads; the directory is the row
    /// the file sits under, so it is never repeated here.
    private func fileRow(_ row: Row) -> some View {
        Button {
            onSelect(row.node.path)
        } label: {
            HStack(spacing: 8) {
                if let status = files[safe: row.node.index]?.status {
                    DiffStatusLetter(status: status)
                }
                Text(row.node.name)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                DiffCountsLabel(
                    additions: row.node.additions, deletions: row.node.deletions
                )
            }
            .padding(.leading, indent(row.depth))
            .padding(.trailing, 12)
            .padding(.vertical, 10)
            .background(row.node.path == selected ? GlassTokens.fillActive : .clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("changes-file-list-row")
    }

    // MARK: - Flattening

    private struct Row {
        let node: DiffTree.Node
        let depth: Int
    }

    /// One nesting step. The tree is already compacted, so the depth is small.
    private static let indentStep: CGFloat = 12

    private func indent(_ depth: Int) -> CGFloat {
        12 + CGFloat(depth) * Self.indentStep
    }

    private func flatten(_ nodes: [DiffTree.Node]) -> [Row] {
        var out: [Row] = []
        walk(nodes, depth: 0, into: &out)
        return out
    }

    private func walk(_ nodes: [DiffTree.Node], depth: Int, into out: inout [Row]) {
        for node in nodes {
            out.append(Row(node: node, depth: depth))
            guard node.kind == .dir, !collapsed.contains(node.path) else { continue }
            walk(node.children, depth: depth + 1, into: &out)
        }
    }
}

/// EXP-916 — a cheap identity for a file set: its paths and their counts. Two
/// diffs that hash the same draw the same tree and the same totals.
func diffFilesKey(_ files: [Diff.File]) -> Int {
    var hasher = Hasher()
    hasher.combine(files.count)
    for file in files {
        hasher.combine(file.path)
        hasher.combine(file.additions)
        hasher.combine(file.deletions)
    }
    return hasher.finalize()
}

/// EXP-916 — the tree cache (the same idea as `EditCardMemo`).
///
/// `DiffTree.fileTree` splits, sorts, compacts and sums every path, and a
/// SwiftUI body runs on any state change — folding ONE folder must not rebuild
/// the whole tree. The key is the files' identity plus the trimmed query;
/// nothing else can change what the builder returns.
///
/// Deliberately NOT `@Observable`: the cache is written DURING a body pass, and
/// an observed write there would invalidate the view that just read it.
@MainActor
final class DiffTreeMemo {
    private var key: Int?
    private var cached: [DiffTree.Node] = []

    func tree(_ files: [Diff.File], query: String) -> [DiffTree.Node] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        var hasher = Hasher()
        hasher.combine(needle)
        hasher.combine(diffFilesKey(files))
        let key = hasher.finalize()
        if key == self.key { return cached }
        let made = DiffTree.fileTree(files, query: needle)
        self.key = key
        cached = made
        return made
    }
}

private extension Array {
    /// A tree node's index is the input file's — a filtered list can still hand
    /// back an index this array no longer has while a refresh is in flight.
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
