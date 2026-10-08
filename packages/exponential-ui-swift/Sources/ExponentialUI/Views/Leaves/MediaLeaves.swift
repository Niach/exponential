import SwiftUI
import ExponentialUIPrimitives

struct IconLeaf: View {
    let cx: LeafContext

    var body: some View {
        let name = cx.props.str("name").isEmpty ? "ui-icon-placeholder" : cx.props.str("name")
        let color = cx.props.str("tone").isEmpty ? cx.ink : (cx.tone(cx.props.str("tone")) ?? cx.ink)
        IconView(name: name, size: max(1, min(cx.inner.width, cx.inner.height)), color: color, model: cx.model)
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
        let request = src.isEmpty ? nil : cx.model.host.mediaRequest(src)
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

/// The tinted placeholder an `Image` paints without a loadable source.
struct ImagePlaceholder: View {
    let ink: Color
    let label: String

    var body: some View {
        VStack(spacing: 4) {
            GlyphView(glyph: .image, size: 20, color: ink.opacity(0.6))
            Text(label).font(.system(size: 12)).foregroundStyle(ink.opacity(0.6)).lineLimit(1)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(ink.opacity(0.08))
    }
}

struct ImageLeaf: View {
    let cx: LeafContext

    var body: some View {
        let src = cx.props.str("src")
        let alt = cx.props.str("alt").isEmpty ? "image" : cx.props.str("alt")
        let muted = cx.themeColor("mutedForeground") ?? cx.ink
        let request = src.isEmpty ? nil : cx.model.host.mediaRequest(src)
        let fit = cx.props.str("fit")
        Group {
            if let request {
                MediaImage(request: request) { image in
                    if fit == "contain" || fit == "scaleDown" {
                        image.resizable().scaledToFit()
                    } else if fit == "fill" {
                        image.resizable()
                    } else {
                        image.resizable().scaledToFill()
                    }
                } placeholder: {
                    ImagePlaceholder(ink: muted, label: alt)
                }
            } else {
                ImagePlaceholder(ink: muted, label: alt)
            }
        }
        .frame(width: cx.size.width, height: cx.size.height)
        .clipped()
    }
}

/// `m:ss` / `h:mm:ss`.
func formatDuration(_ ms: Double) -> String {
    let total = Int(max(0, ms / 1000).rounded())
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60)
    return h > 0 ? String(format: "%d:%02d:%02d", h, m, s) : String(format: "%d:%02d", m, s)
}

/// `Video`: the poster (or a dark tint) with a play button and the duration.
struct VideoLeaf: View {
    let cx: LeafContext

    var body: some View {
        let poster = cx.props.str("poster")
        let request = poster.isEmpty ? nil : cx.model.host.mediaRequest(poster)
        ZStack {
            Color.black.opacity(0.85)
            if let request {
                MediaImage(request: request) { image in
                    image.resizable().scaledToFill()
                } placeholder: {
                    Color.clear
                }
            }
            Circle().fill(Color.white.opacity(0.18)).frame(width: 44, height: 44).overlay(GlyphView(glyph: .play, size: 20, color: .white))
            if let ms = cx.props.num("durationMs") {
                Text(formatDuration(ms))
                    .font(.system(size: 12)).foregroundStyle(.white)
                    .padding(.horizontal, 6).padding(.vertical, 2)
                    .background(Color.black.opacity(0.6), in: RoundedRectangle(cornerRadius: 4))
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                    .padding(.trailing, 8).padding(.bottom, 6)
            }
        }
        .frame(width: cx.size.width, height: cx.size.height)
        .clipped()
    }
}

/// `AudioPlayer`: the title line over a controls bar.
struct AudioLeaf: View {
    let cx: LeafContext

    var body: some View {
        let title = cx.props.str("title")
        let muted = cx.themeColor("muted") ?? cx.ink.opacity(0.1)
        let mutedFg = cx.themeColor("mutedForeground") ?? cx.ink.opacity(0.6)
        let track = cx.part("AudioPlayer", "track")
        let duration = cx.props.num("durationMs").map(formatDuration) ?? "0:00"
        VStack(alignment: .leading, spacing: cx.spacing("xs")) {
            if !title.isEmpty {
                Text(title).font(cx.font).foregroundStyle(track.color ?? cx.ink).lineLimit(1)
            }
            HStack(spacing: 8) {
                Circle().fill(cx.ink).frame(width: 28, height: 28).overlay(GlyphView(glyph: .play, size: 14, color: cx.themeColor("background") ?? .white))
                Capsule().fill(mutedFg.opacity(0.35)).frame(height: 4)
                Text("0:00 / \(duration)").font(.system(size: 12)).foregroundStyle(mutedFg)
            }
            .padding(.horizontal, 8)
            .frame(height: 40)
            .background(muted, in: Capsule())
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
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

/// `Chart`: bars / lines / areas / a pie, a title and a legend.
struct ChartLeaf: View {
    let cx: LeafContext

    var body: some View {
        let chart = ChartModel(cx.props)
        let gap = cx.spacing("xs")
        let legend = ChartModel.legend(cx.props)
        let muted = cx.themeColor("mutedForeground") ?? cx.ink.opacity(0.6)
        let grid = cx.themeColor("border") ?? cx.ink.opacity(0.15)
        VStack(alignment: .leading, spacing: gap) {
            if !chart.title.isEmpty {
                Text(chart.title).font(cx.font).foregroundStyle(cx.ink).lineLimit(1).frame(height: cx.textStyle.lineHeight)
            }
            ChartCanvas(chart: chart, ink: cx.ink, grid: grid, colors: (0..<max(1, chart.series.count)).map { seriesColor($0, chart) })
                .frame(height: chart.height)
            if !legend.isEmpty {
                HStack(spacing: 12) {
                    ForEach(Array(legend.enumerated()), id: \.offset) { i, name in
                        HStack(spacing: 4) {
                            Circle().fill(seriesColor(i, chart)).frame(width: 8, height: 8)
                            Text(name).font(.system(size: 12)).foregroundStyle(muted).lineLimit(1)
                        }
                    }
                }
                .frame(height: cx.part("Chart", "legend").px("lineHeight") ?? 16)
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }

    /// The palette colour of series/slice `i` (tone wins, then chart1..5).
    private func seriesColor(_ i: Int, _ chart: ChartModel) -> Color {
        let tone = chart.kind == "pie" ? nil : chart.series[safe: i]?.tone
        if let tone, let c = cx.tone(tone) { return c }
        if let c = cx.themeColor("chart\(i % 5 + 1)") { return c }
        return RGBA(hue: AvatarFallback.seedHue(String(i)) * 7, saturation: 0.6, lightness: 0.55).color
    }
}

private struct ChartCanvas: View {
    let chart: ChartModel
    let ink: Color
    let grid: Color
    let colors: [Color]

    var body: some View {
        Canvas { ctx, size in
            let n = max(chart.categories.count, chart.series.map(\.values.count).max() ?? 0)
            let maxV = chart.maxValue
            if chart.kind == "pie" {
                let values = chart.series.first?.values ?? []
                let total = values.reduce(0, +)
                let r = min(size.width, size.height) / 2 - 2
                let c = CGPoint(x: size.width / 2, y: size.height / 2)
                var start = -Double.pi / 2
                for (i, v) in values.enumerated() where total > 0 {
                    let sweep = v / total * 2 * .pi
                    var p = Path()
                    p.move(to: c)
                    p.addArc(center: c, radius: r, startAngle: .radians(start), endAngle: .radians(start + sweep), clockwise: false)
                    p.closeSubpath()
                    ctx.fill(p, with: .color(colors[safe: i % max(colors.count, 1)] ?? ink))
                    start += sweep
                }
                return
            }
            // Grid: 4 lines.
            for i in 0...4 {
                let y = size.height * CGFloat(i) / 4
                ctx.stroke(Path { $0.move(to: CGPoint(x: 0, y: y)); $0.addLine(to: CGPoint(x: size.width, y: y)) }, with: .color(grid), lineWidth: 1)
            }
            guard n > 0 else { return }
            let slot = size.width / CGFloat(n)
            if chart.kind == "bar" {
                let bars = max(chart.series.count, 1)
                let bw = max(2, (slot * 0.6) / CGFloat(bars))
                for (si, s) in chart.series.enumerated() {
                    for (i, v) in s.values.enumerated() {
                        let h = size.height * CGFloat(v / maxV)
                        let x = slot * CGFloat(i) + slot * 0.2 + bw * CGFloat(si)
                        ctx.fill(Path(roundedRect: CGRect(x: x, y: size.height - h, width: bw - 1, height: h), cornerRadius: 2), with: .color(colors[safe: si] ?? ink))
                    }
                }
            } else {
                for (si, s) in chart.series.enumerated() {
                    var path = Path()
                    for (i, v) in s.values.enumerated() {
                        let pt = CGPoint(x: slot * (CGFloat(i) + 0.5), y: size.height - size.height * CGFloat(v / maxV))
                        if i == 0 { path.move(to: pt) } else { path.addLine(to: pt) }
                    }
                    let color = colors[safe: si] ?? ink
                    if chart.kind == "area", let last = s.values.indices.last {
                        var area = path
                        area.addLine(to: CGPoint(x: slot * (CGFloat(last) + 0.5), y: size.height))
                        area.addLine(to: CGPoint(x: slot * 0.5, y: size.height))
                        area.closeSubpath()
                        ctx.fill(area, with: .color(color.opacity(0.2)))
                    }
                    ctx.stroke(path, with: .color(color), lineWidth: 2)
                }
            }
        }
    }
}

/// `TreeGuides`: 16 px columns, 1 px lines at x = 7, the elbow at mid-height.
struct TreeGuidesLeaf: View {
    let cx: LeafContext

    var body: some View {
        let depth = Int(max(cx.props.num("depth") ?? 0, 0))
        let elbowAt = cx.props.num("elbowAt").map { Int($0) } ?? (depth - 1)
        let tee = cx.props.flag("tee")
        let pass = cx.props.list("passThrough").compactMap { $0.number.map { Int($0) } }
        let line = cx.part("TreeGuides", "line")
        let color = line.color ?? cx.ink.opacity(0.25)
        let lw = line.px("width") ?? 1
        let h = cx.size.height
        Canvas { ctx, _ in
            for i in 0..<depth {
                let x = CGFloat(i) * 16 + 7
                if pass.contains(i) {
                    ctx.fill(Path(CGRect(x: x, y: 0, width: lw, height: h)), with: .color(color))
                }
                if i == elbowAt {
                    ctx.fill(Path(CGRect(x: x, y: 0, width: lw, height: tee ? h : h / 2)), with: .color(color))
                    ctx.fill(Path(CGRect(x: x, y: (h / 2).rounded(.down), width: 16 - 7, height: lw)), with: .color(color))
                }
            }
        }
        .frame(width: cx.size.width, height: cx.size.height)
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
                .accessibilityLabel("Page \(i + 1)")
                .accessibilityAddTraits(i == page ? [.isButton, .isSelected] : [.isButton])
            }
        }
        .frame(width: cx.size.width, height: cx.size.height)
    }
}
