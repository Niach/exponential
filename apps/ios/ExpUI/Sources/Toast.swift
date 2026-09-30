import ExpCore
import SwiftUI
import UIKit

// EXP-1031: THE toast — one shared stacking component, identical ×4 (web
// packages/ui toast.tsx, desktop crates/ui toast.rs, Android Toast.kt). The
// numbers and the per-item geometry live in ExpCore `ToastStack` (locked to
// packages/domain-contract/fixtures/toast-stack.json); this file only draws
// what it answers.
//
// - A toast is one sentence, an optional description, an optional action and
//   a kind whose colour sits on the ICON alone.
// - The card is the opaque glass card at radius lg — no shadow, no material.
// - Phones: full width minus `mobileViewportOffset`, bottom-centre, above the
//   tab bar when one is up (`ToastBottomInsetKey`).
// - Newest in front, `visible` of them collapsed; a tap expands the stack
//   (every toast at full size, `gap` apart) and pauses the clock.
// - Dismiss = the close glyph, a swipe past `swipeThreshold`, or the action.
//
// Mounted ONCE (`.toastHost`, AppNavigator). The stack draws in its own
// pass-through overlay window so a toast fired from inside a sheet is still
// on top; every other touch falls through to the app.

// MARK: - Toaster

/// The app's one toast queue. Views read it from `@Environment(\.toaster)`
/// and call `show`/`success`/`error`/`info`/`warning`.
@MainActor
@Observable
public final class Toaster {
    /// The environment's default — so previews and any view outside the root
    /// host still reach the same queue.
    public static let shared = Toaster()

    public private(set) var queue = ToastQueue()

    @ObservationIgnored private var clocks: [UUID: Task<Void, Never>] = [:]
    @ObservationIgnored private var remaining: [UUID: Duration] = [:]
    @ObservationIgnored private var startedAt: [UUID: ContinuousClock.Instant] = [:]

    public init() {}

    /// Live toasts, oldest first (the last one is in front).
    public var items: [ToastItem] { queue.items }

    /// Expanded = every toast at full size and the auto-dismiss clocks paused.
    public var expanded: Bool {
        get { queue.expanded }
        set {
            let next = newValue && !queue.items.isEmpty
            guard next != queue.expanded else { return }
            queue.expanded = next
            if next { pauseClocks() } else { resumeClocks() }
        }
    }

    public func show(_ item: ToastItem) {
        let before = Set(queue.items.map(\.id))
        queue.append(item)
        let after = Set(queue.items.map(\.id))
        for dropped in before.subtracting(after) { forget(dropped) }
        remaining[item.id] = .milliseconds(ToastStack.Constants.durationMs)
        if !queue.expanded { startClock(item.id) }
        AccessibilityNotification.Announcement(item.title).post()
    }

    public func success(_ title: String, description: String? = nil, action: ToastAction? = nil) {
        show(ToastItem(kind: .success, title: title, description: description, action: action))
    }

    public func error(_ title: String, description: String? = nil, action: ToastAction? = nil) {
        show(ToastItem(kind: .error, title: title, description: description, action: action))
    }

    public func info(_ title: String, description: String? = nil, action: ToastAction? = nil) {
        show(ToastItem(kind: .info, title: title, description: description, action: action))
    }

    public func warning(_ title: String, description: String? = nil, action: ToastAction? = nil) {
        show(ToastItem(kind: .warning, title: title, description: description, action: action))
    }

    public func dismiss(_ id: UUID) {
        forget(id)
        queue.dismiss(id)
    }

    public func dismissAll() {
        for id in queue.items.map(\.id) { forget(id) }
        queue.removeAll()
    }

    private func forget(_ id: UUID) {
        clocks.removeValue(forKey: id)?.cancel()
        remaining.removeValue(forKey: id)
        startedAt.removeValue(forKey: id)
    }

    private func startClock(_ id: UUID) {
        guard let left = remaining[id] else { return }
        clocks[id]?.cancel()
        startedAt[id] = .now
        clocks[id] = Task { [weak self] in
            try? await Task.sleep(for: left)
            guard !Task.isCancelled else { return }
            self?.dismiss(id)
        }
    }

    private func pauseClocks() {
        let now = ContinuousClock.now
        for (id, task) in clocks {
            task.cancel()
            if let start = startedAt[id], let left = remaining[id] {
                remaining[id] = max(.zero, left - (now - start))
            }
        }
        clocks.removeAll()
        startedAt.removeAll()
    }

    private func resumeClocks() {
        for item in queue.items { startClock(item.id) }
    }
}

private struct ToasterKey: EnvironmentKey {
    static var defaultValue: Toaster { MainActor.assumeIsolated { Toaster.shared } }
}

public extension EnvironmentValues {
    var toaster: Toaster {
        get { self[ToasterKey.self] }
        set { self[ToasterKey.self] = newValue }
    }
}

// MARK: - Metrics

/// The host's placement rules — every number from the contract.
public enum ToastHostMetrics {
    /// The card's radius (`Radius.lg`, the fixture's "radius lg").
    public static let cardRadius: CGFloat = DesignTokens.Radius.lg
    /// The phone's screen inset on each side and below when no bar is up.
    public static let screenInset: CGFloat = CGFloat(ToastStack.Constants.mobileViewportOffset)
    /// A regular-width (iPad) stack keeps the pointer width.
    public static let regularWidth: CGFloat = CGFloat(ToastStack.Constants.width)
    /// Above the floating tab bar: its 64pt band + the 16pt viewport offset
    /// (the same 80pt `.tabBarBottomInset()` reserves).
    public static let tabBarClearance: CGFloat = 64 + screenInset
    /// A drag past this dismisses.
    public static let swipeThreshold: CGFloat = CGFloat(ToastStack.Constants.swipeThreshold)
    /// A card's height before it has been measured.
    public static let estimatedHeight: CGFloat = 56
    /// The icon's size.
    public static let iconSize: CGFloat = 16

    /// Full width minus the inset on both sides; capped at the pointer width
    /// on a regular-width screen.
    public static func cardWidth(container: CGFloat, regular: Bool) -> CGFloat {
        let full = max(container - 2 * screenInset, 0)
        return regular ? min(full, regularWidth) : full
    }

    /// Space below the stack above the safe area: the bar's clearance when a
    /// screen reports one, else the viewport offset.
    public static func bottomPadding(bottomInset: CGFloat) -> CGFloat {
        max(bottomInset, screenInset)
    }

    /// Whether a drag of `translation` dismisses: sideways either way, or
    /// downward (the stack sits at the bottom).
    public static func dismissesOnSwipe(_ translation: CGSize) -> Bool {
        abs(translation.width) > swipeThreshold || translation.height > swipeThreshold
    }
}

/// The bottom clearance a screen asks the root host for (the tab bar's). The
/// largest reported value wins.
public struct ToastBottomInsetKey: PreferenceKey {
    public static let defaultValue: CGFloat = 0
    public static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

// MARK: - Card

public extension ToastKind {
    /// The concept icon (ui-success / ui-error / ui-info / ui-warning).
    var iconName: String {
        switch self {
        case .success: AppIcons.uiSuccess
        case .error: AppIcons.uiError
        case .info: AppIcons.uiInfo
        case .warning: AppIcons.uiWarning
        }
    }

    /// The kind colour — the ICON's alone.
    var tint: Color {
        switch self {
        case .success: DesignTokens.Semantic.green
        case .error: DesignTokens.Semantic.red
        case .info: DesignTokens.Semantic.blue
        case .warning: DesignTokens.Semantic.yellow
        }
    }
}

/// One toast card.
public struct ToastCard: View {
    let item: ToastItem
    let onDismiss: () -> Void
    let onAction: () -> Void
    /// False for a card peeking out BEHIND the front one: only its card
    /// surface shows (sonner hides a back toast's content).
    let contentVisible: Bool

    public init(
        item: ToastItem,
        contentVisible: Bool = true,
        onDismiss: @escaping () -> Void,
        onAction: @escaping () -> Void = {}
    ) {
        self.item = item
        self.contentVisible = contentVisible
        self.onDismiss = onDismiss
        self.onAction = onAction
    }

    public var body: some View {
        HStack(alignment: .center, spacing: 10) {
            AppIcon(item.kind.iconName, size: ToastHostMetrics.iconSize)
                .foregroundStyle(item.kind.tint)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .fixedSize(horizontal: false, vertical: true)
                if let description = item.description {
                    Text(description)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let action = item.action {
                GlassPill(action.label, size: .sm, mode: .action { onAction() })
            }
            Button(action: onDismiss) {
                AppIcon(AppIcons.uiClose, size: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .frame(width: DesignTokens.Size.controlSm, height: DesignTokens.Size.controlSm)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Dismiss")
        }
        .opacity(contentVisible ? 1 : 0)
        .padding(.leading, 14)
        .padding(.trailing, 8)
        .padding(.vertical, 12)
        .glassCard(cornerRadius: ToastHostMetrics.cardRadius, isOpaque: true)
        .accessibilityElement(children: .combine)
        .accessibilityAction(named: "Dismiss", onDismiss)
        .modifier(ToastActionAccessibility(label: item.action?.label, perform: onAction))
    }
}

private struct ToastActionAccessibility: ViewModifier {
    let label: String?
    let perform: () -> Void

    func body(content: Content) -> some View {
        if let label {
            content.accessibilityAction(named: label, perform)
        } else {
            content
        }
    }
}

// MARK: - Stack

/// The stack itself, bottom-centre. Drawn inside the overlay window.
struct ToastStackView: View {
    let toaster: Toaster
    let state: ToastOverlayState

    @Environment(\.motion) private var motion
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var heights: [UUID: CGFloat] = [:]
    @State private var drags: [UUID: CGSize] = [:]

    var body: some View {
        GeometryReader { proxy in
            let width = ToastHostMetrics.cardWidth(
                container: proxy.size.width, regular: sizeClass == .regular
            )
            VStack(spacing: 0) {
                Spacer(minLength: 0)
                stack(width: width)
            }
            .frame(maxWidth: .infinity)
            .padding(.bottom, ToastHostMetrics.bottomPadding(bottomInset: state.bottomInset))
        }
        .animation(motion.slow, value: state.bottomInset)
    }

    private func stack(width: CGFloat) -> some View {
        let items = toaster.items
        let expanded = toaster.expanded
        let measured = items.map { heights[$0.id] ?? ToastHostMetrics.estimatedHeight }
        // Collapsed, every card behind the front one takes the FRONT card's
        // height (sonner's `--front-toast-height`), so each peeks exactly
        // `peek` whatever its own content.
        let frontHeight = measured.last ?? 0
        let drawn = expanded ? measured : measured.map { _ in frontHeight }
        let layout = ToastStack.geometry(
            heights: drawn.map(Double.init),
            expanded: expanded,
            anchoredBottom: true
        )
        return ZStack(alignment: .top) {
            ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                let placement = layout.items[index]
                let drag = drags[item.id] ?? .zero
                ToastCard(
                    item: item,
                    contentVisible: expanded || index == items.count - 1,
                    onDismiss: { dismiss(item.id) },
                    onAction: {
                        item.action?.handler()
                        dismiss(item.id)
                    }
                )
                .frame(width: width)
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height in
                    heights[item.id] = height
                }
                .frame(
                    height: expanded || index == items.count - 1 ? nil : frontHeight,
                    alignment: .top
                )
                .clipShape(RoundedRectangle(cornerRadius: ToastHostMetrics.cardRadius))
                // Anchored at the TOP so an older card's `peek` stays visible
                // above the one in front of it.
                .scaleEffect(placement.scale, anchor: .top)
                .offset(x: drag.width, y: CGFloat(placement.offset) + drag.height)
                .opacity(placement.visible ? 1 : 0)
                .zIndex(Double(index))
                .allowsHitTesting(placement.visible)
                .onTapGesture { toaster.expanded.toggle() }
                .gesture(swipe(item.id))
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .frame(width: width, height: CGFloat(layout.height), alignment: .top)
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { rect in
            state.hitRect = rect
        }
        .animation(motion.standard, value: items.map(\.id))
        .animation(motion.slow, value: toaster.expanded)
        .animation(motion.slow, value: heights)
        .onChange(of: items.map(\.id)) { _, ids in
            let live = Set(ids)
            heights = heights.filter { live.contains($0.key) }
            drags = drags.filter { live.contains($0.key) }
        }
    }

    private func swipe(_ id: UUID) -> some Gesture {
        DragGesture(minimumDistance: 10)
            .onChanged { value in
                // Sideways either way, downward only — the stack sits on the
                // bottom edge, so up has nowhere to go.
                drags[id] = CGSize(
                    width: value.translation.width,
                    height: max(value.translation.height, 0)
                )
            }
            .onEnded { value in
                if ToastHostMetrics.dismissesOnSwipe(value.translation) {
                    dismiss(id)
                } else {
                    withAnimation(motion.standard) { drags[id] = nil }
                }
            }
    }

    private func dismiss(_ id: UUID) {
        withAnimation(motion.standard) {
            toaster.dismiss(id)
        }
    }
}

// MARK: - Host

public extension View {
    /// Mount the app's ONE toast stack. `bottomInset` = the clearance above
    /// the safe area (the tab bar's when it is up; 0 = the viewport offset).
    func toastHost(_ toaster: Toaster, bottomInset: CGFloat = 0) -> some View {
        modifier(ToastHost(toaster: toaster, bottomInset: bottomInset))
    }
}

public struct ToastHost: ViewModifier {
    let toaster: Toaster
    let bottomInset: CGFloat
    @State private var overlay = ToastOverlay()

    public init(toaster: Toaster, bottomInset: CGFloat) {
        self.toaster = toaster
        self.bottomInset = bottomInset
    }

    public func body(content: Content) -> some View {
        content
            .background(
                WindowSceneReader { scene in
                    overlay.install(in: scene, toaster: toaster)
                }
                .allowsHitTesting(false)
            )
            .onChange(of: bottomInset, initial: true) { _, inset in
                overlay.state.bottomInset = inset
            }
    }
}

/// What the overlay window shares with its SwiftUI content.
@MainActor
@Observable
final class ToastOverlayState {
    var bottomInset: CGFloat = 0
    /// The stack box in window coordinates — the only touch target.
    var hitRect: CGRect = .zero
}

/// Owns the pass-through window the stack draws in.
@MainActor
final class ToastOverlay {
    let state = ToastOverlayState()
    private var window: ToastPassthroughWindow?

    func install(in scene: UIWindowScene, toaster: Toaster) {
        guard window?.windowScene !== scene else { return }
        let window = ToastPassthroughWindow(windowScene: scene)
        window.state = state
        window.windowLevel = UIWindow.Level(rawValue: UIWindow.Level.normal.rawValue + 1)
        window.overrideUserInterfaceStyle = .dark
        window.backgroundColor = .clear
        let host = ToastHostingController(
            rootView: ToastStackView(toaster: toaster, state: state)
                .preferredColorScheme(.dark)
        )
        host.view.backgroundColor = .clear
        window.rootViewController = host
        window.isHidden = false
        self.window = window
    }
}

private final class ToastHostingController<Content: View>: UIHostingController<Content> {
    override var preferredStatusBarStyle: UIStatusBarStyle { .lightContent }
}

/// Touches land only inside the stack box; everything else falls through to
/// the app's own window.
private final class ToastPassthroughWindow: UIWindow {
    weak var state: ToastOverlayState?

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard let rect = state?.hitRect, !rect.isEmpty, rect.contains(point) else { return nil }
        return super.hitTest(point, with: event)
    }
}

/// Reports the hosting window's scene once the view is in a window.
private struct WindowSceneReader: UIViewRepresentable {
    let onScene: @MainActor (UIWindowScene) -> Void

    func makeUIView(context: Context) -> ProbeView {
        let view = ProbeView()
        view.onScene = onScene
        return view
    }

    func updateUIView(_ uiView: ProbeView, context: Context) {
        uiView.onScene = onScene
    }

    final class ProbeView: UIView {
        var onScene: (@MainActor (UIWindowScene) -> Void)?

        override func didMoveToWindow() {
            super.didMoveToWindow()
            if let scene = window?.windowScene { onScene?(scene) }
        }
    }
}
