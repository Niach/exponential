import ExpUI
import ExpCore
import SwiftUI

// Shared release display helpers (EXP-56) — consumed by the releases list,
// the release detail, and the add-issues sheet.

/// "Shipped <date>" (emerald) when shipped_at is set; "Ready" (outline
/// emerald) when all non-dropped issues are done and the release is unshipped;
/// nothing otherwise. Mirrors the web's ReleaseStatePill.
struct ReleaseStatePill: View {
    let release: ReleaseEntity
    let isComplete: Bool

    var body: some View {
        if release.shippedAt != nil {
            pill(text: shippedText, filled: true)
        } else if isComplete {
            pill(text: "Ready", filled: false)
        }
    }

    private var shippedText: String {
        if let date = parseTimestamp(release.shippedAt) {
            return "Shipped \(AppDateFormatters.MMMd.string(from: date))"
        }
        return "Shipped"
    }

    @ViewBuilder
    private func pill(text: String, filled: Bool) -> some View {
        Text(text)
            .font(.caption2.weight(.medium))
            .foregroundStyle(DesignTokens.Semantic.green)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(
                Capsule().fill(filled ? DesignTokens.Semantic.green.opacity(0.12) : .clear)
            )
            .overlay(
                Capsule().strokeBorder(DesignTokens.Semantic.green.opacity(0.4), lineWidth: 1)
            )
    }
}

/// "N of M done" — denominator excludes cancelled + duplicate (§10.2).
func progressText(_ progress: ReleaseProgress) -> String {
    progress.total == 0
        ? "No issues"
        : "\(progress.done) of \(progress.denominator) done"
}

func formatReleaseTargetDate(_ dateString: String) -> String {
    guard let date = AppDateFormatters.yyyyMMdd.date(from: dateString) else { return dateString }
    return AppDateFormatters.MMMd.string(from: date)
}

/// Parse a synced ISO-8601 timestamp string (with or without fractional
/// seconds).
func parseTimestamp(_ s: String?) -> Date? {
    guard let s else { return nil }
    let withFractional = ISO8601DateFormatter()
    withFractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = withFractional.date(from: s) { return date }
    return ISO8601DateFormatter().date(from: s)
}
