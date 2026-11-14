import Foundation

/// #EXP-1832: sidebar folders, the Swift mirror of
/// packages/domain-contract/src/sidebar-folders.ts (fixture
/// `fixtures/sidebar-folders.json`, its `$comment` is the spec; web, desktop
/// `domain::sidebar_folders` and Android `SidebarFolders.kt` replay it too).
///
/// One synced `sidebar_layout` row per (member, team, section) node: a FOLDER
/// (a name, never navigable) or an ITEM entry placing one board, agent or app,
/// optionally inside a folder. Two layers max. Phones only READ the layout
/// (`tree` + `rows`); `move` / `ungroup` are mirrored for the fixture replay.
public enum SidebarFolders {
    public static let sections = ["board", "agent", "app"]
    public static let defaultFolderName = "New folder"
    /// Nesting levels: top level + one folder level.
    public static let maxDepth = 2
    public static let folderNameMax = 80

    public enum Kind: String, Sendable { case folder, item }

    /// One synced `sidebar_layout` row of a section.
    public struct Entry: Equatable, Sendable {
        public var id: String
        public var kind: Kind
        public var targetId: String?
        public var parentId: String?
        public var name: String?
        public var sortOrder: Double

        public init(id: String, kind: Kind, targetId: String?, parentId: String?, name: String?, sortOrder: Double) {
            self.id = id
            self.kind = kind
            self.targetId = targetId
            self.parentId = parentId
            self.name = name
            self.sortOrder = sortOrder
        }
    }

    public enum Node: Equatable, Sendable {
        /// The target id (board / agent / app).
        case item(id: String)
        /// The folder entry's id; children = target ids, never empty.
        case folder(id: String, name: String, children: [String])

        public var id: String {
            switch self {
            case let .item(id): return id
            case let .folder(id, _, _): return id
            }
        }
    }

    /// A List `move` event: exactly one of before / after / into.
    public struct Move: Equatable, Sendable {
        public var id: String
        public var before: String?
        public var after: String?
        public var into: String?

        public init(id: String, before: String? = nil, after: String? = nil, into: String? = nil) {
            self.id = id
            self.before = before
            self.after = after
            self.into = into
        }
    }

    /// One row to write; `id` nil = a new item entry (upserted by target).
    public struct EntryWrite: Equatable, Sendable {
        public var id: String?
        public var kind: Kind
        public var targetId: String?
        public var parentId: String?
        public var name: String?
        public var sortOrder: Double
    }

    public struct Writes: Equatable, Sendable {
        public var upserts: [EntryWrite]
        public var deletes: [String]
        public var createdFolderId: String?
    }

    /// One List row: a folder row carries `group` and its item count; a
    /// closed folder's items are left out.
    public struct Row: Equatable, Sendable, Identifiable {
        public var kind: Kind
        /// Node id (target id for an item, entry id for a folder).
        public var id: String
        public var depth: Int
        public var group: Bool
        public var name: String?
        public var open: Bool
        public var count: Int
    }

    private static func ordered(_ a: Entry, _ b: Entry) -> Bool {
        a.sortOrder != b.sortOrder ? a.sortOrder < b.sortOrder : a.id < b.id
    }

    /// The section as shown: placed nodes first by sortOrder (ties by entry
    /// id), then every item with no entry in default order.
    public static func tree(items: [String], entries: [Entry]) -> [Node] {
        let visible = Set(items)
        var folders: [String: Entry] = [:]
        var folderOrder: [String] = []
        var itemEntries: [String: Entry] = [:]
        var itemOrder: [String] = []
        for entry in entries.sorted(by: ordered) {
            if entry.kind == .folder {
                if folders[entry.id] == nil { folderOrder.append(entry.id) }
                folders[entry.id] = entry
            } else if let target = entry.targetId, visible.contains(target), itemEntries[target] == nil {
                itemEntries[target] = entry
                itemOrder.append(target)
            }
        }
        var children: [String: [Entry]] = [:]
        var top: [Entry] = []
        for target in itemOrder {
            let entry = itemEntries[target]!
            if let parent = entry.parentId, folders[parent] != nil {
                children[parent, default: []].append(entry)
            } else {
                top.append(entry)
            }
        }
        for id in folderOrder where children[id] != nil { top.append(folders[id]!) }
        top.sort(by: ordered)
        var nodes: [Node] = top.map { entry in
            entry.kind == .folder
                ? .folder(
                    id: entry.id,
                    name: entry.name ?? defaultFolderName,
                    children: (children[entry.id] ?? []).sorted(by: ordered).compactMap(\.targetId)
                )
                : .item(id: entry.targetId ?? "")
        }
        for item in items where itemEntries[item] == nil { nodes.append(.item(id: item)) }
        return nodes
    }

    /// Where a node sits: the index of its folder in the tree (nil = top level).
    private static func locate(_ tree: [Node], _ id: String) -> (folder: Int?, node: Node)? {
        for (index, node) in tree.enumerated() {
            if node.id == id { return (nil, node) }
            if case let .folder(_, _, children) = node, children.contains(id) { return (index, .item(id: id)) }
        }
        return nil
    }

    private static func materialize(_ tree: [Node], _ entries: [Entry], _ deletes: [String], _ created: String?) -> Writes {
        let byId = Dictionary(entries.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        var byTarget: [String: Entry] = [:]
        for entry in entries.sorted(by: ordered) where entry.kind == .item {
            if let target = entry.targetId, byTarget[target] == nil { byTarget[target] = entry }
        }
        var upserts: [EntryWrite] = []
        func writeItem(_ target: String, _ parent: String?, _ sortOrder: Double) {
            let entry = byTarget[target]
            if let entry, entry.parentId == parent, entry.sortOrder == sortOrder { return }
            upserts.append(EntryWrite(id: entry?.id, kind: .item, targetId: target, parentId: parent, name: nil, sortOrder: sortOrder))
        }
        for (index, node) in tree.enumerated() {
            let order = Double(index + 1)
            switch node {
            case let .item(id):
                writeItem(id, nil, order)
            case let .folder(id, name, children):
                let entry = byId[id]
                if entry == nil || entry?.sortOrder != order || entry?.name != name || entry?.parentId != nil {
                    upserts.append(EntryWrite(id: id, kind: .folder, targetId: nil, parentId: nil, name: name, sortOrder: order))
                }
                for (childIndex, child) in children.enumerated() { writeItem(child, id, Double(childIndex + 1)) }
            }
        }
        return Writes(upserts: upserts, deletes: deletes, createdFolderId: created)
    }

    private static func removeChild(_ tree: inout [Node], folder index: Int, _ id: String) {
        guard case let .folder(fid, name, children) = tree[index] else { return }
        tree[index] = .folder(id: fid, name: name, children: children.filter { $0 != id })
    }

    /// Applies a List `move`; nil = refused (nothing written).
    public static func move(items: [String], entries: [Entry], move: Move, newFolderId: String) -> Writes? {
        let positions: [(String, String)] = [("before", move.before), ("after", move.after), ("into", move.into)]
            .compactMap { key, value in value.map { (key, $0) } }
        guard positions.count == 1 else { return nil }
        let (position, targetId) = positions[0]
        guard targetId != move.id else { return nil }
        var tree = tree(items: items, entries: entries)
        guard let dragged = locate(tree, move.id), let target = locate(tree, targetId) else { return nil }
        if case .folder = dragged.node, position == "into" || target.folder != nil { return nil }
        if position == "into", case .item = target.node, target.folder != nil { return nil }

        // Take the dragged node out (folder ids stay stable; indexes are re-found).
        let sourceId = dragged.folder.map { tree[$0].id }
        if let source = dragged.folder {
            removeChild(&tree, folder: source, move.id)
        } else {
            tree.removeAll { $0.id == move.id }
        }

        var created: String?
        if position == "into" {
            let index = tree.firstIndex { $0.id == targetId }!
            if case let .folder(fid, name, children) = tree[index] {
                tree[index] = .folder(id: fid, name: name, children: children + [move.id])
            } else {
                created = newFolderId
                tree[index] = .folder(id: newFolderId, name: defaultFolderName, children: [targetId, move.id])
            }
        } else if target.folder != nil, let folderIndex = tree.firstIndex(where: { node in
            if case let .folder(_, _, children) = node { return children.contains(targetId) }
            return false
        }) {
            guard case let .folder(fid, name, children) = tree[folderIndex] else { return nil }
            var list = children
            list.insert(move.id, at: list.firstIndex(of: targetId)! + (position == "after" ? 1 : 0))
            tree[folderIndex] = .folder(id: fid, name: name, children: list)
        } else {
            let index = tree.firstIndex { $0.id == targetId }!
            tree.insert(dragged.node, at: index + (position == "after" ? 1 : 0))
        }

        var deletes: [String] = []
        if let sourceId, let index = tree.firstIndex(where: { $0.id == sourceId }),
           case let .folder(_, _, children) = tree[index], children.isEmpty {
            tree.remove(at: index)
            deletes.append(sourceId)
        }
        return materialize(tree, entries, deletes, created)
    }

    /// Ungroup: the folder's items take its place at the top level; the
    /// folder is deleted. Nil = no such (visible) folder.
    public static func ungroup(items: [String], entries: [Entry], folderId: String) -> Writes? {
        var tree = tree(items: items, entries: entries)
        guard let index = tree.firstIndex(where: { if case .folder = $0 { return $0.id == folderId } else { return false } }),
              case let .folder(_, _, children) = tree[index] else { return nil }
        tree.replaceSubrange(index ... index, with: children.map { Node.item(id: $0) })
        return materialize(tree, entries, [folderId], nil)
    }

    /// A folder name as saved: trimmed, at most 80 characters, empty = default.
    public static func folderName(_ input: String) -> String {
        let name = String(input.trimmingCharacters(in: .whitespacesAndNewlines).prefix(folderNameMax))
        return name.isEmpty ? defaultFolderName : name
    }

    /// The device-local expanded flag's key (UserDefaults); no flag = OPEN.
    public static func openKey(teamId: String, folderId: String) -> String {
        "sidebar-folder-open:\(teamId):\(folderId)"
    }

    public static func rows(_ tree: [Node], isOpen: (String) -> Bool) -> [Row] {
        var rows: [Row] = []
        for node in tree {
            switch node {
            case let .item(id):
                rows.append(Row(kind: .item, id: id, depth: 0, group: false, name: nil, open: false, count: 0))
            case let .folder(id, name, children):
                let open = isOpen(id)
                rows.append(Row(kind: .folder, id: id, depth: 0, group: true, name: name, open: open, count: children.count))
                if open {
                    for child in children {
                        rows.append(Row(kind: .item, id: child, depth: 1, group: false, name: nil, open: false, count: 0))
                    }
                }
            }
        }
        return rows
    }

    /// The UserDefaults-backed expanded flag (default open).
    public static func isOpen(teamId: String, folderId: String, defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: openKey(teamId: teamId, folderId: folderId)) as? Bool ?? true
    }

    public static func setOpen(_ open: Bool, teamId: String, folderId: String, defaults: UserDefaults = .standard) {
        defaults.set(open, forKey: openKey(teamId: teamId, folderId: folderId))
    }
}
