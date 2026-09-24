import SwiftUI

// EXP-1029 contract — the assignee picker: the team's members by avatar +
// name, the email as the description and a search keyword; `Unassigned`
// first. The issue properties and the create-issue sheet pick one.

public struct AssigneePickerMember: Identifiable, Hashable {
    public let id: String
    public let name: String
    public let email: String?
    public let image: String?

    public init(id: String, name: String, email: String? = nil, image: String? = nil) {
        self.id = id
        self.name = name
        self.email = email
        self.image = image
    }
}

public struct AssigneePicker<Trigger: View>: View {
    /// The row that clears the pick (single mode); it reports an EMPTY set.
    nonisolated public static var unassignedValue: String { "" }

    public let members: [AssigneePickerMember]
    /// `.single` for the issue properties / create sheet (an empty `value` =
    /// unassigned), `.multi` for a filter (`allowsNone` ignored).
    public let mode: PickerMode
    public let value: Set<String>
    public let onChange: (Set<String>) -> Void
    /// Offer the `Unassigned` row (single mode only).
    public let allowsNone: Bool
    private let trigger: () -> Trigger

    public init(
        members: [AssigneePickerMember],
        mode: PickerMode = .single,
        value: Set<String>,
        onChange: @escaping (Set<String>) -> Void,
        allowsNone: Bool = true,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.members = members
        self.mode = mode
        self.value = value
        self.onChange = onChange
        self.allowsNone = allowsNone
        self.trigger = trigger
    }

    nonisolated public static func items(_ members: [AssigneePickerMember], allowsNone: Bool) -> [PickerItem<String>] {
        let rows = members.map { member in
            PickerItem(
                value: member.id,
                label: member.name,
                description: member.email,
                keywords: [member.name, member.email ?? ""].filter { !$0.isEmpty }
            )
        }
        return allowsNone ? [PickerItem(value: unassignedValue, label: "Unassigned")] + rows : rows
    }

    private var single: Bool { mode == .single }

    public var body: some View {
        GlassPicker(
            items: Self.items(members, allowsNone: single && allowsNone),
            mode: mode,
            value: single && value.isEmpty && allowsNone ? [Self.unassignedValue] : value,
            onChange: { picked in onChange(picked.subtracting([Self.unassignedValue])) },
            search: true,
            emptyText: "No members",
            title: "Assignee",
            trigger: trigger
        )
    }
}
