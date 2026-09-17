import Foundation

/// EXP-916 — the file TREE a Changes surface lists beside its file cards: the
/// sidebar on web (≥md) and the desktop, the file SHEET on a phone
/// (`DiffFileTree`). One pure builder over the shared `Diff.File` model,
/// hand-mirrored ×4 (TS `@exp/domain-contract` `diff-tree.ts`, web `@exp/ui`
/// `FileDiffTree`, desktop `domain::diff_tree`, Android `domain/DiffTree.kt`)
/// and byte-locked by `packages/domain-contract/fixtures/diff/tree.json`.
///
/// Rules:
/// - a path splits on `/`; every segment but the last is a directory;
/// - per level, directories come before files, each group sorted by the
///   lower-cased name compared by UTF-16 CODE UNIT (never a locale compare, and
///   never Swift's default `String` ordering — neither is the JS `<` the
///   fixture was written against), ties broken by the raw name the same way;
/// - a directory with exactly ONE child, a directory, and no files of its own
///   compacts into that child — `apps/web/src` is one node (VS Code's compact
///   folders), and the compaction repeats down the chain;
/// - a directory's `additions`/`deletions`/`files` are its subtree sums;
/// - a non-blank `query` returns a FLAT list of the FILE nodes whose path
///   contains it, case-insensitively, in INPUT order — the filter is a search
///   result, not a pruned tree.
public enum DiffTree {
    public enum Kind: String, Equatable, Sendable {
        case dir
        case file
    }

    public struct Node: Equatable, Sendable, Identifiable {
        public let kind: Kind
        /// The full path from the root (`apps/web/src` for a compacted dir).
        public let path: String
        /// The label: the last segment, or the compacted chain `a/b/c`.
        public let name: String
        public let additions: Int
        public let deletions: Int
        /// Files in the subtree (1 for a file).
        public let files: Int
        /// The index into the input `files`; -1 for a directory.
        public let index: Int
        public let children: [Node]

        /// A directory and a file can share a path only across levels, so the
        /// kind is part of the identity the list renders by.
        public var id: String { "\(kind.rawValue):\(path)" }

        public init(
            kind: Kind,
            path: String,
            name: String,
            additions: Int,
            deletions: Int,
            files: Int,
            index: Int,
            children: [Node]
        ) {
            self.kind = kind
            self.path = path
            self.name = name
            self.additions = additions
            self.deletions = deletions
            self.files = files
            self.index = index
            self.children = children
        }
    }

    /// The tree (or, with a `query`, the flat match list) for one file set.
    public static func fileTree(_ files: [Diff.File], query: String = "") -> [Node] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if !needle.isEmpty {
            // UTF-16 code units, like the JS `includes` the fixture locks (and
            // Kotlin's `contains`, and Rust's on bytes). Swift's own `contains`
            // matches CANONICALLY, so an NFD path would answer a needle typed
            // in NFC — a different file set than the other three return.
            let pin = Array(needle.utf16)
            var out: [Node] = []
            for (index, file) in files.enumerated()
            where codeUnitsContain(Array(file.path.lowercased().utf16), pin) {
                out.append(leaf(file, index: index, name: file.path))
            }
            return out
        }
        let root = Building(path: "", name: "")
        for (index, file) in files.enumerated() {
            let segments = file.path.components(separatedBy: "/")
            var at = root
            for i in 0..<max(segments.count - 1, 0) {
                let name = segments[i]
                let path = segments[0...i].joined(separator: "/")
                at = at.child(name: name, path: path)
            }
            at.leaves.append(
                leaf(file, index: index, name: segments[segments.count - 1])
            )
        }
        return finish(root).children
    }

    /// The byte-lock projection: one line per node, two spaces per depth, a
    /// directory as `name +a -d (files)` and a file as `name +a -d`. ASCII `-`,
    /// like `Diff.render`.
    public static func render(_ nodes: [Node]) -> [String] {
        var out: [String] = []
        walk(nodes, depth: 0, into: &out)
        return out
    }

    private static func walk(_ nodes: [Node], depth: Int, into out: inout [String]) {
        for node in nodes {
            let indent = String(repeating: "  ", count: depth)
            let counts = "+\(node.additions) -\(node.deletions)"
            out.append(
                node.kind == .dir
                    ? "\(indent)\(node.name) \(counts) (\(node.files))"
                    : "\(indent)\(node.name) \(counts)"
            )
            walk(node.children, depth: depth + 1, into: &out)
        }
    }

    // MARK: - Building

    /// A directory under construction. A reference type so the walk down a
    /// path can hold onto the node it is filling, and its children keep
    /// INSERTION order until `finish` sorts them.
    private final class Building {
        let path: String
        let name: String
        var dirOrder: [String] = []
        var dirs: [String: Building] = [:]
        var leaves: [Node] = []

        init(path: String, name: String) {
            self.path = path
            self.name = name
        }

        func child(name: String, path: String) -> Building {
            if let existing = dirs[name] { return existing }
            let made = Building(path: path, name: name)
            dirs[name] = made
            dirOrder.append(name)
            return made
        }
    }

    private static func finish(_ dir: Building) -> Node {
        var dirs = dir.dirOrder.compactMap { dir.dirs[$0] }
        var leaves = dir.leaves
        var path = dir.path
        var name = dir.name
        // Compact a lone child directory into this one, down the chain.
        while dirs.count == 1, leaves.isEmpty, !path.isEmpty {
            let only = dirs[0]
            path = only.path
            name = "\(name)/\(only.name)"
            dirs = only.dirOrder.compactMap { only.dirs[$0] }
            leaves = only.leaves
        }
        let children = dirs.map(finish).sorted(by: byName) + leaves.sorted(by: byName)
        var additions = 0
        var deletions = 0
        var files = 0
        for child in children {
            additions += child.additions
            deletions += child.deletions
            files += child.files
        }
        return Node(
            kind: .dir,
            path: path,
            name: name,
            additions: additions,
            deletions: deletions,
            files: files,
            index: -1,
            children: children
        )
    }

    private static func leaf(_ file: Diff.File, index: Int, name: String) -> Node {
        Node(
            kind: .file,
            path: file.path,
            name: name,
            additions: file.additions,
            deletions: file.deletions,
            files: 1,
            index: index,
            children: []
        )
    }

    /// Lower-cased code-unit order, then the raw name — portable everywhere.
    /// EVERY rung compares CODE UNITS, the equality too: Swift's `==`/`!=` on
    /// `String` is canonical, so an NFC name and its NFD twin would fall
    /// through both rungs as "equal" and land in whatever order the (unstable)
    /// sort left them — while JS, Rust and Kotlin order them by code unit.
    private static func byName(_ a: Node, _ b: Node) -> Bool {
        let la = Array(a.name.lowercased().utf16)
        let lb = Array(b.name.lowercased().utf16)
        if la != lb { return codeUnitsLess(la, lb) }
        let ra = Array(a.name.utf16)
        let rb = Array(b.name.utf16)
        if ra != rb { return codeUnitsLess(ra, rb) }
        return false
    }

    /// JavaScript's `<` on strings: UTF-16 code units, lexicographically.
    /// Swift's own `<` normalizes and orders by grapheme, which is a DIFFERENT
    /// answer for the same pair — and the fixture locks the JS one.
    private static func codeUnitsLess(_ a: [UInt16], _ b: [UInt16]) -> Bool {
        var i = 0
        while i < a.count, i < b.count {
            if a[i] != b[i] { return a[i] < b[i] }
            i += 1
        }
        return a.count < b.count
    }

    /// JavaScript's `includes` on strings: a UTF-16 code-unit substring.
    private static func codeUnitsContain(_ haystack: [UInt16], _ needle: [UInt16]) -> Bool {
        if needle.isEmpty { return true }
        if needle.count > haystack.count { return false }
        for start in 0...(haystack.count - needle.count) {
            var i = 0
            while i < needle.count, haystack[start + i] == needle[i] { i += 1 }
            if i == needle.count { return true }
        }
        return false
    }
}
