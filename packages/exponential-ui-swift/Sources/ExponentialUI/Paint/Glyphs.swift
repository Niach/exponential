import SwiftUI

/// Chrome glyphs the controls need (check marks, chevrons, close, search,
/// calendar, send, attach, loader, play, image): SF Symbols, so a host with
/// no icon registry still gets working controls. Catalog icon NAMES go
/// through `HostPlugin.icon`.
public enum Glyph: String, Sendable {
    case check = "checkmark"
    case chevronDown = "chevron.down"
    case chevronUp = "chevron.up"
    case chevronRight = "chevron.right"
    case chevronLeft = "chevron.left"
    case chevronsUpDown = "chevron.up.chevron.down"
    case close = "xmark"
    case search = "magnifyingglass"
    case calendar = "calendar"
    case send = "arrow.up"
    case attach = "paperclip"
    case loader = "circle.dotted"
    case play = "play.fill"
    case image = "photo"
    case stop = "stop.fill"
    case circle = "circle"
    case externalLink = "arrow.up.right"
}

struct GlyphView: View {
    let glyph: Glyph
    let size: CGFloat
    let color: Color
    var weight: Font.Weight = .medium

    var body: some View {
        Image(systemName: glyph.rawValue)
            .font(.system(size: size * 0.8, weight: weight))
            .foregroundStyle(color)
            .frame(width: size, height: size)
    }
}

/// A catalog icon by NAME through the host; the placeholder circle when
/// the host has none.
struct IconView: View {
    let name: String
    let size: CGFloat
    let color: Color
    let model: SurfaceModel

    var body: some View {
        if let view = model.host.icon(name, size: size) {
            view.frame(width: size, height: size).foregroundStyle(color)
        } else {
            Circle()
                .strokeBorder(color.opacity(0.5), lineWidth: 1.5)
                .frame(width: size * 0.8, height: size * 0.8)
                .frame(width: size, height: size)
        }
    }
}

/// The rotating loader.
struct SpinnerView: View {
    let size: CGFloat
    let color: Color
    @State private var spinning = false

    var body: some View {
        Circle()
            .trim(from: 0.15, to: 1)
            .stroke(color, style: StrokeStyle(lineWidth: max(1.5, size / 10), lineCap: .round))
            .frame(width: size - 2, height: size - 2)
            .rotationEffect(.degrees(spinning ? 360 : 0))
            .animation(.linear(duration: 0.8).repeatForever(autoreverses: false), value: spinning)
            .onAppear { spinning = true }
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}
