import SwiftUI
import ExponentialUIPrimitives

/// `Chart` (contract §3): bar / stackedBar / line / area / pie / donut /
/// sparkline drawn in a `Canvas` from the numbers the CORE put in
/// `props.chart` (`chartExtent`, `niceTicks`, the series colour tokens, the
/// donut hole), so the axes are every painter's: grid lines and y labels at
/// the ticks, category labels, optional value labels, the axis titles, a
/// legend (2+ series or slices, never on a sparkline) and a tooltip for the
/// category or slice under the pointer (hover on macOS / iPadOS, a tap on
/// touch, the arrows: `SurfaceModel.chartHover`). The plot fills the leaf (whose height the core fixed) minus the
/// title and legend rows. Geometry is gpui `paint/chart.rs`.
struct ChartLeaf: View {
    let cx: LeafContext

    var body: some View {
        let hovered = cx.model.chartHover[cx.index]
        let chart = ChartModel(cx.props)
        let gap = cx.spacing("xs")
        let legendProps = cx.part("Chart", "legend")
        let legendLH = legendProps.px("lineHeight") ?? 16
        let names = ChartModel.legend(cx.props)
        let count = chart.isSlices ? chart.sliceValues.count : chart.series.count
        let colors = (0..<max(count, 1)).map { color(chart, $0) }
        let axis = cx.part("Chart", "axis")
        let axisColor = axis.color ?? cx.muted
        let axisSize = min(axis.px("fontSize") ?? 12, 12)
        let titleH: CGFloat = chart.title.isEmpty ? 0 : cx.textStyle.lineHeight + gap
        let legendH: CGFloat = names.isEmpty ? 0 : legendLH + gap
        let plotH = max(cx.inner.height - titleH - legendH, 1)
        let geo = ChartGeometry(chart: chart, width: cx.inner.width, height: plotH, axisSize: axisSize)
        VStack(alignment: .leading, spacing: gap) {
            if !chart.title.isEmpty {
                let title = cx.part("Chart", "title")
                Text(chart.title)
                    .font(cx.font.weight(ExponentialUIFonts.swiftUIWeight(Int(title.props.num("fontWeight") ?? 600))))
                    .foregroundStyle(cx.ink).lineLimit(1)
                    .frame(height: cx.textStyle.lineHeight)
            }
            ZStack(alignment: .topLeading) {
                ChartCanvas(geo: geo, colors: colors, grid: cx.part("Chart", "grid").color ?? cx.themeColor("border") ?? cx.ink.opacity(0.15), band: cx.ink.opacity(0.06), hovered: hovered)
                if geo.chart.showAxes { axes(geo, color: axisColor, size: axisSize) }
                if geo.chart.showValues, !geo.chart.isSlices, geo.chart.kind != "sparkline" { values(geo, color: cx.part("Chart", "valueLabel").color ?? cx.ink, size: axisSize) }
                if let h = hovered { tooltip(geo, h, colors: colors) }
            }
            .frame(width: cx.inner.width, height: plotH)
            .contentShape(Rectangle())
            .onContinuousHover { phase in
                switch phase {
                case .active(let p): cx.model.setChartHover(cx.index, geo.hit(p))
                case .ended: cx.model.setChartHover(cx.index, nil)
                }
            }
            .onTapGesture(coordinateSpace: .local) { p in
                let hit = geo.hit(p)
                cx.model.setChartHover(cx.index, hit == hovered ? nil : hit)
            }
            if !names.isEmpty {
                HStack(spacing: cx.spacing("sm")) {
                    ForEach(Array(names.enumerated()), id: \.offset) { i, name in
                        HStack(spacing: 4) {
                            RoundedRectangle(cornerRadius: 2).fill(colors[safe: i] ?? axisColor).frame(width: 8, height: 8)
                            Text(name).font(.system(size: axisSize)).foregroundStyle(legendProps.color ?? axisColor).lineLimit(1)
                        }
                    }
                }
                .frame(height: legendLH)
            }
        }
        .frame(width: cx.inner.width, height: cx.inner.height, alignment: .topLeading)
        .offset(x: cx.inner.minX, y: cx.inner.minY)
    }

    /// The colour of series / slice `i`: the core's token, else chart1..8.
    private func color(_ chart: ChartModel, _ i: Int) -> Color {
        cx.themeColor(chart.colorName(i)) ?? cx.themeColor("chart\(i % 8 + 1)") ?? RGBA(hue: Double((i * 67) % 360), saturation: 0.6, lightness: 0.55).color
    }

    @ViewBuilder
    private func axes(_ g: ChartGeometry, color: Color, size: CGFloat) -> some View {
        ForEach(Array(g.chart.ticks.enumerated()), id: \.offset) { _, t in
            Text(ChartModel.format(t)).font(.system(size: size)).foregroundStyle(color).lineLimit(1)
                .frame(width: max(g.left - 6, 0), height: 14, alignment: .trailing)
                .offset(x: 0, y: g.y(t) - 7)
        }
        ForEach(Array(g.chart.categories.enumerated()), id: \.offset) { i, c in
            Text(c).font(.system(size: size)).foregroundStyle(color).lineLimit(1)
                .frame(width: g.slot, height: 14)
                .offset(x: g.x(i) - g.slot / 2, y: g.top + g.innerH + 4)
        }
        if !g.chart.xLabel.isEmpty {
            Text(g.chart.xLabel).font(.system(size: size)).foregroundStyle(color).lineLimit(1)
                .frame(width: g.innerW, height: 14)
                .offset(x: g.left, y: g.top + g.innerH + 20)
        }
        if !g.chart.yLabel.isEmpty {
            Text(g.chart.yLabel).font(.system(size: size)).foregroundStyle(color).lineLimit(1).fixedSize()
        }
    }

    @ViewBuilder
    private func values(_ g: ChartGeometry, color: Color, size: CGFloat) -> some View {
        ForEach(Array(g.chart.series.enumerated()), id: \.offset) { si, s in
            ForEach(Array(s.values.enumerated()), id: \.offset) { i, v in
                Text(ChartModel.format(v)).font(.system(size: size)).foregroundStyle(color).lineLimit(1)
                    .frame(width: 40, height: 14)
                    .offset(x: (g.chart.kind == "bar" ? g.barX(si, i) + g.barWidth / 2 : g.x(i)) - 20, y: g.y(v) - 15)
            }
        }
    }

    @ViewBuilder
    private func tooltip(_ g: ChartGeometry, _ h: Int, colors: [Color]) -> some View {
        let tip = cx.part("Chart", "tooltip")
        let fs = tip.px("fontSize") ?? 12
        let pad = tip.px("padding") ?? 8
        let fg = tip.color ?? cx.ink
        let label = g.chart.categories[safe: h] ?? ""
        let lines: [(String, Color, String)] = g.chart.isSlices
            ? [(label, colors[safe: h] ?? fg, g.chart.sliceValues[safe: h].map(ChartModel.format) ?? "")]
            : g.chart.series.enumerated().map { i, s in (s.name, colors[safe: i] ?? fg, s.values[safe: h].map(ChartModel.format) ?? "") }
        VStack(alignment: .leading, spacing: 2) {
            if !g.chart.isSlices, !label.isEmpty {
                Text(label).font(.system(size: fs, weight: .semibold))
            }
            ForEach(Array(lines.enumerated()), id: \.offset) { _, l in
                HStack(spacing: 6) {
                    RoundedRectangle(cornerRadius: 2).fill(l.1).frame(width: 8, height: 8)
                    Text(l.0.isEmpty ? l.2 : "\(l.0): \(l.2)").font(.system(size: fs))
                }
            }
        }
        .foregroundStyle(fg)
        .padding(pad)
        .fixedSize()
        .background(tip.style.background ?? cx.themeColor("popover") ?? .black, in: RoundedRectangle(cornerRadius: tip.style.radius))
        .offset(x: min((g.chart.isSlices ? g.width / 2 : g.x(h)) + 8, max(g.width - 120, 0)), y: g.top)
        .allowsHitTesting(false)
    }
}

/// The plot geometry gpui `chart::paint` uses: gutters for the axes, the
/// value → y and category → x maps, the bar slots, the hit test.
struct ChartGeometry {
    let chart: ChartModel
    let width: CGFloat
    let height: CGFloat
    let left: CGFloat
    let right: CGFloat
    let top: CGFloat
    let bottom: CGFloat

    init(chart: ChartModel, width: CGFloat, height: CGFloat, axisSize: CGFloat) {
        self.chart = chart
        self.width = width
        self.height = height
        let spark = chart.kind == "sparkline"
        let labelW: CGFloat = chart.showAxes ? CGFloat(chart.ticks.map { ChartModel.format($0).count }.max() ?? 1) * axisSize * 0.62 + 8 : 0
        if spark {
            (left, right, top) = (1, 1, 2)
        } else {
            (left, right, top) = (max(labelW, 4), 8, chart.showValues || !chart.yLabel.isEmpty ? 16 : 8)
        }
        bottom = spark || chart.isSlices ? 2 : (chart.showAxes ? 20 + (chart.xLabel.isEmpty ? 0 : 14) : 8)
    }

    var innerW: CGFloat { max(width - left - right, 1) }
    var innerH: CGFloat { max(height - top - bottom, 1) }
    var span: Double { max(chart.max - chart.min, .ulpOfOne) }
    var n: Int { chart.categoryCount }
    var slot: CGFloat { innerW / CGFloat(n) }
    func x(_ i: Int) -> CGFloat { left + innerW * (CGFloat(i) + 0.5) / CGFloat(n) }
    func y(_ v: Double) -> CGFloat { top + innerH - innerH * CGFloat((v - chart.min) / span) }
    var barWidth: CGFloat { slot * 0.7 / CGFloat(max(chart.series.count, 1)) }
    func barX(_ si: Int, _ i: Int) -> CGFloat { left + slot * CGFloat(i) + slot * 0.15 + barWidth * CGFloat(si) }
    var radius: CGFloat { min(innerW, innerH) / 2 }

    /// The category (slice) under a point of the plot.
    func hit(_ p: CGPoint) -> Int? {
        switch chart.kind {
        case "pie", "donut": ChartModel.slice(dx: p.x - width / 2, dy: p.y - height / 2, radius: radius, hole: CGFloat(chart.hole), values: chart.sliceValues)
        case "sparkline": nil
        default: ChartModel.category(at: p.x - left, width: innerW, count: n)
        }
    }
}

private struct ChartCanvas: View {
    let geo: ChartGeometry
    let colors: [Color]
    let grid: Color
    let band: Color
    let hovered: Int?

    var body: some View {
        Canvas { ctx, _ in
            let g = geo
            let chart = g.chart
            let color: (Int) -> Color = { colors[safe: $0] ?? grid }
            if chart.showGrid {
                for t in chart.ticks {
                    ctx.fill(Path(CGRect(x: g.left, y: g.y(t).rounded(.down), width: g.innerW, height: 1)), with: .color(grid))
                }
            }
            if let h = hovered, !chart.isSlices, chart.kind != "sparkline" {
                ctx.fill(Path(CGRect(x: g.left + g.slot * CGFloat(h), y: g.top, width: g.slot, height: g.innerH)), with: .color(band))
            }
            switch chart.kind {
            case "pie", "donut":
                let values = chart.sliceValues
                let total = max(values.reduce(0, +), .ulpOfOne)
                let c = CGPoint(x: g.width / 2, y: g.height / 2)
                let hole = CGFloat(chart.hole) * g.radius
                var start = -Double.pi / 2
                for (i, v) in values.enumerated() {
                    let sweep = v / total * 2 * .pi
                    let r = g.radius + (hovered == i ? 3 : 0)
                    var p = Path()
                    p.addArc(center: c, radius: r, startAngle: .radians(start), endAngle: .radians(start + sweep), clockwise: false)
                    if hole > 0 {
                        p.addArc(center: c, radius: hole, startAngle: .radians(start + sweep), endAngle: .radians(start), clockwise: true)
                    } else {
                        p.addLine(to: c)
                    }
                    p.closeSubpath()
                    ctx.fill(p, with: .color(color(i)))
                    start += sweep
                }
            case "line", "area", "sparkline":
                for (si, s) in chart.series.enumerated() where !s.values.isEmpty {
                    let pts = s.values.enumerated().map { CGPoint(x: g.x($0.offset), y: g.y($0.element)) }
                    if chart.kind == "area", let first = pts.first, let last = pts.last {
                        let base = g.y(min(max(chart.min, 0), chart.max))
                        var area = Path()
                        area.move(to: CGPoint(x: first.x, y: base))
                        for p in pts { area.addLine(to: p) }
                        area.addLine(to: CGPoint(x: last.x, y: base))
                        area.closeSubpath()
                        ctx.fill(area, with: .color(color(si).opacity(0.2)))
                    }
                    if pts.count > 1 {
                        var line = Path()
                        line.addLines(pts)
                        ctx.stroke(line, with: .color(color(si)), lineWidth: chart.kind == "sparkline" ? 1.5 : 2)
                    } else if let p = pts.first {
                        ctx.fill(Path(ellipseIn: CGRect(x: p.x - 2, y: p.y - 2, width: 4, height: 4)), with: .color(color(si)))
                    }
                    if let h = hovered, chart.kind != "sparkline", let p = pts[safe: h] {
                        ctx.fill(Path(ellipseIn: CGRect(x: p.x - 3.5, y: p.y - 3.5, width: 7, height: 7)), with: .color(color(si)))
                    }
                }
            case "stackedBar":
                let bw = g.slot * 0.6
                for i in 0..<g.n {
                    var acc = 0.0
                    for (si, s) in chart.series.enumerated() {
                        let v = max(s.values[safe: i] ?? 0, 0)
                        let (y0, y1) = (g.y(acc), g.y(acc + v))
                        acc += v
                        ctx.fill(Path(CGRect(x: g.left + g.slot * CGFloat(i) + g.slot * 0.2, y: y1, width: bw, height: max(y0 - y1, 0))), with: .color(color(si)))
                    }
                }
            default:
                let zero = g.y(min(max(0, chart.min), chart.max))
                for (si, s) in chart.series.enumerated() {
                    for (i, v) in s.values.enumerated() {
                        let y = g.y(v)
                        let rect = y <= zero ? CGRect(x: g.barX(si, i), y: y, width: g.barWidth, height: zero - y) : CGRect(x: g.barX(si, i), y: zero, width: g.barWidth, height: y - zero)
                        ctx.fill(Path(roundedRect: rect, cornerRadius: 2), with: .color(color(si)))
                    }
                }
            }
        }
        .frame(width: geo.width, height: geo.height)
        .accessibilityHidden(true)
    }
}
