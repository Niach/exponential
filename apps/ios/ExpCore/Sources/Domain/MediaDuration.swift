import Foundation

/// EXP-824 — the duration chip on inline video/audio: `0:07`, `2:34`,
/// `1:02:03`. Byte-identical to web `formatDuration` (video-metadata.ts) and
/// its desktop/Android twins: whole seconds ROUNDED (7 250 ms is `0:07`),
/// hours only when there are any, minutes never zero-padded below an hour.
public enum MediaDuration {
    public static func format(ms: Int) -> String {
        let totalSeconds = max(0, Int((Double(ms) / 1000).rounded()))
        let hours = totalSeconds / 3600
        let minutes = (totalSeconds % 3600) / 60
        let seconds = totalSeconds % 60
        if hours > 0 {
            return "\(hours):\(two(minutes)):\(two(seconds))"
        }
        return "\(minutes):\(two(seconds))"
    }

    private static func two(_ value: Int) -> String {
        value < 10 ? "0\(value)" : "\(value)"
    }
}
