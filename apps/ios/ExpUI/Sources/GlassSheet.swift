import SwiftUI

// The ONE bottom-sheet shell (EXP-687) — the iOS twin of Android's
// ui/components/GlassSheet.kt. Every `.sheet` in the app roots in
// `GlassSheetChrome`, so a sheet cannot drift: system drag indicator, opaque
// `glass.backgroundBottom` (#18181B), 24pt top radius, a left-aligned title,
// and — when there is a primary action — ONE full-width `GlassSubmitButton`
// pinned to the bottom. There is no ✕ and no Cancel anywhere: a swipe down
// dismisses, on both phones.
//
// Moved here from Exponential/UI/Issue/Sheets/GlassSheetChrome.swift by
// EXP-603, which retired the last stock `PickerSheet` call sites: a sheet is
// now the app's ONE answer to "pick one of these", so it belongs in ExpUI next
// to the glass controls (EXP-604) rather than under the issue feature folder.
//
// Absent twins, deliberately: Android's `GlassSheet` takes an `onDismiss`
// because a Compose bottom sheet is state-driven; here the caller's
// `.sheet(...)` owns presentation and the shell only paints.

/// How tall a sheet opens. `.fitted` measures its content and sizes to exactly
/// that (capped at `GlassSheetTokens.fittedMaxFraction` of the screen, inner
/// scroll past it); `.full` opens at `.large` and lets the caller's own
/// Form/List scroll.
public enum GlassSheetHeight {
    case fitted
    case full
}

/// The cross-platform sheet constants, pinned by `GlassSheetTokenTests` and
/// mirrored by Android's `GlassSheetDefaults` (`GlassSheetDefaultsTest.kt`).
public enum GlassSheetTokens {
    /// 24pt top corners — the `xl3` rung, matching web's `rounded-t-3xl`.
    public static let cornerRadius: CGFloat = 24
    /// A fitted sheet never claims more than 85 % of the screen.
    public static let fittedMaxFraction: CGFloat = 0.85
    /// Clears the system drag indicator, which paints in the top ~14pt.
    public static let headerTopPadding: CGFloat = 22
    public static let headerBottomPadding: CGFloat = 10
    public static let headerHPadding: CGFloat = 20
    public static let actionHPadding: CGFloat = 16
    public static let actionVPadding: CGFloat = 10
    /// Opaque #18181B — no blur. A sheet is a surface, not a scrim.
    public static var background: Color { DesignTokens.Glass.backgroundBottom }
}

/// The sheet shell: optional title header (+ a trailing slot for "Clear all" /
/// "+12 −4"), an optional pinned header (a search field that must not scroll
/// away), the caller's content, and an optional bottom-pinned primary action.
public struct GlassSheetChrome<Header: View, Pinned: View, Content: View, Action: View>: View {
    let title: String?
    var height: GlassSheetHeight = .fitted
    @ViewBuilder let headerTrailing: () -> Header
    @ViewBuilder let pinnedHeader: () -> Pinned
    @ViewBuilder let content: () -> Content
    @ViewBuilder let primaryAction: () -> Action

    // The pieces of a fitted sheet's natural height. Each slot measures itself
    // AFTER its padding, so the sum is the detent.
    @State private var headerHeight: CGFloat = 0
    @State private var pinnedHeight: CGFloat = 0
    @State private var contentHeight: CGFloat = 0
    @State private var actionHeight: CGFloat = 0
    // The sheet's bottom safe-area inset (home indicator) — part of a
    // `.height` detent, so it must be added to the measured content. Values
    // from a raised keyboard are ignored (they'd inflate the detent).
    @State private var bottomInset: CGFloat = 0

    public init(
        title: String? = nil,
        height: GlassSheetHeight = .fitted,
        @ViewBuilder headerTrailing: @escaping () -> Header,
        @ViewBuilder pinnedHeader: @escaping () -> Pinned,
        @ViewBuilder content: @escaping () -> Content,
        @ViewBuilder primaryAction: @escaping () -> Action
    ) {
        self.title = title
        self.height = height
        self.headerTrailing = headerTrailing
        self.pinnedHeader = pinnedHeader
        self.content = content
        self.primaryAction = primaryAction
    }

    private var hasAction: Bool { Action.self != EmptyView.self }

    private var resolvedDetents: Set<PresentationDetent> {
        guard height == .fitted else { return [.large] }
        guard contentHeight > 0 else { return [.medium] }
        let natural = headerHeight + pinnedHeight + contentHeight + actionHeight + bottomInset
        let cap = UIScreen.main.bounds.height * GlassSheetTokens.fittedMaxFraction
        return [.height(min(natural, cap))]
    }

    public var body: some View {
        VStack(spacing: 0) {
            if let title {
                HStack(spacing: 12) {
                    Text(title)
                        .font(.headline)
                        .foregroundStyle(.white)
                    Spacer(minLength: 0)
                    headerTrailing()
                }
                .padding(.horizontal, GlassSheetTokens.headerHPadding)
                .padding(.top, GlassSheetTokens.headerTopPadding)
                .padding(.bottom, GlassSheetTokens.headerBottomPadding)
                .measuredHeight { headerHeight = $0 }
            }

            pinnedHeader()
                .measuredHeight { pinnedHeight = $0 }

            switch height {
            case .fitted:
                ScrollView {
                    content()
                        .measuredHeight { contentHeight = $0 }
                }
                .scrollDismissesKeyboard(.interactively)
            case .full:
                content()
                    .frame(maxHeight: .infinity)
            }

            if hasAction {
                primaryAction()
                    .padding(.horizontal, GlassSheetTokens.actionHPadding)
                    .padding(.vertical, GlassSheetTokens.actionVPadding)
                    // Opaque: the action is a floor, not a translucent strip
                    // over the last scrolled row. A `.height` detent excludes
                    // safe-area insets, so this is a VStack slot rather than a
                    // `safeAreaInset`.
                    .background(GlassSheetTokens.background)
                    .measuredHeight { actionHeight = $0 }
            }
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .onGeometryChange(for: CGFloat.self, of: { $0.safeAreaInsets.bottom }) { inset in
            if inset < 60 { bottomInset = inset }
        }
        .presentationDetents(resolvedDetents)
        .presentationBackground(GlassSheetTokens.background)
        .presentationCornerRadius(GlassSheetTokens.cornerRadius)
        .presentationDragIndicator(.visible)
        .presentationContentInteraction(.scrolls)
    }
}

extension View {
    /// Report this view's laid-out height once it changes — the fitted sheet's
    /// measurement, factored out so every slot measures identically.
    fileprivate func measuredHeight(_ onChange: @escaping (CGFloat) -> Void) -> some View {
        onGeometryChange(for: CGFloat.self, of: { $0.size.height }, action: onChange)
    }
}

// MARK: - Convenience inits

extension GlassSheetChrome where Header == EmptyView, Pinned == EmptyView, Action == EmptyView {
    public init(
        title: String? = nil,
        height: GlassSheetHeight = .fitted,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.init(
            title: title,
            height: height,
            headerTrailing: { EmptyView() },
            pinnedHeader: { EmptyView() },
            content: content,
            primaryAction: { EmptyView() }
        )
    }
}

extension GlassSheetChrome where Header == EmptyView, Pinned == EmptyView {
    public init(
        title: String? = nil,
        height: GlassSheetHeight = .fitted,
        @ViewBuilder content: @escaping () -> Content,
        @ViewBuilder primaryAction: @escaping () -> Action
    ) {
        self.init(
            title: title,
            height: height,
            headerTrailing: { EmptyView() },
            pinnedHeader: { EmptyView() },
            content: content,
            primaryAction: primaryAction
        )
    }
}

extension GlassSheetChrome where Header == EmptyView, Action == EmptyView {
    public init(
        title: String? = nil,
        height: GlassSheetHeight = .fitted,
        @ViewBuilder pinnedHeader: @escaping () -> Pinned,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.init(
            title: title,
            height: height,
            headerTrailing: { EmptyView() },
            pinnedHeader: pinnedHeader,
            content: content,
            primaryAction: { EmptyView() }
        )
    }
}

extension GlassSheetChrome where Header == EmptyView {
    public init(
        title: String? = nil,
        height: GlassSheetHeight = .fitted,
        @ViewBuilder pinnedHeader: @escaping () -> Pinned,
        @ViewBuilder content: @escaping () -> Content,
        @ViewBuilder primaryAction: @escaping () -> Action
    ) {
        self.init(
            title: title,
            height: height,
            headerTrailing: { EmptyView() },
            pinnedHeader: pinnedHeader,
            content: content,
            primaryAction: primaryAction
        )
    }
}

extension GlassSheetChrome where Pinned == EmptyView, Action == EmptyView {
    public init(
        title: String? = nil,
        height: GlassSheetHeight = .fitted,
        @ViewBuilder headerTrailing: @escaping () -> Header,
        @ViewBuilder content: @escaping () -> Content
    ) {
        self.init(
            title: title,
            height: height,
            headerTrailing: headerTrailing,
            pinnedHeader: { EmptyView() },
            content: content,
            primaryAction: { EmptyView() }
        )
    }
}

extension GlassSheetChrome where Pinned == EmptyView {
    public init(
        title: String? = nil,
        height: GlassSheetHeight = .fitted,
        @ViewBuilder headerTrailing: @escaping () -> Header,
        @ViewBuilder content: @escaping () -> Content,
        @ViewBuilder primaryAction: @escaping () -> Action
    ) {
        self.init(
            title: title,
            height: height,
            headerTrailing: headerTrailing,
            pinnedHeader: { EmptyView() },
            content: content,
            primaryAction: primaryAction
        )
    }
}

/// The untyped "pick one of these" sheet (EXP-603 retired the stock
/// `PickerSheet` it was cloned from): immediate commit + dismiss on tap. The
/// chrome measures the rows, so a three-option list presents as a short sheet
/// instead of a half-screen of empty glass.
///
/// EXP-1021: the SHARED picker (`GlassPicker` + its ten typed pickers) is what
/// a board / issue / action / account / device / assignee / icon / status /
/// priority / label pick renders through. This one stays for the picks that
/// are not one of those ten — a repository, a branch, an estimate scale, an
/// "Any board" event filter — and it draws its rows in the picker's OWN
/// language so the two cannot look like different products: plain rows on the
/// sheet, and the pick marked by the row's highlight, never a trailing
/// checkmark.
public struct GlassPickerSheet<Item, ID: Hashable, Row: View>: View {
    let title: String
    let items: [Item]
    let selectedID: ID?
    let idFor: (Item) -> ID
    let onSelect: (Item) -> Void
    @ViewBuilder let row: (Item) -> Row

    @Environment(\.dismiss) private var dismiss

    public init(
        title: String,
        items: [Item],
        selectedID: ID?,
        idFor: @escaping (Item) -> ID,
        onSelect: @escaping (Item) -> Void,
        @ViewBuilder row: @escaping (Item) -> Row
    ) {
        self.title = title
        self.items = items
        self.selectedID = selectedID
        self.idFor = idFor
        self.onSelect = onSelect
        self.row = row
    }

    private struct IdentifiedItem: Identifiable {
        let id: ID
        let value: Item
    }

    public var body: some View {
        let identified = items.map { IdentifiedItem(id: idFor($0), value: $0) }
        GlassSheetChrome(title: title) {
            VStack(spacing: GlassPickerTokens.rowSpacing) {
                ForEach(identified) { wrapped in
                    Button {
                        onSelect(wrapped.value)
                        dismiss()
                    } label: {
                        let picked = selectedID.map { $0 == wrapped.id } ?? false
                        HStack(spacing: 10) {
                            row(wrapped.value)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, GlassPickerTokens.rowHPadding)
                        .frame(minHeight: GlassPickerTokens.rowMinHeight)
                        .background(
                            picked ? GlassPickerTokens.pickedFill : GlassPickerTokens.restingFill,
                            in: RoundedRectangle(cornerRadius: GlassPickerTokens.rowRadius)
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: GlassPickerTokens.rowRadius)
                                .strokeBorder(
                                    picked
                                        ? GlassPickerTokens.pickedStroke
                                        : GlassPickerTokens.restingStroke,
                                    lineWidth: GlassTokens.hairline
                                )
                        )
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, GlassPickerTokens.listHPadding)
            .padding(.bottom, GlassPickerTokens.listBottomPadding)
        }
    }
}
