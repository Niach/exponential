import ExpCore
import SwiftUI

// N-12: THE Dock ×iOS (fixtures/dock.json ×4: web `@exp/ui` dock.tsx, desktop
// `dock.rs`, Android `Dock.kt`): the floating band over the transcript's
// bottom edge: the DockCards (a waiting plan or question, THE Queue, the
// queued messages, the suggested tasks, a status banner) piled like the toasts (`Dock.layout`,
// anchored at the bottom: the front card last, the older ones peeking out
// above it, `visible` at most; a tap fans them `gap` apart, the next tap on a
// card opens it), then THE composer, which never leaves. An opened card is
// THE bottom sheet (`DockSheetChrome` inside `GlassSheetChrome`: a pinned
// header, ONE scrolling body, a pinned footer, fitted up to
// `sheetMaxHeightFraction`). The host puts the band in its scroller's bottom
// `safeAreaInset`, so the band's height IS the transcript's inset and the
// jump button's offset.

extension Dock.Tint {
    /// Web `TINT_CLASS`: plan = primary, question = yellow, muted, warning.
    public var color: Color {
        switch self {
        case .plan: DesignTokens.Palette.primary
        case .question: DesignTokens.Semantic.yellow
        case .muted: .white.opacity(TextOpacity.secondary)
        case .warning: DockMetrics.warning
        }
    }
}

public enum DockMetrics {
    /// The cards' and the composer's gap in the band (web `gap-1.5`).
    public static let bandGap: CGFloat = 6
    /// Web `amber-400`: a warning banner's tint.
    public static let warning = Color(red: 0.984, green: 0.749, blue: 0.141)
    /// The band's side gutter (the transcript's).
    public static let gutter: CGFloat = 16
    /// A card's padding (web `p-3`).
    public static let cardPadding: CGFloat = 12

    /// A row's concept icon as its registry glyph.
    public static func glyph(_ concept: String) -> String {
        switch concept {
        case Dock.Icons.plan: AppIcons.codingPlan
        case Dock.Icons.question: AppIcons.uiHelp
        case Dock.Icons.tasks: AppIcons.traceTodo
        case Dock.Icons.queued: AppIcons.uiQueued
        case Dock.Icons.suggestions: AppIcons.uiChecklist
        case "ui-usage": AppIcons.uiUsage
        case "coding-compact": AppIcons.codingCompact
        case "ui-device-offline": AppIcons.uiDeviceOffline
        case "ui-refresh": AppIcons.uiRefresh
        case "ui-loading": AppIcons.uiLoading
        // EXP-1374: the session banner (session-card.json `icons`).
        case "run-pause": AppIcons.runPause
        case "trace-background": AppIcons.traceBackground
        // #EXP-1405: a queued session waiting for its device.
        case "ui-clock": AppIcons.uiClock
        default: concept
        }
    }
}

/// THE band: the pile (when any card waits), then THE composer. `rows` =
/// `Dock.rows(sources)`, top to bottom. `renderSheet` = an expandable card's
/// sheet content (a `DockSheetChrome`), `renderAction` = a banner's button.
public struct DockView<Composer: View>: View {
    let rows: [Dock.Row]
    @Binding var openId: String?
    let renderSheet: (Dock.Row, @escaping () -> Void) -> AnyView?
    let renderAction: (Dock.Row) -> AnyView?
    let composer: Composer

    @State private var expanded: Bool
    @Environment(\.motion) private var motion

    public init(
        rows: [Dock.Row],
        openId: Binding<String?>,
        initialExpanded: Bool = false,
        renderSheet: @escaping (Dock.Row, @escaping () -> Void) -> AnyView?,
        renderAction: @escaping (Dock.Row) -> AnyView? = { _ in nil },
        @ViewBuilder composer: () -> Composer
    ) {
        self.rows = rows
        _openId = openId
        _expanded = State(initialValue: initialExpanded)
        self.renderSheet = renderSheet
        self.renderAction = renderAction
        self.composer = composer()
    }

    private var view: Dock.View { Dock.view(rows: rows, expanded: expanded, openId: openId) }

    public var body: some View {
        let view = view
        VStack(spacing: DockMetrics.bandGap) {
            if view.mode != .none {
                DockPile(
                    rows: view.rows,
                    expanded: Binding(get: { view.mode == .fanned }, set: { next in
                        withAnimation(motion.slow) { expanded = next }
                    }),
                    single: view.mode == .single,
                    onOpen: { row in
                        // A card opens its sheet; a banner re-piles the fan.
                        if row.expandable { openId = row.id }
                        withAnimation(motion.slow) { expanded = false }
                    },
                    renderAction: renderAction
                )
                // The composer column: the transcript's gutter and cap.
                .padding(.horizontal, DockMetrics.gutter)
                .frame(maxWidth: DesignTokens.Transcript.maxWidth)
                .frame(maxWidth: .infinity)
                .transition(.opacity)
            }
            composer
        }
        .animation(motion.standard, value: rows.map(\.id))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Dock.TestIds.dock)
        // Release train 2026-10-23 (N-12 F2): the open card vanished (the
        // run ended, the ask was answered): forget its id, or a later row
        // with the same fixed id (`tasks`, `queued`, `banner:*`) reopens the
        // sheet unasked.
        .onChange(of: view.openId) { _, id in
            if id == nil, openId != nil { openId = nil }
        }
        .sheet(isPresented: Binding(
            get: { view.openId != nil },
            set: { if !$0 { openId = nil } }
        )) {
            if let id = view.openId, let row = rows.first(where: { $0.id == id }),
               let sheet = renderSheet(row, { openId = nil }) {
                sheet
            }
        }
    }
}

/// The pile: every card placed by `Dock.layout` (measured heights), the
/// front one last; collapsed = the older cards peek out above it, their
/// words hidden; a tap on a collapsed pile fans it, a tap on a fanned card
/// (or the single one) opens it.
public struct DockPile: View {
    let rows: [Dock.Row]
    @Binding var expanded: Bool
    var single = false
    let onOpen: (Dock.Row) -> Void
    var renderAction: (Dock.Row) -> AnyView? = { _ in nil }

    @State private var heights: [String: CGFloat] = [:]
    @Environment(\.motion) private var motion

    public init(
        rows: [Dock.Row],
        expanded: Binding<Bool>,
        single: Bool = false,
        onOpen: @escaping (Dock.Row) -> Void,
        renderAction: @escaping (Dock.Row) -> AnyView? = { _ in nil }
    ) {
        self.rows = rows
        _expanded = expanded
        self.single = single
        self.onOpen = onOpen
        self.renderAction = renderAction
    }

    public var body: some View {
        let fanned = expanded || single
        let layout = Dock.layout(heights: rows.map { Double(heights[$0.id] ?? 0) }, expanded: fanned)
        let frontId = rows.last?.id
        ZStack(alignment: .top) {
            ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                let item = layout.items[index]
                let front = row.id == frontId
                DockCard(
                    row: row,
                    hidden: !fanned && !front,
                    action: renderAction(row),
                    onOpen: {
                        // A touch fans the pile first; the next touch opens.
                        if !fanned {
                            expanded = true
                            return
                        }
                        onOpen(row)
                    }
                )
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height in
                    heights[row.id] = height
                }
                .scaleEffect(x: item.scale, y: 1, anchor: .top)
                .offset(y: CGFloat(item.offset))
                .opacity(item.visible ? 1 : 0)
                .allowsHitTesting(item.visible)
                .zIndex(Double(index))
                .transition(.opacity)
            }
        }
        .frame(height: CGFloat(layout.height), alignment: .top)
        .frame(maxWidth: .infinity)
        .animation(motion.slow, value: fanned)
        .animation(motion.slow, value: heights)
        .onChange(of: rows.map(\.id)) { _, ids in
            let live = Set(ids)
            heights = heights.filter { live.contains($0.key) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Dock.TestIds.pile)
    }
}

/// A DockCard collapsed: the AskCard chrome (the kind icon, the tinted
/// label, the muted caption, the trailing meta) over the one title line and
/// the open chevron (an expandable card) or the banner's action. `hidden` =
/// a card behind the front one while piled (its chrome peeks, its words do
/// not).
public struct DockCard: View {
    let row: Dock.Row
    var hidden = false
    var action: AnyView?
    var onOpen: (() -> Void)?

    public init(row: Dock.Row, hidden: Bool = false, action: AnyView? = nil, onOpen: (() -> Void)? = nil) {
        self.row = row
        self.hidden = hidden
        self.action = action
        self.onOpen = onOpen
    }

    public var body: some View {
        let content = HStack(alignment: .top, spacing: 8) {
            AppIcon(DockMetrics.glyph(row.icon), size: 14)
                .foregroundStyle(row.tint.color)
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(row.label)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(row.tint.color)
                        .lineLimit(1)
                        .layoutPriority(1)
                    if let caption = row.caption, !caption.isEmpty {
                        Text(caption)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                    if let meta = row.meta, !meta.isEmpty {
                        Spacer(minLength: 0)
                        Text(meta)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                            .fixedSize()
                    }
                }
                HStack(spacing: 8) {
                    Text(row.title)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(0.9))
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if row.expandable {
                        AppIcon(AppIcons.uiChevronRight, size: 14)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    } else if let action {
                        action
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .opacity(hidden ? 0 : 1)
        .padding(DockMetrics.cardPadding)
        // ONE gray ×4: THE card fill under the card hairline.
        .glassCard()
        .contentShape(Rectangle())

        Group {
            if let onOpen, row.expandable || hidden {
                Button(action: onOpen) { content }
                    .buttonStyle(.plain)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel("\(row.label): \(row.title)")
                    .accessibilityAddTraits(.isButton)
            } else if let onOpen {
                // A banner: a tap on it fans a pile; its action is its own.
                content
                    .onTapGesture(perform: onOpen)
                    .accessibilityElement(children: .contain)
            } else {
                content
                    .accessibilityElement(children: .contain)
            }
        }
        .accessibilityHidden(hidden)
        .accessibilityIdentifier(Dock.TestIds.card)
    }
}

/// The sheet's face: the pinned header (the card's icon, label, caption, the
/// content's `meta`, the close glyph) over ONE scrolling body and an
/// optional pinned footer, inside THE sheet shell (`GlassSheetChrome`: the
/// grabber, drag to dismiss, fitted up to 85 % of the screen).
public struct DockSheetChrome<Body: View, Footer: View>: View {
    let row: Dock.Row
    var meta: String?
    let onClose: () -> Void
    let sheetBody: Body
    let footer: Footer

    public init(
        row: Dock.Row,
        meta: String? = nil,
        onClose: @escaping () -> Void,
        @ViewBuilder body: () -> Body,
        @ViewBuilder footer: () -> Footer
    ) {
        self.row = row
        self.meta = meta
        self.onClose = onClose
        self.sheetBody = body()
        self.footer = footer()
    }

    public var body: some View {
        GlassSheetChrome(
            title: nil,
            height: .fitted,
            headerTrailing: { EmptyView() },
            pinnedHeader: {
                DockSheetHeader(row: row, meta: meta, onClose: onClose)
            },
            content: {
                sheetBody
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.bottom, 12)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier(Dock.TestIds.sheetBody)
            },
            primaryAction: {
                footer
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier(Dock.TestIds.sheetFooter)
            },
            actionDivider: true,
            // A footer-less sheet (THE Queue, the queued messages, the relay's
            // ask) draws no footer strip and no divider, as on web (the footer
            // is only rendered when it holds something).
            actionVisible: Footer.self != EmptyView.self,
            // ONE gray ×4: the sheet wears the card's fill, not the phone
            // sheet's darker ground.
            background: GlassTokens.opaqueCardFill
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Dock.TestIds.sheet)
    }
}

extension DockSheetChrome where Footer == EmptyView {
    public init(
        row: Dock.Row,
        meta: String? = nil,
        onClose: @escaping () -> Void,
        @ViewBuilder body: () -> Body
    ) {
        self.init(row: row, meta: meta, onClose: onClose, body: body, footer: { EmptyView() })
    }
}

/// The sheet's pinned header row (also the styleguide's static frame).
public struct DockSheetHeader: View {
    let row: Dock.Row
    var meta: String?
    var onClose: (() -> Void)?

    public init(row: Dock.Row, meta: String? = nil, onClose: (() -> Void)? = nil) {
        self.row = row
        self.meta = meta
        self.onClose = onClose
    }

    public var body: some View {
        HStack(spacing: 8) {
            AppIcon(DockMetrics.glyph(row.icon), size: 14)
                .foregroundStyle(row.tint.color)
            Text(row.label)
                .font(.caption.weight(.medium))
                .foregroundStyle(row.tint.color)
                .lineLimit(1)
                .accessibilityAddTraits(.isHeader)
            if let caption = row.caption, !caption.isEmpty {
                Text(caption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Spacer(minLength: 0)
            if let meta, !meta.isEmpty {
                Text(meta)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize()
                    .accessibilityIdentifier(PlanCard.TestIds.step)
            }
            GhostIconButton(
                AppIcons.uiClose,
                accessibilityLabel: Dock.Copy.close,
                size: 24,
                glyphSize: 14,
                action: { onClose?() }
            )
            .accessibilityIdentifier(Dock.TestIds.close)
        }
        .padding(.horizontal, 16)
        .padding(.top, GlassSheetTokens.headerTopPadding)
        .padding(.bottom, 8)
    }
}

/// The styleguide's sheet at rest (no presentation): the same header, body
/// and footer in a box wearing the sheet's face, `height` tall.
public struct DockSheetSpecimenFrame<Body: View, Footer: View>: View {
    let row: Dock.Row
    var meta: String?
    var height: CGFloat
    let sheetBody: Body
    let footer: Footer

    public init(
        row: Dock.Row,
        meta: String? = nil,
        height: CGFloat = 360,
        @ViewBuilder body: () -> Body,
        @ViewBuilder footer: () -> Footer
    ) {
        self.row = row
        self.meta = meta
        self.height = height
        self.sheetBody = body()
        self.footer = footer()
    }

    public var body: some View {
        VStack(spacing: 0) {
            DockSheetHeader(row: row, meta: meta)
            ScrollView {
                sheetBody
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.bottom, 12)
            }
            if Footer.self != EmptyView.self {
                footer
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .padding(.horizontal, GlassSheetTokens.actionHPadding)
                    .padding(.vertical, GlassSheetTokens.actionVPadding)
                    .overlay(alignment: .top) { GlassSheetDivider() }
            }
        }
        .frame(height: height)
        .background(GlassTokens.opaqueCardFill)
        .clipShape(UnevenRoundedRectangle(
            topLeadingRadius: CGFloat(Dock.sheetRadius), topTrailingRadius: CGFloat(Dock.sheetRadius)
        ))
        .overlay(
            UnevenRoundedRectangle(
                topLeadingRadius: CGFloat(Dock.sheetRadius), topTrailingRadius: CGFloat(Dock.sheetRadius)
            )
            .stroke(DesignTokens.Glass.strokeCard, lineWidth: 1)
        )
    }
}

/// AUTO-OPEN (dock.json): the plan or question that just arrived opens its
/// sheet while the view is in front (`enabled` and the app active). Until
/// the host's data has loaded (`ready`: the relay live, the thread's rows
/// in) and for `autoOpenGraceMs` after that, every card shown only seeds
/// the memory, so what already waited never opens itself; every id seen is
/// remembered so a card never re-opens.
public struct DockAutoOpen: ViewModifier {
    let rows: [Dock.Row]
    let enabled: Bool
    let ready: Bool
    let open: (String) -> Void

    /// dock.json `autoOpenGraceMs` (`Dock.autoOpenGraceMs`).
    private static let grace: TimeInterval = TimeInterval(Dock.autoOpenGraceMs) / 1000

    @State private var seen: Set<String> = []
    @State private var readyAt: Date? = nil
    @Environment(\.scenePhase) private var scenePhase

    public func body(content: Content) -> some View {
        content
            .onAppear { check(rows) }
            .onChange(of: rows.map(\.id)) { _, _ in check(rows) }
            .onChange(of: ready) { _, _ in check(rows) }
    }

    private func check(_ rows: [Dock.Row]) {
        let ids = rows.map(\.id)
        guard ready else {
            readyAt = nil
            seen.formUnion(ids)
            return
        }
        let since = readyAt ?? Date()
        if readyAt == nil { readyAt = since }
        guard Date().timeIntervalSince(since) >= Self.grace else {
            seen.formUnion(ids)
            return
        }
        let next = Dock.autoOpen(previous: seen, rows: rows.map { (id: $0.id, kind: $0.kind) })
        seen.formUnion(ids)
        guard let next, enabled, scenePhase == .active else { return }
        open(next)
    }
}

extension View {
    public func dockAutoOpen(_ rows: [Dock.Row], enabled: Bool = true, ready: Bool = true, open: @escaping (String) -> Void) -> some View {
        modifier(DockAutoOpen(rows: rows, enabled: enabled, ready: ready, open: open))
    }
}
