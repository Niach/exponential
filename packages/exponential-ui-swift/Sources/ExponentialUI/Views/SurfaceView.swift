import SwiftUI
import UniformTypeIdentifiers
import ExponentialUIPrimitives

/// The SwiftUI face of a surface: the root node at the core's frames, as
/// wide as its container and as tall as its content (wrap it in YOUR
/// scroller), with the open layers presented on top. Coordinates are
/// physical (`layoutDirection` pinned: an RTL surface is already resolved
/// by the core; painters read `model.layoutDirection` for text and the
/// mirrored glyphs), fonts and icons come from the host, the generic
/// primitives read the theme's tokens.
///
/// It also feeds the platform into the model live (round-1 contract §9
/// SwiftUI): the colour scheme (`mode: system`), `colorSchemeContrast`
/// (`contrast: system`), Dynamic Type (the font scale), Reduce Motion, the
/// safe area (layer insets), an iPad pointer (hover); routes the hardware
/// keyboard into `SurfaceModel.handleKey` (macOS, iPad keyboards); and
/// presents the file picker a FileUpload asks for.
public struct ExponentialSurface: View {
    let model: SurfaceModel

    public init(model: SurfaceModel) {
        self.model = model
    }

    public var body: some View {
        SurfaceBody(model: model)
            .environment(model)
            .environment(\.layoutDirection, .leftToRight)
            .primitiveTokens(model.primitiveTokens)
    }
}

private struct SurfaceBody: View {
    let model: SurfaceModel
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.dynamicTypeSize) private var dynamicType
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @FocusState private var rootFocused: Bool
    @State private var safeArea = EdgeInsets()

    var body: some View {
        let size = model.surfaceSize
        ZStack(alignment: .topLeading) {
            if model.passCount > 0, let root = model.rootIndex {
                NodeView(index: root)
                    .layoutValue(key: NodeIndexKey.self, value: root)
                PaintedLayers(model: model)
            }
        }
        // The surface direction (text, mirrored glyphs) and reduced motion
        // every painter under the tree and the layers reads.
        .environment(\.xuiRTL, model.isRTL)
        .environment(\.xuiReducedMotion, model.reducedMotion)
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .frame(height: size.height > 0 ? size.height : nil, alignment: .topLeading)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width in
            model.setViewport(width: width, height: model.viewportHeight, maxHeight: model.maxHeight)
        }
        .onGeometryChange(for: EdgeInsets.self) { $0.safeAreaInsets } action: { insets in
            safeArea = insets
        }
        // Round 2: the host scroll view's offset of the whole surface
        // (unbounded lists window against it, sticky nodes pin to it).
        .onGeometryChange(for: CGPoint.self) { p in
            let f = p.frame(in: .scrollView)
            return CGPoint(x: max(0, -f.minX), y: max(0, -f.minY))
        } action: { offset in
            model.setSurfaceScroll(offset)
        }
        // Round 2: a `formatRelativeTime` without `now` re-binds once a minute.
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                model.tick()
            }
        }
        .modifier(KeyboardRouting(model: model, rootFocused: $rootFocused))
        .modifier(FilePicking(model: model))
        #if os(iOS)
        // A trackpad / mouse hovering the surface: hover capability on.
        .onContinuousHover { phase in
            if case .active = phase { model.pointerSeen() }
        }
        #endif
        .onAppear { syncPlatform() }
        .onChange(of: traits) { _, _ in syncPlatform() }
    }

    private var traits: PlatformTraits {
        var t = PlatformTraits(
            prefersDark: colorScheme == .dark,
            highContrast: contrast == .increased,
            fontScale: PlatformTraits.fontScale(dynamicType),
            reduceMotion: reduceMotion,
            safeArea: safeArea
        )
        // A pointer once seen stays (the model flips it on the first hover).
        t.pointerHover = t.pointerHover || model.platform.pointerHover
        return t
    }

    private func syncPlatform() {
        model.setPlatform(traits)
    }
}

/// The hardware keyboard: the surface root takes keyboard focus for the
/// nodes the model focuses (non-field nodes keep a VIRTUAL focus the core
/// paints as `focus` / `focus-visible`; text fields own their responder)
/// and hands every key to `SurfaceModel.handleKey`.
private struct KeyboardRouting: ViewModifier {
    let model: SurfaceModel
    var rootFocused: FocusState<Bool>.Binding

    func body(content: Content) -> some View {
        content
            .focusable()
            .focusEffectDisabled()
            .focused(rootFocused)
            .onKeyPress(phases: [.down, .repeat]) { press in
                guard let (key, shift, typed) = SurfaceKeys.name(press) else { return .ignored }
                return model.handleKey(key: key, shift: shift, typed: typed) ? .handled : .ignored
            }
            .onChange(of: model.focusRequest) { _, request in
                guard let request, let n = model.node(request.index) else { return }
                // Text fields take their own first responder (their views
                // observe the request); everything else keys through the root.
                if !n.isTextField { rootFocused.wrappedValue = true }
            }
            .onChange(of: rootFocused.wrappedValue) { _, focused in
                if !focused && model.focusedField == nil, model.focusedId != nil {
                    // Focus left the surface (another view took it).
                    model.blur()
                }
            }
    }
}

/// `KeyPress` → the model's key names (gpui's).
enum SurfaceKeys {
    static func name(_ press: KeyPress) -> (String, Bool, String?)? {
        let mods = press.modifiers
        if mods.contains(.command) || mods.contains(.control) || mods.contains(.option) { return nil }
        let shift = mods.contains(.shift)
        switch press.key {
        case .tab: return ("tab", shift, nil)
        case .escape: return ("escape", shift, nil)
        case .return: return ("enter", shift, nil)
        case .space: return ("space", shift, nil)
        case .upArrow: return ("up", shift, nil)
        case .downArrow: return ("down", shift, nil)
        case .leftArrow: return ("left", shift, nil)
        case .rightArrow: return ("right", shift, nil)
        case .home: return ("home", shift, nil)
        case .end: return ("end", shift, nil)
        case .pageUp: return ("pageup", shift, nil)
        case .pageDown: return ("pagedown", shift, nil)
        case .delete: return ("backspace", shift, nil)
        case .deleteForward: return ("delete", shift, nil)
        default: break
        }
        switch press.key.character {
        // Shift+Tab arrives as a back-tab on macOS.
        case "\u{19}": return ("tab", true, nil)
        // NSF10FunctionKey.
        case "\u{F70D}": return ("f10", shift, nil)
        default: break
        }
        let typed = press.characters
        guard typed.count == 1, let c = typed.unicodeScalars.first, !CharacterSet.controlCharacters.contains(c) else { return nil }
        return (typed.lowercased(), shift, typed)
    }
}

/// The file picker a FileUpload asks for (`pickFiles` when the host has no
/// picker of its own): `.fileImporter`, the picked files read and handed to
/// `SurfaceModel.filesPicked`.
private struct FilePicking: ViewModifier {
    let model: SurfaceModel

    private var presented: Binding<Bool> {
        Binding(get: { model.filePickRequest != nil }, set: { if !$0 { model.filePickFinished() } })
    }

    func body(content: Content) -> some View {
        let request = model.filePickRequest
        content.fileImporter(isPresented: presented, allowedContentTypes: SurfaceModel.contentTypes(accept: request?.accept), allowsMultipleSelection: request?.multiple ?? false) { result in
            let componentId = request?.componentId
            model.filePickFinished()
            guard let componentId, case let .success(urls) = result else { return }
            model.filesPicked(componentId: componentId, urls: urls)
        }
    }
}
