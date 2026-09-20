import Foundation

// EXP-920: the ONE entity-preview rule. An Exponential MCP tool's settled
// transcript row carries `preview.refs` (contract `entityRefKind`), and every
// client draws those refs the SAME way: one CHIP per ref (a glyph + a short
// label), a card resolved from the client's own synced rows, and a tap that
// opens the entity's detail screen.
//
// Hand-mirrored ×4 (web `@exp/domain-contract/entity-preview`, desktop
// `domain::entity_preview`, Android `domain/EntityPreview.kt`) and byte-locked
// by `packages/domain-contract/fixtures/entity-chip.json`
// (`EntityPreviewTests`). Every doc comment here IS the rule; the TS module
// is the reference wording.

/// One entity an Exponential tool's answer named — the wire shape
/// (`tool_update.preview.refs[]`), every string already clamped by the
/// publisher to `expToolPreview.textMax`.
public struct EntityRef: Equatable, Sendable {
    /// A contract `entityRefKind` value (`parse` drops anything else); a
    /// `list` names its MEMBER kind in `id`.
    public let kind: String
    public let id: String
    /// The subject's human identifier (`EXP-849`) when it has one.
    public let identifier: String?
    public let title: String?
    /// A `list` ref's member count.
    public let count: Int?

    public init(
        kind: String,
        id: String,
        identifier: String? = nil,
        title: String? = nil,
        count: Int? = nil
    ) {
        self.kind = kind
        self.id = id
        self.identifier = identifier
        self.title = title
        self.count = count
    }

    /// The wire parser (web `parseEntityRef`): the kind must be one this build
    /// knows, the id trimmed and non-empty; `identifier`/`title` are trimmed
    /// and dropped when blank, every string cut to `expToolPreviewTextMax`;
    /// a `count` must be a finite non-negative number and is rounded. Nil for
    /// anything else — an unknown kind from a NEWER publisher is skipped, not
    /// rendered as a mystery chip.
    public static func parse(_ raw: Any?) -> EntityRef? {
        guard let row = raw as? [String: Any],
              let kind = row["kind"] as? String,
              DomainContract.entityRefKindValues.contains(kind),
              let id = clampedText(row["id"])
        else { return nil }
        var count: Int?
        if let number = row["count"] as? NSNumber {
            let value = number.doubleValue
            if value.isFinite, value >= 0 {
                count = Int(value.rounded())
            }
        }
        return EntityRef(
            kind: kind,
            id: id,
            identifier: clampedText(row["identifier"]),
            title: clampedText(row["title"]),
            count: count
        )
    }

    /// A wire string, trimmed, nil when blank, cut to the contract's text cap.
    private static func clampedText(_ value: Any?) -> String? {
        guard let text = value as? String else { return nil }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return String(trimmed.prefix(DomainContract.expToolPreviewTextMax))
    }
}

public enum EntityPreview {
    /// A chip's label never runs past this many characters (code points); a
    /// longer one is cut to `chipLabelMax - 1` and ends in an ellipsis.
    public static let chipLabelMax = 48

    /// The icon CONCEPT a kind's chip and card header draw (`packages/icons`
    /// `semantic`). A `list` chip draws its MEMBER kind's icon. The ExpUI side
    /// (`EntityChipIcon`) maps a concept to its `AppIcons` glyph.
    public static let icon: [String: String] = [
        "issue": "ui-issue",
        "board": "nav-boards",
        "action": "nav-actions",
        "automation": "nav-automations",
        "comment": "notification-issue-comment",
        "session": "coding-running",
        "label": "settings-labels",
        "status": "settings-statuses",
        "workflow": "nav-workflows",
        "device": "ui-device",
        "member": "ui-avatar-placeholder",
        "repository": "ui-repository",
        "team": "ui-team",
        "invite": "ui-invite",
        "notification": "nav-notifications",
        "thread": "nav-support",
        "attachment": "ui-attach",
        "list": "ui-checklist",
    ]

    private static let nouns: [String: (String, String)] = [
        "issue": ("issue", "issues"),
        "board": ("board", "boards"),
        "action": ("action", "actions"),
        "automation": ("automation", "automations"),
        "comment": ("comment", "comments"),
        "session": ("run", "runs"),
        "label": ("label", "labels"),
        "status": ("status", "statuses"),
        "workflow": ("workflow", "workflows"),
        "device": ("device", "devices"),
        "member": ("member", "members"),
        "repository": ("repository", "repositories"),
        "team": ("team", "teams"),
        "invite": ("invite", "invites"),
        "notification": ("notification", "notifications"),
        "thread": ("thread", "threads"),
        "attachment": ("attachment", "attachments"),
        "list": ("list", "lists"),
    ]

    /// The product noun for a kind: singular for `count` 1, plural otherwise.
    /// An unknown kind (a NEWER publisher) reads as `item`/`items`.
    public static func kindNoun(_ kind: String, count: Int = 1) -> String {
        let pair = nouns[kind] ?? ("item", "items")
        return count == 1 ? pair.0 : pair.1
    }

    /// The icon concept a ref draws: a `list` ref its member kind's, anything
    /// else its own; an unknown kind falls back to the `list` glyph.
    public static func refIcon(_ ref: EntityRef) -> String {
        let kind = ref.kind == "list" ? ref.id : ref.kind
        return icon[kind] ?? icon["list"]!
    }

    private static func capitalize(_ text: String) -> String {
        guard let first = text.first else { return text }
        return first.uppercased() + text.dropFirst()
    }

    /// Cut to `chipLabelMax` code points, ellipsis included in the budget;
    /// the kept prefix loses its trailing whitespace so no label ends in ` …`.
    public static func clampChipLabel(_ text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let points = Array(trimmed.unicodeScalars)
        if points.count <= chipLabelMax { return trimmed }
        var view = String.UnicodeScalarView()
        view.append(contentsOf: points.prefix(chipLabelMax - 1))
        let kept = String(view).trimmingTrailingWhitespace()
        return "\(kept)…"
    }

    /// The chip's text:
    /// - a `list`: `<count> <member noun>` ("3 issues", "1 run", "0 labels");
    /// - an issue: its identifier, else its title, else "Issue";
    /// - anything else: its title, else the capitalized noun ("Board", "Run").
    /// Whitespace-only titles count as absent.
    public static func chipLabel(_ ref: EntityRef) -> String {
        if ref.kind == "list" {
            let count = max(0, ref.count ?? 0)
            return clampChipLabel("\(count) \(kindNoun(ref.id, count: count))")
        }
        let identifier = trimmed(ref.identifier)
        let title = trimmed(ref.title)
        if ref.kind == "issue", let identifier { return clampChipLabel(identifier) }
        if let title { return clampChipLabel(title) }
        return capitalize(kindNoun(ref.kind, count: 1))
    }

    /// The secondary text an issue chip shows beside its identifier (the
    /// title), nil for every other kind or when the identifier already IS the
    /// label.
    public static func chipDetail(_ ref: EntityRef) -> String? {
        guard ref.kind == "issue",
              trimmed(ref.identifier) != nil,
              let title = trimmed(ref.title)
        else { return nil }
        return clampChipLabel(title)
    }

    /// One chip's worth of refs.
    public struct Group: Equatable, Sendable {
        public let ref: EntityRef
        /// The member refs a `list` chip's card lists; `[]` on every other chip.
        public var members: [EntityRef]

        public init(ref: EntityRef, members: [EntityRef] = []) {
            self.ref = ref
            self.members = members
        }
    }

    /// One chip per group: a `list` ref opens a group and absorbs every
    /// DIRECTLY following ref of its member kind; the first ref of another kind
    /// (or another list) closes it. Any other ref is a group of its own.
    public static func groupRefs(_ refs: [EntityRef]) -> [Group] {
        var groups: [Group] = []
        var openIndex: Int?
        for ref in refs {
            if let index = openIndex, ref.kind != "list", ref.kind == groups[index].ref.id {
                groups[index].members.append(ref)
                continue
            }
            groups.append(Group(ref: ref))
            openIndex = ref.kind == "list" ? groups.count - 1 : nil
        }
        return groups
    }

    private static func trimmed(_ text: String?) -> String? {
        guard let text else { return nil }
        let value = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}

private extension String {
    /// JavaScript's `trimEnd()`: trailing whitespace and line terminators.
    func trimmingTrailingWhitespace() -> String {
        var scalars = Array(unicodeScalars)
        while let last = scalars.last, CharacterSet.whitespacesAndNewlines.contains(last) {
            scalars.removeLast()
        }
        var view = String.UnicodeScalarView()
        view.append(contentsOf: scalars)
        return String(view)
    }
}
