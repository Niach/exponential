import SwiftUI

// EXP-1029 contract — the issue picker (single or multi): rows read
// `IDENT Title`, search matches the identifier and the title. The composer
// picks several (a batch); relations, duplicates and the stack dialog pick
// one. Search over a big board runs the ONE engine (EXP-892) at the call
// site; the picker renders what it is handed.

public struct IssuePickerIssue: Identifiable, Hashable {
    public let id: String
    public let identifier: String
    public let title: String
    public let disabled: Bool
    /// The issue's STATUS glyph (an `AppIcons` name) — the leading mark the
    /// relations linker's rows have always drawn, and the look the rest of
    /// EXP-1021 is measured against. Additive on all four clients.
    public let icon: String?
    /// The glyph's colour as a hex, for a status row that carries one.
    public let colorHex: String?
    /// The glyph's colour when it is a design TOKEN rather than a hex — the
    /// builtin status palette (EXP-314). Wins over `colorHex` when set.
    public let color: Color?

    public init(
        id: String,
        identifier: String,
        title: String,
        disabled: Bool = false,
        icon: String? = nil,
        colorHex: String? = nil,
        color: Color? = nil
    ) {
        self.id = id
        self.identifier = identifier
        self.title = title
        self.disabled = disabled
        self.icon = icon
        self.colorHex = colorHex
        self.color = color
    }
}

public struct IssuePicker<Trigger: View>: View {
    public let issues: [IssuePickerIssue]
    public let mode: PickerMode
    public let value: Set<String>
    public let onChange: (Set<String>) -> Void
    /// EXP-1021 — controlled filter text. The issue picker is the one that
    /// needs it: over a big board the rows are ranked by the ONE engine
    /// (EXP-892) at the call site, so the primitive renders `issues` verbatim
    /// and only reports what was typed.
    public let query: Binding<String>?
    /// A muted "Loading…" row instead of the rows, while the caller's pool is
    /// still being read.
    public let loading: Bool
    /// EXP-1021 — the surface controls every typed picker forwards verbatim
    /// (web's `PickerSurfaceProps`): a host that opens the picker from its own
    /// property row or `…` menu drives `open` and hides the trigger, and
    /// `onDismiss` fires once the sheet finished animating away (what a
    /// hand-off to a SECOND picker is promoted on).
    public let open: Binding<Bool>?
    public let hideTrigger: Bool
    public let onDismiss: (() -> Void)?
    private let trigger: () -> Trigger

    public init(
        issues: [IssuePickerIssue],
        mode: PickerMode = .single,
        value: Set<String>,
        onChange: @escaping (Set<String>) -> Void,
        query: Binding<String>? = nil,
        loading: Bool = false,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.issues = issues
        self.mode = mode
        self.value = value
        self.onChange = onChange
        self.query = query
        self.loading = loading
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(_ issues: [IssuePickerIssue]) -> [PickerItem<String>] {
        issues.map { issue in
            PickerItem(
                value: issue.id,
                // ONE line, `IDENT Title` — the ×4 contract and the fixture;
                // only the leading glyph is per-issue.
                label: "\(issue.identifier) \(issue.title)",
                icon: issue.icon,
                color: issue.color ?? issue.colorHex.flatMap { Color(hex: $0) },
                disabled: issue.disabled,
                keywords: [issue.identifier, issue.title]
            )
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(issues),
            mode: mode,
            value: value,
            onChange: onChange,
            search: true,
            emptyText: "No issues",
            title: "Issues",
            query: query,
            loading: loading,
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            trigger: trigger
        )
    }
}
