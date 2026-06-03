import SwiftUI

extension Color {
    /// Best-effort `#rrggbb` parsing for project/label accent colors. Accepts an
    /// optional string (project/label colors can be nil) and tolerates a leading
    /// `#` plus surrounding whitespace; returns nil for anything that isn't a
    /// 6-digit hex so callers can fall back (`Color(hex:) ?? .gray`).
    public init?(hex: String?) {
        guard let hex else { return nil }
        let cleaned = hex
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "#", with: "")
        guard cleaned.count == 6, let rgb = UInt64(cleaned, radix: 16) else { return nil }
        self.init(
            red: Double((rgb >> 16) & 0xFF) / 255.0,
            green: Double((rgb >> 8) & 0xFF) / 255.0,
            blue: Double(rgb & 0xFF) / 255.0
        )
    }
}
