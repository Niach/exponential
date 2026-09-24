import SwiftUI

// EXP-1029 contract — sub-shell navigation for the settings shell
// (`GlassSection` rows, EXP-994). EXP-1020 implements it and uses it for
// "Workflow settings" inside the device settings sheet.
//
// A `SubShell` is a ROW ENTRY inside a card. Opening it slides a child page
// in place of the WHOLE card — not a nested card, not a pushed screen —
// with a back button on top; the child page is the same shell (its own
// `GlassSection`s of rows), so a sub-shell may hold another sub-shell.
// `SubShellHost` is the card boundary the page replaces: it renders its
// card at rest and, once a row inside opened, that row's page.
//
// Siblings (same names): web `packages/ui/src/sub-shell.tsx`, IDE
// `ui::sub_shell`, Android `ui/components/SubShell.kt`.
//
// This file is the CONTRACT: the row stub (never opens) and a host that only
// ever renders its card. `ExpUI/Tests/SubShellContractTests.swift` carries
// the behaviour, skipped until EXP-1020.

/// The card boundary a sub-shell page replaces. Contract stub: the card.
public struct SubShellHost<Content: View>: View {
    private let content: () -> Content

    public init(@ViewBuilder content: @escaping () -> Content) {
        self.content = content
    }

    public var body: some View {
        content()
            .accessibilityIdentifier("sub-shell-host")
    }
}

/// A row entry that slides its child page in place of the whole card.
/// Contract stub: the row (label, description, value, chevron); opening
/// does nothing until EXP-1020.
public struct SubShell<Page: View>: View {
    public let label: String
    /// A muted second line under the label.
    public let description: String?
    /// A leading `AppIcons` name.
    public let icon: String?
    /// A muted trailing summary (`opus · fable`).
    public let value: String?
    /// The child page's title; defaults to `label`.
    public let title: String?
    public let disabled: Bool
    private let page: () -> Page

    public init(
        label: String,
        description: String? = nil,
        icon: String? = nil,
        value: String? = nil,
        title: String? = nil,
        disabled: Bool = false,
        @ViewBuilder page: @escaping () -> Page
    ) {
        self.label = label
        self.description = description
        self.icon = icon
        self.value = value
        self.title = title
        self.disabled = disabled
        self.page = page
    }

    public var body: some View {
        HStack(spacing: 12) {
            if let icon {
                AppIcon(icon)
                    .foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(label)
                if let description {
                    Text(description)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
            if let value {
                Text(value)
                    .foregroundStyle(.secondary)
            }
            AppIcon(AppIcons.uiChevronRight)
                .foregroundStyle(.secondary)
        }
        .opacity(disabled ? 0.5 : 1)
        .accessibilityIdentifier("sub-shell")
    }
}
