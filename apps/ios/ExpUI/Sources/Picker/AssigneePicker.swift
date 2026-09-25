import ExpCore
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
        members: [AssigneePickerMember],
        mode: PickerMode = .single,
        value: Set<String>,
        onChange: @escaping (Set<String>) -> Void,
        allowsNone: Bool = true,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.members = members
        self.mode = mode
        self.value = value
        self.onChange = onChange
        self.allowsNone = allowsNone
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
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
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            // The avatar is what makes a member row a MEMBER row, and it is
            // not a `PickerItem` slot (a picker glyph is an icon, never a
            // photo) — so the one picker that needs one draws it over the
            // primitive's mark. `Unassigned` keeps its own glyph: there is
            // nobody to picture.
            renderMark: { item in
                if item.value == Self.unassignedValue {
                    return AnyView(
                        AppIcon(AppIcons.uiUnassigned, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    )
                }
                guard let member = members.first(where: { $0.id == item.value }) else { return nil }
                return AnyView(
                    UserAvatar(
                        image: member.image,
                        initials: memberInitials(
                            forDisplayName: member.name.isEmpty ? (member.email ?? "") : member.name
                        ),
                        hueKey: member.id,
                        size: 22
                    )
                )
            },
            trigger: trigger
        )
    }
}
