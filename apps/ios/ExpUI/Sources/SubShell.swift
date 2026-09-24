import SwiftUI

// EXP-1029 contract, implemented by EXP-1020/EXP-1042 — sub-shell navigation
// for the settings shell (`GlassSection` rows, EXP-994). The device settings
// sheet's "Workflow settings" is its first caller.
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
// How it works here: the host owns a PAGE STACK (`SubShellNavigation`) and
// hands the rows below it one channel, the environment's `subShellOpen`. A
// tapped row pushes its page BUILDER — never a rendered snapshot — so the
// closure runs again on every render of the host and the page keeps seeing
// live bindings (the workflow pickers write straight back into the sheet's
// drafts). A page is drawn by the same host, which is what makes nesting
// fall out for free: a sub-shell inside a page pushes one level deeper and
// back returns ONE level, and only the TOP page is drawn — a deeper page
// hides its parent's rows and its parent's back header.
//
// SwiftUI has no portal, so the card's rows are not kept mounted the way
// web keeps them (there they sit hidden beside the page). Everything an open
// page reads therefore has to be a `Binding` (or another live value), which
// is what every settings row hands it anyway.

// MARK: - The page stack

/// One open page: what a level draws instead of its card. `content` is the
/// row's own page closure, re-invoked on every render.
public struct SubShellPage: Identifiable {
    public let id: String
    public let title: String
    let content: () -> AnyView

    public init<Content: View>(
        id: String,
        title: String,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.id = id
        self.title = title
        self.content = { AnyView(content()) }
    }
}

/// The host's page stack — the whole behaviour of a sub-shell that is not
/// drawing: what is open, what one level back is, and the disabled gate.
/// The views only render what it says, so this is where the rules are tested.
public struct SubShellNavigation {
    /// Outermost first; the LAST entry is the page on screen.
    public private(set) var stack: [SubShellPage] = []

    public init() {}

    public var isOpen: Bool { !stack.isEmpty }
    /// The page on screen, nil at rest (the card).
    public var current: SubShellPage? { stack.last }
    /// 0 = the card, 1 = a page, 2 = a page of a page.
    public var depth: Int { stack.count }

    /// Open `page` one level deeper. A DISABLED row never opens — the gate
    /// lives here so every platform's row can be a plain tap target.
    public mutating func push(_ page: SubShellPage, disabled: Bool = false) {
        guard !disabled else { return }
        stack.append(page)
    }

    /// Back goes ONE level: to the enclosing page, else to the card.
    public mutating func back() {
        guard !stack.isEmpty else { return }
        stack.removeLast()
    }

    /// Straight back to the card (the host going away, a sheet closing).
    public mutating func reset() {
        stack.removeAll()
    }
}

// MARK: - The channel a row opens through

/// Reference box around the open closure — the `GlassMenuDismissAction`
/// shape: a bare closure cannot be an `EnvironmentKey.defaultValue` under
/// strict concurrency, and what it captures is main-actor bound by
/// construction. The default one has no host above it, so it does nothing.
final class SubShellOpenAction: @unchecked Sendable {
    private let handler: ((SubShellPage, Bool) -> Void)?

    init(_ handler: ((SubShellPage, Bool) -> Void)? = nil) {
        self.handler = handler
    }

    func callAsFunction(_ page: SubShellPage, disabled: Bool) {
        handler?(page, disabled)
    }
}

private struct SubShellOpenKey: EnvironmentKey {
    static let defaultValue = SubShellOpenAction()
}

extension EnvironmentValues {
    /// Set by the enclosing `SubShellHost` — the ONE channel a row opens
    /// through.
    var subShellOpen: SubShellOpenAction {
        get { self[SubShellOpenKey.self] }
        set { self[SubShellOpenKey.self] = newValue }
    }
}

// MARK: - Host

/// The card boundary a sub-shell page replaces: it renders its card at rest
/// and the open row's page instead of it, sliding in from the trailing edge
/// (still under Reduce Motion — `\.motion` is nil then).
public struct SubShellHost<Content: View>: View {
    private let content: () -> Content

    @State private var navigation = SubShellNavigation()
    @Environment(\.motion) private var motion

    public init(@ViewBuilder content: @escaping () -> Content) {
        self.content = content
    }

    public var body: some View {
        Group {
            if let page = navigation.current {
                SubShellPageView(title: page.title, onBack: goBack) {
                    page.content()
                }
                // Each level slides in on its own: without the depth
                // identity SwiftUI would read a deeper push as the same
                // view and swap the contents with no transition.
                .id(navigation.depth)
                .transition(.move(edge: .trailing).combined(with: .opacity))
            } else {
                content()
                    .transition(.move(edge: .leading).combined(with: .opacity))
            }
        }
        .environment(\.subShellOpen, SubShellOpenAction(open))
        .accessibilityIdentifier("sub-shell-host")
    }

    private func open(_ page: SubShellPage, disabled: Bool) {
        withAnimation(motion.standard) {
            navigation.push(page, disabled: disabled)
        }
    }

    private func goBack() {
        withAnimation(motion.standard) {
            navigation.back()
        }
    }
}

/// A page: the back header, then the page's own shell of rows.
private struct SubShellPageView<Content: View>: View {
    let title: String
    let onBack: () -> Void
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 4) {
                GhostIconButton(
                    AppIcons.uiChevronLeft,
                    accessibilityLabel: "Back",
                    glyphSize: AppIcon.Size.medium
                ) {
                    onBack()
                }
                .accessibilityIdentifier("sub-shell-back")
                Text(title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            // Lines the back glyph up with the inset rows below it (the
            // ghost button pads its own glyph by ~6pt).
            .padding(.horizontal, 12)
            content()
        }
        .accessibilityIdentifier("sub-shell-page")
    }
}

// MARK: - Row

/// A row entry that slides its child page in place of the whole card.
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

    /// Stable for the life of the row, so a page can be identified across the
    /// host's renders without the label having to be unique.
    @State private var pageId = UUID().uuidString
    @Environment(\.subShellOpen) private var subShellOpen

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
        .contentShape(Rectangle())
        .onTapGesture { open() }
        .accessibilityIdentifier("sub-shell")
    }

    /// The page is pushed as a CLOSURE: the host re-invokes it on every
    /// render, so the bindings the caller handed it stay live.
    private func open() {
        subShellOpen(
            SubShellPage(id: pageId, title: title ?? label, content: page),
            disabled: disabled
        )
    }
}
