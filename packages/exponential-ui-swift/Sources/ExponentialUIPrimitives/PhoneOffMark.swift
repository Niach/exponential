import SwiftUI

/// VAPP-162: Lucide's `phone-off` (`call-end`, the call's hang-up glyph):
/// the receiver crossed by a slash, as desktop and Android draw it. SF
/// Symbols has no slashed phone (`phone.down` is a different glyph), so the
/// receiver is the `phone` symbol and the slash a stroked line on top,
/// bottom-left to top-right like Lucide's.
public struct PhoneOffMark: View {
    let size: CGFloat
    let weight: Font.Weight

    public init(size: CGFloat, weight: Font.Weight = .medium) {
        self.size = size
        self.weight = weight
    }

    public var body: some View {
        ZStack {
            Image(systemName: "phone").font(.system(size: size * 0.7, weight: weight))
            Path { p in
                p.move(to: CGPoint(x: size * 0.14, y: size * 0.86))
                p.addLine(to: CGPoint(x: size * 0.86, y: size * 0.14))
            }
            .stroke(style: StrokeStyle(lineWidth: max(1.5, size * 0.085), lineCap: .round))
        }
        .frame(width: size, height: size)
    }
}
