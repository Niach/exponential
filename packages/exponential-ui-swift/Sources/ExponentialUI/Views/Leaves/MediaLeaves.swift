import SwiftUI
import AVKit
import ExponentialUIPrimitives

struct IconLeaf: View {
    let cx: LeafContext

    var body: some View {
        let name = cx.props.str("name").isEmpty ? "ui-icon-placeholder" : cx.props.str("name")
        let color = cx.props.str("tone").isEmpty ? cx.ink : (cx.tone(cx.props.str("tone")) ?? cx.ink)
        ConceptIcon(name: name, size: max(1, min(cx.inner.width, cx.inner.height)), color: color, model: cx.model)
            .frame(width: cx.size.width, height: cx.size.height)
    }
}

/// `Avatar`: the image when the host resolves one, else tinted initials.
struct AvatarLeaf: View {
    let cx: LeafContext

    var body: some View {
        let name = cx.props.str("name")
        let seed = cx.props.str("seed").isEmpty ? name : cx.props.str("seed")
        let fallback = cx.part("Avatar", "fallback")
        let size = min(cx.size.width, cx.size.height)
        let fs = fallback.px("fontSize")
        let src = cx.props.str("src")
        let request = src.isEmpty ? nil : cx.model.mediaRequest(src)
        Group {
            if let request {
                AvatarView(name: name, seed: seed, size: size, dark: cx.dark, fill: seed.isEmpty ? fallback.style.background : nil, ink: seed.isEmpty ? fallback.color : nil, fontSize: fs) {
                    MediaImage(request: request) { image in
                        image.resizable().scaledToFill()
                    } placeholder: {
                        AvatarView(name: name, seed: seed, size: size, dark: cx.dark, fontSize: fs)
                    }
                }
            } else {
                AvatarView(name: name, seed: seed, size: size, dark: cx.dark, fill: seed.isEmpty ? fallback.style.background : nil, ink: seed.isEmpty ? fallback.color : nil, fontSize: fs)
            }
        }
        .frame(width: cx.size.width, height: cx.size.height)
    }
}

/// The tinted placeholder an `Image` paints without a loadable source (or
/// while loading / after an error): its `fallback` glyph (default
/// `builtinIcons["Image.fallback"]`) over the alt text.
struct ImagePlaceholder: View {
    let ink: Color
    let label: String
    var icon: String = BuiltinIcons.name("Image.fallback")
    let model: SurfaceModel

    var body: some View {
        VStack(spacing: 4) {
            ConceptIcon(name: icon, size: 20, color: ink.opacity(0.6), model: model)
            if !label.isEmpty {
                Text(label).font(.system(size: 12)).foregroundStyle(ink.opacity(0.6)).lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(ink.opacity(0.08))
    }
}

/// `Image`: the host-loaded picture fitted by `fit`; `focalX` / `focalY`
/// (0...1, 0.5 default) keep that point when the fit crops or letterboxes
/// (CSS `object-position`, gpui `focal_bounds`); the fallback placeholder
/// on no source, while loading and on error. `loading: lazy` loads when the
/// leaf first appears (a SwiftUI view only exists on screen anyway).
struct ImageLeaf: View {
    let cx: LeafContext

    var body: some View {
        let src = cx.props.str("src")
        let alt = cx.props.str("alt")
        let muted = cx.themeColor("mutedForeground") ?? cx.ink
        let fallback = cx.props.str("fallback").isEmpty ? BuiltinIcons.name("Image.fallback") : cx.props.str("fallback")
        let request = src.isEmpty ? nil : cx.model.mediaRequest(src)
        let fit = cx.props.str("fit")
        let fx = CGFloat(min(1, max(0, cx.props.num("focalX") ?? 0.5)))
        let fy = CGFloat(min(1, max(0, cx.props.num("focalY") ?? 0.5)))
        let w = cx.size.width, h = cx.size.height
        Group {
            if let request {
                MediaImage(request: request) { image in
                    Self.fitted(image, fit: fit)
                        .alignmentGuide(HorizontalAlignment.leading) { d in fx * (d.width - w) }
                        .alignmentGuide(VerticalAlignment.top) { d in fy * (d.height - h) }
                        .frame(width: w, height: h, alignment: .topLeading)
                } placeholder: {
                    ImagePlaceholder(ink: muted, label: alt, icon: fallback, model: cx.model)
                }
            } else {
                ImagePlaceholder(ink: muted, label: alt, icon: fallback, model: cx.model)
            }
        }
        .frame(width: w, height: h)
        .clipped()
    }

    /// The image sized by `fit` (its own frame, before the focal placement).
    @ViewBuilder
    static func fitted(_ image: Image, fit: String) -> some View {
        switch fit {
        case "contain": image.resizable().scaledToFit()
        case "fill": image.resizable()
        case "none": image.fixedSize()
        case "scaleDown": image.resizable().scaledToFit()
        default: image.resizable().scaledToFill()
        }
    }
}

/// `m:ss` / `h:mm:ss`.
func formatDuration(_ ms: Double) -> String {
    let total = Int(max(0, ms / 1000).rounded())
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60)
    return h > 0 ? String(format: "%d:%02d:%02d", h, m, s) : String(format: "%d:%02d", m, s)
}

/// `Video`: `src` plays through the policed media request (AVKit's player
/// with the platform's controls, `autoplay` = muted on appear); before that
/// the poster (or a dark tint), a play button and the duration. A denied
/// src stays that poster with an inert play glyph (React's sourceless
/// `<video>`).
struct VideoLeaf: View {
    let cx: LeafContext
    @State private var playback = MediaPlayback()

    var body: some View {
        let src = cx.props.str("src")
        let request = src.isEmpty ? nil : cx.model.mediaRequest(src)
        let key = request.map(MediaLoader.key)
        let autoplay = cx.props.flag("autoplay")
        let poster = cx.props.str("poster")
        let posterRequest = poster.isEmpty ? nil : cx.model.mediaRequest(poster)
        ZStack {
            Color.black.opacity(0.85)
            if let player = playback.player {
                VideoPlayer(player: player)
            } else {
                if let posterRequest {
                    MediaImage(request: posterRequest) { image in
                        image.resizable().scaledToFill()
                    } placeholder: {
                        Color.clear
                    }
                }
                Button {
                    Task { await playback.play(request) }
                } label: {
                    Circle().fill(Color.white.opacity(0.18)).frame(width: 44, height: 44).overlay(ConceptIcon(name: "ui-play", size: 20, color: .white, model: cx.model))
                }
                .buttonStyle(.plain)
                .disabled(request == nil)
                if let ms = cx.props.num("durationMs") {
                    Text(formatDuration(ms))
                        .font(.system(size: 12)).foregroundStyle(.white)
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(Color.black.opacity(0.6), in: RoundedRectangle(cornerRadius: 4))
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                        .padding(.trailing, 8).padding(.bottom, 6)
                }
            }
        }
        .frame(width: cx.size.width, height: cx.size.height)
        .clipped()
        .task(id: key) {
            if autoplay, let request {
                await playback.play(request, muted: true)
            } else if playback.openedKey != key {
                playback.stop()
            }
        }
        .onDisappear { playback.stop() }
    }
}

/// `AudioPlayer`: the title line over a controls bar. `src` plays through
/// the policed media request: play / pause, the track fills with the
/// position (a press seeks), `elapsed / length` (the item's, else
/// `durationMs`). A denied src keeps the controls inert.
struct AudioLeaf: View {
    let cx: LeafContext
    @State private var playback = MediaPlayback()

    var body: some View {
        let title = cx.props.str("title")
        let src = cx.props.str("src")
        let request = src.isEmpty ? nil : cx.model.mediaRequest(src)
        let key = request.map(MediaLoader.key)
        let muted = cx.themeColor("muted") ?? cx.ink.opacity(0.1)
        let mutedFg = cx.themeColor("mutedForeground") ?? cx.ink.opacity(0.6)
        let track = cx.part("AudioPlayer", "track")
        let known = cx.props.num("durationMs").map { $0 / 1000 }
        let length = playback.duration ?? known
        let progress = length.map { $0 > 0 ? min(1, playback.time / $0) : 0 } ?? 0
        VStack(alignment: .leading, spacing: cx.spacing("xs")) {
            if !title.isEmpty {
                Text(title).font(cx.font).foregroundStyle(track.color ?? cx.ink).lineLimit(1)
            }
            HStack(spacing: 8) {
                Button {
                    if playback.playing { playback.pause() } else { Task { await playback.play(request) } }
                } label: {
                    Circle().fill(cx.ink).frame(width: 28, height: 28).overlay(ConceptIcon(name: playback.playing ? "pause" : "ui-play", size: 14, color: cx.themeColor("background") ?? .white, model: cx.model))
                }
                .buttonStyle(.plain)
                .disabled(request == nil)
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        Capsule().fill(mutedFg.opacity(0.35))
                        Capsule().fill(mutedFg).frame(width: geo.size.width * progress)
                    }
                    .frame(height: 4)
                    .frame(maxHeight: .infinity)
                    .contentShape(Rectangle())
                    .gesture(DragGesture(minimumDistance: 0).onEnded { g in
                        playback.seek(fraction: geo.size.width > 0 ? g.location.x / geo.size.width : 0, fallbackDuration: known)
                    })
                }
                Text("\(formatDuration(playback.time * 1000)) / \(length.map { formatDuration($0 * 1000) } ?? "0:00")").font(.system(size: 12)).foregroundStyle(mutedFg)
            }
            .padding(.horizontal, 8)
            .frame(height: cx.control("row", SurfaceMeasurer.audioControlsHeight))
            .background(muted, in: Capsule())
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
        .task(id: key) {
            if playback.openedKey != key { playback.stop() }
        }
        .onDisappear { playback.stop() }
    }
}

/// `Ring`: track circle + value arc from 12 o'clock, the label centred.
struct RingLeaf: View {
    let cx: LeafContext

    var body: some View {
        let value = min(1, max(0, cx.props.num("value") ?? 0))
        let track = cx.part("Ring", "track")
        let fill = cx.part("Ring", "fill")
        let stroke = track.px("borderWidth") ?? 2
        let fillStroke = fill.px("borderWidth") ?? stroke
        let trackColor = track.color ?? cx.ink.opacity(0.15)
        let fillColor = fill.color ?? cx.tone(cx.props.str("tone")) ?? cx.ink
        let label = cx.part("Ring", "label")
        let size = min(cx.size.width, cx.size.height)
        ZStack {
            RingView(value: value, lineWidth: stroke, fillWidth: fillStroke, track: trackColor, fill: fillColor).frame(width: size, height: size)
            if !cx.props.str("label").isEmpty {
                Text(cx.props.str("label")).font(.system(size: label.px("fontSize") ?? 12)).foregroundStyle(label.color ?? cx.ink).lineLimit(1).fixedSize()
            }
        }
        .frame(width: cx.size.width, height: cx.size.height)
    }
}

/// `TreeGuides` (round 3, `layout.json` `treeGuideColumn` /
/// `treeGuideRadius` / `treeGuideBridge`): column `i`'s line has its LEFT
/// edge at `i·14 + 7`; the elbow = a vertical from `top − bridge` to the
/// row's centre, a 3 px ROUNDED corner, a stub to the column's right edge
/// (`i·14 + 14`); `tee` continues the vertical to the bottom; pass-through
/// columns run full height from `top − bridge`. The bridge is paint only:
/// the canvas overshoots the part's top so a Section's hairline divider
/// never breaks the line. Mirrored under RTL (gpui `tree_guides`).
struct TreeGuidesLeaf: View {
    let cx: LeafContext

    var body: some View {
        let depth = Int(max(cx.props.num("depth") ?? 0, 0))
        // The core fills `elbowAt`; a row it never saw (a lone root, a template item) draws no elbow.
        let elbowAt = cx.props.num("elbowAt").map { Int($0) } ?? -1
        let tee = cx.props.flag("tee")
        let pass = cx.props.list("passThrough").compactMap { $0.number.map { Int($0) } }
        let line = cx.part("TreeGuides", "line")
        let color = line.color ?? cx.ink.opacity(0.25)
        let lw = line.px("width") ?? 1
        let rtl = cx.rtl
        let geo = TreeGuideGeometry.self
        let bridge = geo.bridge
        let w = cx.size.width
        let h = cx.size.height
        Canvas { ctx, _ in
            for seg in geo.segments(depth: depth, elbowAt: elbowAt, tee: tee, passThrough: pass, height: h, lineWidth: lw) {
                var path = Path()
                switch seg {
                case let .vertical(x, from, to):
                    let px = rtl ? w - x - lw : x
                    path.addRect(CGRect(x: px, y: from + bridge, width: lw, height: to - from))
                    ctx.fill(path, with: .color(color))
                case let .elbow(x, mid, stubEnd):
                    // The corner and the stub: ONE stroked path, so the
                    // quarter-turn is round rather than mitred.
                    let dir: CGFloat = rtl ? -1 : 1
                    let cxl = (rtl ? w - x - lw : x) + lw / 2
                    let end = rtl ? w - stubEnd : stubEnd
                    let r = geo.radius
                    path.move(to: CGPoint(x: cxl, y: mid - r + bridge))
                    path.addQuadCurve(to: CGPoint(x: cxl + dir * r, y: mid + bridge), control: CGPoint(x: cxl, y: mid + bridge))
                    path.addLine(to: CGPoint(x: end, y: mid + bridge))
                    ctx.stroke(path, with: .color(color), lineWidth: lw)
                }
            }
        }
        // Taller by the bridge, bottom-aligned: the canvas starts `bridge`
        // px above the part (a frame does not clip).
        .frame(width: w, height: h + bridge)
        .frame(width: w, height: h, alignment: .bottom)
    }
}

/// The tree-guide geometry in the part's own coordinates (y = 0 is the
/// part's top; the bridge makes verticals start ABOVE it), shared by the
/// painter and the geometry test.
enum TreeGuideGeometry {
    static let column = SurfaceMeasurer.treeGuideColumn
    static let radius = SurfaceMeasurer.treeGuideRadius
    static let bridge = SurfaceMeasurer.treeGuideBridge

    enum Segment: Equatable {
        /// A `lineWidth`-wide vertical whose LEFT edge is at `x`.
        case vertical(x: CGFloat, from: CGFloat, to: CGFloat)
        /// The rounded corner from `(x, mid − radius)` into the stub that
        /// runs at `mid` to `stubEnd`.
        case elbow(x: CGFloat, mid: CGFloat, stubEnd: CGFloat)
    }

    static func segments(depth: Int, elbowAt: Int, tee: Bool, passThrough: [Int], height h: CGFloat, lineWidth: CGFloat = 1) -> [Segment] {
        var out: [Segment] = []
        let top = -bridge
        let mid = h / 2
        for i in 0..<max(depth, 0) {
            let x = CGFloat(i) * column + column / 2
            if passThrough.contains(i) {
                out.append(.vertical(x: x, from: top, to: h))
            }
            if i == elbowAt {
                let to = tee ? h : mid - radius
                if to > top { out.append(.vertical(x: x, from: top, to: to)) }
                out.append(.elbow(x: x, mid: mid, stubEnd: CGFloat(i + 1) * column))
            }
        }
        return out
    }
}

/// The Carousel dot strip, centred on the core's single-dot leaf.
struct CarouselIndicatorLeaf: View {
    let cx: LeafContext

    var body: some View {
        let count = Int(max(cx.props.num("count") ?? 0, 0))
        let page = Int(cx.props.num("page") ?? 0)
        let dot = cx.part("Carousel", "indicator")
        let size = dot.width ?? 8
        let gap = cx.spacing("xs")
        HStack(spacing: gap) {
            ForEach(0..<max(count, 0), id: \.self) { i in
                Button {
                    cx.model.carouselPage(cx.index, page: i)
                } label: {
                    Circle().fill(i == page ? (dot.style.background ?? cx.ink) : (dot.style.background ?? cx.ink).opacity(0.35)).frame(width: size, height: size)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(cx.model.builtinString("pageOf", ["page": .number(Double(i + 1)), "total": .number(Double(count))]))
                .accessibilityAddTraits(i == page ? [.isButton, .isSelected] : [.isButton])
            }
        }
        .frame(width: cx.size.width, height: cx.size.height)
    }
}
