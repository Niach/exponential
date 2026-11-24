import Foundation

/// #EXP-1950 (fixtures/install-trust.json `copy`, `icon`; decisions a1 + b1):
/// the approval of a new install and a team-mate's reset, and this install
/// waiting itself, as the genUI trees web builds
/// (`apps/web/src/components/devices/install-approval.tsx`): the same node
/// ids, props and order, painted by the core catalog (`CiSurface`): Section,
/// Row, Badge, Button, EmptyState. No new component, no monospace style: the
/// code is the Row's subtitle / the EmptyState's description. Rows: Needs
/// you, and the phone's Devices too. The code is derived on THIS install
/// from the row's endpoint id (`InstallCode`), never read from the server.
public enum InstallApprovalView {
    /// The fixture's `copy`, the same ×4.
    public enum Copy {
        public static let newInstall = "New install"
        public static let reset = "Reset"
        public static let approve = "Approve"
        public static let remove = "Remove"
        public static let waiting = "Waiting for approval"

        public static func badge(_ kind: TrustRow.Kind) -> String {
            switch kind {
            case .newInstall: newInstall
            case .reset: reset
            }
        }
    }

    /// The fixture's `icon` (the waiting EmptyState's glyph).
    public static let icon = "ui-private"

    /// Events the rows fire (context `id` = the row's install id).
    public enum Events {
        public static let approve = "approve"
        public static let remove = "remove"
    }

    public enum TestIds {
        public static let pending = "pending-installs"
        public static let waiting = "install-waiting"
    }

    /// A row's title: the install's label; a team-mate's reset leads with
    /// their name. Nothing = the code.
    public static func title(_ row: TrustRow, memberName: String?) -> String {
        let label = row.label ?? ""
        guard row.kind == .reset else { return label.isEmpty ? row.code : label }
        let joined = [memberName ?? "", label].filter { !$0.isEmpty }.joined(separator: " · ")
        return joined.isEmpty ? row.code : joined
    }

    private static func node(
        _ id: String, _ component: String, props: [String: CardJSON], children: [CardJSON]? = nil, extra: [String: CardJSON] = [:]
    ) -> CardJSON {
        var out: [String: CardJSON] = ["id": .string(id), "component": .string(component), "props": .object(props)]
        for (key, value) in extra { out[key] = value }
        if let children { out["children"] = .array(children) }
        return .object(out)
    }

    private static func press(_ name: String, _ installId: String) -> [String: CardJSON] {
        ["on": .object(["press": .object(["event": .object(["name": .string(name), "context": .object(["id": .string(installId)])])])])]
    }

    /// One Row per install: title, the code as its subtitle; trailing the
    /// Badge (secondary), Approve (Button sm) and Remove (Button sm ghost).
    public static func row(_ row: TrustRow, title: String) -> CardJSON {
        let id = "install-\(row.installId)"
        let actions = node("\(id)-actions", "Stack", props: ["direction": .string("horizontal"), "gap": .string("sm")], children: [
            node("\(id)-badge", "Badge", props: ["text": .string(Copy.badge(row.kind)), "variant": .string("secondary")]),
            node("\(id)-approve", "Button", props: ["label": .string(Copy.approve), "size": .string("sm")], extra: press(Events.approve, row.installId)),
            node("\(id)-remove", "Button", props: ["label": .string(Copy.remove), "size": .string("sm"), "variant": .string("ghost")], extra: press(Events.remove, row.installId)),
        ])
        return node(id, "Row", props: ["title": .string(title), "subtitle": .string(row.code)], extra: ["slots": .object(["trailing": actions])])
    }

    /// The band of installs waiting for this install's approval.
    public static func pendingTree(_ rows: [(row: TrustRow, title: String)]) -> CardJSON {
        node(TestIds.pending, "Section", props: [:], children: rows.map { row($0.row, title: $0.title) })
    }

    /// This install, not approved yet: THE page EmptyState with its own code.
    public static func waitingTree(code: String) -> CardJSON {
        node(TestIds.waiting, "EmptyState", props: ["icon": .string(icon), "title": .string(Copy.waiting), "description": .string(code)])
    }
}

/// The capture specimens' rows (`install-approval`, web `?specimen=`):
/// install-trust.json `keys.N` (a New install) and `keys.T3` (a team-mate's
/// Reset); InstallTrustFixtureTests holds them to the file.
public enum InstallTrustSpecimenRows {
    public static let rows = [
        TrustRow(
            kind: .newInstall, installId: "install-N",
            endpointId: "3f7708d5f5cc2bc633b59d2b3a2ed92e7479220c6f08ade208bebcd8580ab93b",
            userId: "user-me", label: "Pixel 9", code: "HNMBN A0319"
        ),
        TrustRow(
            kind: .reset, installId: "install-T3",
            endpointId: "74fca2a3b389fb1a64d9bf52cc0dd4c2964f3804c0cf7c755e8513c6db8198dc",
            userId: "user-mate", label: "Mira's new iPhone", code: "776AF T3NNF"
        ),
    ]
}
