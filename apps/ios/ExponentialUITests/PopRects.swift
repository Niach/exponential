import XCTest

/// Pop-out rect sidecars for the App Store slide compositor (EXP-627).
///
/// The store compositor (`apps/marketing/scripts/store/*`) enlarges ONE detail
/// of each raw capture into a floating "pop-out" panel. Until now the crop was
/// hand-tuned per shot and silently drifted whenever the layout moved. This
/// dumps the real on-screen geometry instead: for every store shot, the union
/// of the frames of the accessibility identifiers listed in [identifiers],
/// padded by [insetFraction] of the window, normalised to 0..1 against the
/// window frame, written next to the PNG the same run produces.
///
/// Output: `<simulator>-pop-<shot>.json` in
/// `~/Library/Caches/tools.fastlane/exp-pop/` — a sibling of the host directory
/// `SnapshotHelper.swift` writes `<simulator>-<shot>.png` into, so fastlane's
/// own copy step carries both to `fastlane/screenshots-raw/`.
/// `bun run screenshots:pop-sidecars -- --platform ios` then merges them into
/// the form-keyed `pop-<shot>.json` the compositor reads.
///
/// Shape (one object per file, normalised 0..1, origin top-left):
/// ```json
/// { "shot": "02_issue-detail", "platform": "ios",
///   "device": "iPhone 17 Pro Max",
///   "x": 0.04, "y": 0.31, "w": 0.92, "h": 0.28 }
/// ```
///
/// A shot with no entry here — or whose identifiers are all absent from the
/// hierarchy — is skipped silently (one NSLog line, never a failure): a
/// missing sidecar just leaves the compositor on its hand-tuned default.
enum PopRects {

    /// shot → the accessibility identifiers whose union is the pop-out rect.
    /// Only the FIRST match of each identifier counts: `notification-row` and
    /// friends repeat down a list, and the union of a whole list is the whole
    /// screen, which is not a pop-out.
    ///
    /// The identifiers themselves live on the product views (`issue-description`,
    /// `coding-now-row`, `start-coding-agent-picker`, `agent-feed-question`,
    /// `pr-merge-bar`, `notification-row`, plus the pre-existing `issue-row-*`,
    /// `action-row`, `support-thread-row`) and are mirrored 1:1 as Android
    /// testTags — see `PopRects.kt`.
    static let identifiers: [String: [String]] = [
        "01_board": ["issue-row-APP-5"],
        "02_issue-detail": ["issue-description", "coding-now-row"],
        "03_start-coding": ["start-coding-agent-picker"],
        "04_steering": ["agent-feed-question"],
        "05_review": ["pr-merge-bar"],
        "06_actions": ["action-row"],
        "07_inbox": ["notification-row"],
        "08_support": ["support-thread-row"],
    ]

    /// Outward padding around the union, as a fraction of the window's width
    /// and height — the pop-out panel needs a little air around the element it
    /// magnifies.
    static let insetFraction: CGFloat = 0.015

    /// Measures and writes the sidecar for `shot`. Call it immediately BEFORE
    /// the matching `snapshot(...)`, while the screen is settled.
    @MainActor
    static func dump(_ shot: String, _ app: XCUIApplication) {
        // A shot the run was told to skip must not leave a sidecar behind: it
        // would describe a screen this run never photographed.
        guard ScreenshotShots.isWanted(shot) else { return }
        guard let wanted = identifiers[shot], !wanted.isEmpty else {
            NSLog("EXP-627 pop rect: no identifiers registered for \(shot) — skipping")
            return
        }

        let window = app.windows.firstMatch.frame
        guard window.width > 0, window.height > 0 else {
            NSLog("EXP-627 pop rect: no window frame for \(shot) — skipping")
            return
        }

        var union: CGRect?
        for identifier in wanted {
            let element = app.descendants(matching: .any)
                .matching(identifier: identifier).firstMatch
            guard element.exists else {
                NSLog("EXP-627 pop rect: \(shot) has no element \"\(identifier)\"")
                continue
            }
            let frame = element.frame
            guard frame.width > 0, frame.height > 0 else { continue }
            union = union.map { $0.union(frame) } ?? frame
        }
        guard var rect = union else {
            NSLog("EXP-627 pop rect: \(shot) matched nothing — skipping")
            return
        }

        rect = rect.insetBy(
            dx: -insetFraction * window.width,
            dy: -insetFraction * window.height
        ).intersection(window)
        guard !rect.isNull, rect.width > 0, rect.height > 0 else { return }

        let normalized = CGRect(
            x: (rect.minX - window.minX) / window.width,
            y: (rect.minY - window.minY) / window.height,
            width: rect.width / window.width,
            height: rect.height / window.height
        )
        write(shot: shot, rect: normalized)
    }

    @MainActor
    private static func write(shot: String, rect: CGRect) {
        // A SIBLING of the directory SnapshotHelper writes the PNGs into —
        // never that directory itself: fastlane's collector `rm_rf`s it after
        // moving the PNGs out (snapshot/collector.rb), which would delete
        // these sidecars before `screenshots:pop-sidecars` gets to read them.
        guard let directory = Snapshot.cacheDirectory?
            .appendingPathComponent("exp-pop", isDirectory: true) else {
            NSLog("EXP-627 pop rect: no screenshots directory — skipping \(shot)")
            return
        }
        let environment = ProcessInfo().environment
        var simulator = environment["SIMULATOR_DEVICE_NAME"] ?? "simulator"
        // Parallel runs prefix the device name; SnapshotHelper strips it too.
        if let regex = try? NSRegularExpression(pattern: "Clone [0-9]+ of ") {
            simulator = regex.stringByReplacingMatches(
                in: simulator,
                range: NSRange(location: 0, length: simulator.count),
                withTemplate: ""
            )
        }
        let payload: [String: Any] = [
            "shot": shot,
            "platform": "ios",
            "device": simulator,
            "x": round(rect.minX * 10_000) / 10_000,
            "y": round(rect.minY * 10_000) / 10_000,
            "w": round(rect.width * 10_000) / 10_000,
            "h": round(rect.height * 10_000) / 10_000,
        ]
        do {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true
            )
            let data = try JSONSerialization.data(
                withJSONObject: payload, options: [.sortedKeys]
            )
            let path = directory.appendingPathComponent("\(simulator)-pop-\(shot).json")
            try data.write(to: path, options: .atomic)
            NSLog("EXP-627 pop rect: wrote \(path.lastPathComponent)")
        } catch {
            NSLog("EXP-627 pop rect: could not write \(shot): \(error.localizedDescription)")
        }
    }
}
