import SwiftUI

// The UI cleanup batch — THE repository picker on iOS (web
// `packages/ui/src/picker/repository-picker.tsx`: `RepositoryPicker` +
// `RepositoryPickerList`; Android `RepositoryPicker`). It retired the three
// hand-rolled repo sheets: the board form's repository select, the readiness
// checklist's "Choose repository" and the Add-repository list off GitHub, and
// adds `BranchPicker` for the board's branch.
//
// One ROW everywhere: the GitHub glyph, `owner/name` in mono, a private lock,
// an optional trailing tag ("matches board" in green, "used by Website"
// muted), on the picker's own row geometry (`GlassPickerTokens`).
//
// Two arms, exactly as on web:
//   - `RepositoryPicker`: a typed `GlassPicker` (trigger + the ONE sheet),
//     for a pick that IS a value (the board form).
//   - `RepositoryPickerList`: the bare rows, for a sheet that carries more
//     than the list (the readiness checklist's search + Add from GitHub, the
//     GitHub listing's prerequisites and footer). The host owns the search,
//     the footer, the empty copy and what a pick does.

/// One repository row.
public struct RepositoryPickerRow: Identifiable, Hashable, Sendable {
    /// The pick value (a registry id, or the full name for a live GitHub row).
    public let id: String
    public let fullName: String
    public let isPrivate: Bool
    /// A trailing note; `emphasis` paints it green ("matches board").
    public let tag: String?
    public let emphasis: Bool

    public init(id: String, fullName: String, isPrivate: Bool = false, tag: String? = nil, emphasis: Bool = false) {
        self.id = id
        self.fullName = fullName
        self.isPrivate = isPrivate
        self.tag = tag
        self.emphasis = emphasis
    }
}

public enum RepositoryPickerRules {
    /// The picker's "none" row value ("No repository").
    public static let noneValue = "__none"
    /// The filter field turns on once the list is long (web: `> 7`).
    public static let searchThreshold = 7

    public static func showsSearch(count: Int) -> Bool { count > searchThreshold }

    /// The rows as picker items: the GitHub glyph as the MARK, the full name
    /// as label + search keyword; a `noneLabel` leads as a plain row.
    public static func items(_ rows: [RepositoryPickerRow], noneLabel: String? = nil) -> [PickerItem<String>] {
        let none = noneLabel.map { [PickerItem(value: noneValue, label: $0)] } ?? []
        return none + rows.map { row in
            PickerItem(value: row.id, label: row.fullName, icon: AppIcons.uiGithub, keywords: [row.fullName])
        }
    }

    /// The bare list's filter: case-insensitive `contains` on the full name,
    /// the picker's own fold (`PickerSearch`).
    public static func filter(_ rows: [RepositoryPickerRow], query: String) -> [RepositoryPickerRow] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return rows }
        return rows.filter { $0.fullName.localizedCaseInsensitiveContains(trimmed) }
    }
}

/// Everything right of the row's mark: `owner/name`, the lock, the tag — or a
/// spinner while the host is acting on this row.
public struct RepositoryPickerRowBody: View {
    let row: RepositoryPickerRow
    var busy: Bool = false

    public init(row: RepositoryPickerRow, busy: Bool = false) {
        self.row = row
        self.busy = busy
    }

    public var body: some View {
        HStack(spacing: 8) {
            Text(row.fullName)
                .font(.subheadline.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.primary))
                .lineLimit(1)
                .truncationMode(.middle)
            if row.isPrivate {
                AppIcon(AppIcons.uiPrivate, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityLabel("Private")
            }
            Spacer(minLength: 8)
            if busy {
                ProgressView().controlSize(.small).tint(.white)
            } else if let tag = row.tag {
                Text(tag)
                    .font(.caption)
                    .foregroundStyle(row.emphasis ? DesignTokens.Semantic.green : .white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
            }
        }
    }
}

/// THE repository picker: one pick out of the team's repositories.
public struct RepositoryPicker<Trigger: View>: View {
    public let repositories: [RepositoryPickerRow]
    /// The picked row's id; nil = none (the `noneLabel` row, when there is one).
    public let value: String?
    /// The picked id, or nil for the `noneLabel` row.
    public let onChange: (String?) -> Void
    public let title: String
    /// A first row that clears the pick ("No repository").
    public let noneLabel: String?
    public let emptyText: String?
    public let disabled: Bool
    /// Under the rows ("Connect another repository…").
    public let footer: (() -> AnyView)?
    public let open: Binding<Bool>?
    public let hideTrigger: Bool
    public let onDismiss: (() -> Void)?
    private let trigger: () -> Trigger

    public init(
        repositories: [RepositoryPickerRow],
        value: String?,
        onChange: @escaping (String?) -> Void,
        title: String = "Repository",
        noneLabel: String? = nil,
        emptyText: String? = "No repositories",
        disabled: Bool = false,
        footer: (() -> AnyView)? = nil,
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.repositories = repositories
        self.value = value
        self.onChange = onChange
        self.title = title
        self.noneLabel = noneLabel
        self.emptyText = emptyText
        self.disabled = disabled
        self.footer = footer
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    public var body: some View {
        let byId = Dictionary(repositories.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        GlassPicker(
            items: RepositoryPickerRules.items(repositories, noneLabel: noneLabel),
            mode: .single,
            value: [value ?? (noneLabel == nil ? "" : RepositoryPickerRules.noneValue)],
            onChange: { picked in
                guard let id = picked.first else { return }
                onChange(id == RepositoryPickerRules.noneValue ? nil : id)
            },
            search: RepositoryPickerRules.showsSearch(count: repositories.count),
            emptyText: emptyText,
            title: title,
            disabled: disabled,
            searchPlaceholder: "Search repositories…",
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            footer: footer,
            footerReplacesEmpty: false,
            renderItem: { item in
                byId[item.value].map { AnyView(RepositoryPickerRowBody(row: $0)) }
            },
            sheetIdentifier: "repository-picker",
            trigger: trigger
        )
    }
}

/// The bare rows (web `RepositoryPickerList`), on the picker's row geometry,
/// for a sheet that carries more than the list.
public struct RepositoryPickerList: View {
    let rows: [RepositoryPickerRow]
    let onPick: (RepositoryPickerRow) -> Void
    /// The row the host is acting on: it spins, every row waits.
    var busyId: String?
    var disabled: Bool
    /// Drawn when `rows` is empty; nil = nothing.
    var emptyText: String?
    /// A per-row accessibility identifier (flows pinned to a repo row).
    var rowIdentifier: ((RepositoryPickerRow) -> String)?

    public init(
        rows: [RepositoryPickerRow],
        busyId: String? = nil,
        disabled: Bool = false,
        emptyText: String? = nil,
        rowIdentifier: ((RepositoryPickerRow) -> String)? = nil,
        onPick: @escaping (RepositoryPickerRow) -> Void
    ) {
        self.rows = rows
        self.busyId = busyId
        self.disabled = disabled
        self.emptyText = emptyText
        self.rowIdentifier = rowIdentifier
        self.onPick = onPick
    }

    public var body: some View {
        VStack(spacing: GlassPickerTokens.rowSpacing) {
            if rows.isEmpty {
                if let emptyText {
                    Text(emptyText)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 16)
                }
            } else {
                ForEach(rows) { row in
                    Button {
                        onPick(row)
                    } label: {
                        HStack(spacing: 10) {
                            AppIcon(AppIcons.uiGithub, size: AppIcon.Size.medium)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .frame(width: GlassPickerTokens.markWidth)
                            RepositoryPickerRowBody(row: row, busy: busyId == row.id)
                        }
                        .padding(.horizontal, GlassPickerTokens.rowHPadding)
                        .frame(minHeight: GlassPickerTokens.rowMinHeight)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(disabled || busyId != nil)
                    .modifier(OptionalIdentifier(identifier: rowIdentifier?(row)))
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("repository-picker")
    }
}

private struct OptionalIdentifier: ViewModifier {
    let identifier: String?

    func body(content: Content) -> some View {
        if let identifier {
            content.accessibilityIdentifier(identifier)
        } else {
            content
        }
    }
}

// MARK: - Branch

/// The board's branch (web `BranchPicker`): the repo's branches, the
/// effective one always offered even when GitHub dropped it (a pin deleted
/// upstream), the repo default tagged "default". Picking the default reports
/// nil — follow the repo again. `branches == nil` = still loading.
public enum BranchPickerRules {
    public static let defaultTag = "default"

    /// The rows: the effective branch first when GitHub no longer lists it.
    public static func names(branches: [String], value: String) -> [String] {
        branches.contains(value) ? branches : [value] + branches
    }

    /// What a pick reports: nil for the repo default, else the branch.
    public static func pin(_ picked: String, repoDefault: String) -> String? {
        picked == repoDefault ? nil : picked
    }
}

public struct BranchPicker<Trigger: View>: View {
    public let branches: [String]?
    public let value: String
    public let repoDefault: String
    public let onChange: (String?) -> Void
    /// A failed listing: shown under the (empty) rows with a Retry.
    public let errorText: String?
    public let onRetry: (() -> Void)?
    public let disabled: Bool
    public let open: Binding<Bool>?
    private let trigger: () -> Trigger

    public init(
        branches: [String]?,
        value: String,
        repoDefault: String,
        onChange: @escaping (String?) -> Void,
        errorText: String? = nil,
        onRetry: (() -> Void)? = nil,
        disabled: Bool = false,
        open: Binding<Bool>? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.branches = branches
        self.value = value
        self.repoDefault = repoDefault
        self.onChange = onChange
        self.errorText = errorText
        self.onRetry = onRetry
        self.disabled = disabled
        self.open = open
        self.trigger = trigger
    }

    public var body: some View {
        let names = branches.map { BranchPickerRules.names(branches: $0, value: value) } ?? []
        GlassPicker(
            items: names.map { name in
                PickerItem(
                    value: name,
                    label: name,
                    icon: AppIcons.uiBranch,
                    description: name == repoDefault ? BranchPickerRules.defaultTag : nil
                )
            },
            mode: .single,
            value: [value],
            onChange: { picked in
                guard let name = picked.first, name != value else { return }
                onChange(BranchPickerRules.pin(name, repoDefault: repoDefault))
            },
            search: true,
            emptyText: errorText == nil ? "No branches found." : nil,
            title: "Branch",
            disabled: disabled,
            searchPlaceholder: "Search branches…",
            loading: branches == nil && errorText == nil,
            open: open,
            footer: errorText.map { message in
                {
                    AnyView(
                        VStack(alignment: .leading, spacing: 8) {
                            Text(message).font(.caption).foregroundStyle(.red.opacity(0.8))
                            if let onRetry {
                                GlassPill("Retry", icon: AppIcons.uiRefresh, mode: .action(onRetry))
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, GlassPickerTokens.rowHPadding)
                        .padding(.vertical, 8)
                    )
                }
            },
            renderItem: { item in
                AnyView(
                    HStack(spacing: 8) {
                        Text(item.label)
                            .font(.subheadline.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.primary))
                            .lineLimit(1)
                            .truncationMode(.middle)
                        if let tag = item.description {
                            Text(tag)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                )
            },
            sheetIdentifier: "branch-picker",
            trigger: trigger
        )
    }
}
