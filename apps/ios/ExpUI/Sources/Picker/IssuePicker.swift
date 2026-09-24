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

    public init(id: String, identifier: String, title: String, disabled: Bool = false) {
        self.id = id
        self.identifier = identifier
        self.title = title
        self.disabled = disabled
    }
}

public struct IssuePicker<Trigger: View>: View {
    public let issues: [IssuePickerIssue]
    public let mode: PickerMode
    public let value: Set<String>
    public let onChange: (Set<String>) -> Void
    private let trigger: () -> Trigger

    public init(
        issues: [IssuePickerIssue],
        mode: PickerMode = .single,
        value: Set<String>,
        onChange: @escaping (Set<String>) -> Void,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.issues = issues
        self.mode = mode
        self.value = value
        self.onChange = onChange
        self.trigger = trigger
    }

    public static func items(_ issues: [IssuePickerIssue]) -> [PickerItem<String>] {
        issues.map { issue in
            PickerItem(
                value: issue.id,
                label: "\(issue.identifier) \(issue.title)",
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
            trigger: trigger
        )
    }
}
