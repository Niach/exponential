import SwiftUI

/// VAPP-162: Lucide's `phone-off` (`call-end`, the call's hang-up glyph),
/// the receiver crossed by a slash, as web, desktop and Android draw it
/// (EXP-1555: Lucide geometry, no SF Symbol).
public struct PhoneOffMark: View {
    let size: CGFloat
    let weight: Font.Weight

    public init(size: CGFloat, weight: Font.Weight = .medium) {
        self.size = size
        self.weight = weight
    }

    static let paths: [String] = ["M 10.1 13.9 C 11.189 14.988 12.45 15.89 13.832 16.568 C 14.257 16.763 14.761 16.637 15.045 16.265 L 15.4 15.8 C 15.778 15.296 16.37 15 17 15 L 20 15 C 21.105 15 22 15.895 22 17 L 22 20 C 22 21.105 21.105 22 20 22 C 15.226 22 10.648 20.104 7.272 16.728", "M 22 2 L 2 22", "M 4.76 13.582 C 2.956 10.712 1.999 7.39 2 4 C 2 2.895 2.895 2 4 2 L 7 2 C 8.105 2 9 2.895 9 4 L 9 7 C 9 7.63 8.704 8.222 8.2 8.6 L 7.732 8.951 C 7.353 9.24 7.231 9.756 7.44 10.184 C 7.518 10.343 7.6 10.501 7.684 10.657"]

    public var body: some View {
        LucideGlyph(paths: Self.paths, size: size)
    }
}
