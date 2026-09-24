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
    /// The row that clears the pick.
    public static var unassignedValue: String { "" }

    public let members: [AssigneePickerMember]
    public let value: String?
    public let onChange: (String?) -> Void
    /// Offer the `Unassigned` row.
    public let allowsNone: Bool
    private let trigger: () -> Trigger

    public init(
        members: [AssigneePickerMember],
        value: String?,
        onChange: @escaping (String?) -> Void,
        allowsNone: Bool = true,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.members = members
        self.value = value
        self.onChange = onChange
        self.allowsNone = allowsNone
        self.trigger = trigger
    }

    public static func items(_ members: [AssigneePickerMember], allowsNone: Bool) -> [PickerItem<String>] {
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

    public var body: some View {
        GlassPicker(
            items: Self.items(members, allowsNone: allowsNone),
            mode: .single,
            value: [value ?? (allowsNone ? Self.unassignedValue : nil)].compactMap { $0 }.reduce(into: Set<String>()) { $0.insert($1) },
            onChange: { picked in
                guard let first = picked.first else { return }
                onChange(first == Self.unassignedValue ? nil : first)
            },
            search: true,
            emptyText: "No members",
            title: "Assignee",
            trigger: trigger
        )
    }
}
