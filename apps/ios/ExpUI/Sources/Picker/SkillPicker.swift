import ExpCore
import SwiftUI

// #EXP-1661 (skills.json `menuRow` / `agentRow`): THE multi picker over the
// team's skills, sorted by name, each row the `ui-docs` concept + the name,
// the description as its caption. The composer's "+" Skills row and the
// agent page's Skills row open it (McpServerPicker's twin).
public struct SkillPicker<Trigger: View>: View {
    public let skills: [SkillEntity]
    public let value: [String]
    public let onChange: ([String]) -> Void
    public let title: String
    public let disabled: Bool
    public let open: Binding<Bool>?
    public let hideTrigger: Bool
    public let onDismiss: (() -> Void)?
    private let trigger: () -> Trigger

    /// More rows than this get the search field.
    public static var searchThreshold: Int { 6 }

    public init(
        skills: [SkillEntity],
        value: [String],
        onChange: @escaping ([String]) -> Void,
        title: String = Skills.title,
        disabled: Bool = false,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.skills = skills
        self.value = value
        self.onChange = onChange
        self.title = title
        self.disabled = disabled
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(_ skills: [SkillEntity]) -> [PickerItem<String>] {
        Skills.sortSkills(skills).map { skill in
            PickerItem(
                value: skill.id,
                label: skill.name,
                icon: AppIcons.uiDocs,
                description: skill.description.isEmpty ? nil : skill.description
            )
        }
    }

    public var body: some View {
        let sorted = Skills.sortSkills(skills)
        GlassPicker(
            items: Self.items(sorted),
            mode: .multi,
            value: Set(value),
            onChange: { picked in
                // List order, not the set's (preselect's order too).
                onChange(sorted.map(\.id).filter { picked.contains($0) })
            },
            search: skills.count > Self.searchThreshold,
            emptyText: Skills.pickerEmpty,
            title: title,
            disabled: disabled,
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            sheetIdentifier: "skill-picker",
            trigger: trigger
        )
        .pickerCheckMarks()
    }
}
