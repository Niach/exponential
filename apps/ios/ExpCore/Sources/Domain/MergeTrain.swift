import Foundation

/// #EXP-1707/#EXP-1710: the merge train's client half ×4 (packages/domain-
/// contract/fixtures/merge-train.json STATUS PILL / MENU / NEEDS YOU is the
/// spec; TS reference `src/merge-train.ts` `ordinal`, `statusPill`,
/// `mergeMenuEntries`, `stuckRow`). Pure, replayed by `MergeTrainTests`.
public enum MergeTrain {
    // MARK: - Copy, limits, test ids (fixture `copy` / `limits` / `testIds`)

    public enum Copy {
        public enum Pill {
            public static let draft = "Draft"
            public static let checksRunning = "Checks running"
            public static let sessionWorking = "Session working"
            public static let queued = "Queued"
            public static let queuedAt = "Queued · {position}"
            public static let testing = "Testing"
            public static let testingWith = "Testing with {count}"
            public static let merged = "Merged"
            public static let conflict = "Conflict"
            public static let failing = "Checks failing"
            public static let onHold = "On hold"
            public static let stuck = "Auto-merge stuck"
            public static let needsApproval = "Needs approval"
        }

        public enum Menu {
            public static let mergeNow = "Merge now"
            public static let hold = "Hold auto-merge"
            public static let resume = "Resume auto-merge"
        }

        public enum NeedsYou {
            public static let title = "Auto-merge stuck on #{number}"
            public static let bounces = "{count} bounces"
            public static let timeout = "24 hours"
            public static let offline = "Device offline"
            public static let retry = "Retry"
            public static let mergeNow = "Merge now"
            public static let close = "Close PR"
        }

        public enum Settings {
            public static let label = "Merge"
            public static let auto = "Automatic"
            public static let approval = "After approval"
        }

        public enum Events {
            public static let merged = "Merged PR {pr}"
            public static let conflict = "Conflict on PR {pr}"
            public static let failing = "Checks failing on PR {pr}"
            public static let held = "Auto-merge stopped on PR {pr}"
        }
    }

    public enum Limits {
        public static let bounces = 3
        public static let offlineHours = 2
    }

    public enum TestIds {
        public static let statusPill = "pr-merge-status"
        public static let mergeNow = "pr-merge-now"
        public static let hold = "pr-merge-hold"
        public static let resume = "pr-merge-resume"
        public static let stuckRow = "needs-you-merge-stuck"
        public static let settingsRow = "settings-merge-mode"
    }

    /// contract prMergeState; nil / unknown reads as `checks`.
    public static func normalizedState(_ state: String?) -> String {
        guard let state, DomainContract.prMergeStateValues.contains(state) else {
            return DomainContract.prMergeStateChecks
        }
        return state
    }

    /// session-thread.json copy.events `pr_merge` by the card's
    /// `mergeOutcome`; nil = an unknown / missing outcome (the caller keeps
    /// its fallback).
    public static func eventTemplate(outcome: String?) -> String? {
        switch outcome {
        case "merged": return Copy.Events.merged
        case "conflict": return Copy.Events.conflict
        case "failing": return Copy.Events.failing
        case "held": return Copy.Events.held
        default: return nil
        }
    }

    // MARK: - The status pill

    public struct StatusPill: Equatable, Sendable {
        public enum Glyph: String, Sendable {
            case spinner
            case prDraft = "pr-draft"
            case ciQueued = "ci-queued"
            case prMerged = "pr-merged"
            case prConflict = "pr-conflict"
            case ciFailed = "ci-failed"
            case prHold = "pr-hold"
        }

        public enum Tone: String, Sendable { case muted, red }
        /// approve = the merge confirm, then the queue; checks = the Checks
        /// submenu; nil = readonly.
        public enum Tap: String, Sendable { case approve, checks }
        public enum Confirm: String, Sendable { case merge, stack }

        public let label: String
        public let glyph: Glyph
        public let tone: Tone
        public let primary: Bool
        public let tap: Tap?
        public let confirm: Confirm?
    }

    /// `1st 2nd 3rd 4th 11th 12th 13th 21st`.
    public static func ordinal(_ n: Int) -> String {
        let rem100 = n % 100
        if rem100 >= 11 && rem100 <= 13 { return "\(n)th" }
        switch n % 10 {
        case 1: return "\(n)st"
        case 2: return "\(n)nd"
        case 3: return "\(n)rd"
        default: return "\(n)th"
        }
    }

    /// THE header status pill. `batchSize` = PRs in the testing run, this
    /// one included; `checks` = the PR's check rows (none = readonly).
    public static func statusPill(
        state: String?,
        wait: String?,
        position: Int? = nil,
        batchSize: Int? = nil,
        holdReason: String? = nil,
        stack: Bool,
        checks: Int
    ) -> StatusPill {
        func status(_ label: String, _ glyph: StatusPill.Glyph, _ tone: StatusPill.Tone = .muted) -> StatusPill {
            StatusPill(label: label, glyph: glyph, tone: tone, primary: false, tap: checks > 0 ? .checks : nil, confirm: nil)
        }
        switch normalizedState(state) {
        case DomainContract.prMergeStateDraft:
            return status(Copy.Pill.draft, .prDraft)
        case DomainContract.prMergeStateQueued:
            if let position, position > 0 {
                return status(Copy.Pill.queuedAt.replacingOccurrences(of: "{position}", with: ordinal(position)), .ciQueued)
            }
            return status(Copy.Pill.queued, .ciQueued)
        case DomainContract.prMergeStateTesting:
            let others = (batchSize ?? 1) - 1
            return status(
                others > 0 ? Copy.Pill.testingWith.replacingOccurrences(of: "{count}", with: "\(others)") : Copy.Pill.testing,
                .spinner
            )
        case DomainContract.prMergeStateMerged:
            return status(Copy.Pill.merged, .prMerged)
        case DomainContract.prMergeStateConflict:
            return status(Copy.Pill.conflict, .prConflict, .red)
        case DomainContract.prMergeStateFailing:
            return status(Copy.Pill.failing, .ciFailed, .red)
        case DomainContract.prMergeStateHeld:
            return holdReason == "person"
                ? status(Copy.Pill.onHold, .prHold)
                : status(Copy.Pill.stuck, .prHold, .red)
        default: // checks
            if wait == "approval" {
                return StatusPill(
                    label: Copy.Pill.needsApproval, glyph: .prMerged, tone: .muted, primary: true,
                    tap: .approve, confirm: stack ? .stack : .merge
                )
            }
            return status(wait == "session" ? Copy.Pill.sessionWorking : Copy.Pill.checksRunning, .spinner)
        }
    }

    /// #EXP-1710: the pill's state off the PR's OWN state first: closed =
    /// no pill (nil); merged = `merged`; draft = `draft`; else the train's
    /// `merge_state`.
    public static func pillState(prState: String, mergeState: String?) -> String? {
        switch prState {
        case DomainContract.prStateClosed: return nil
        case DomainContract.prStateMerged: return DomainContract.prMergeStateMerged
        case "draft": return DomainContract.prMergeStateDraft
        default: return normalizedState(mergeState)
        }
    }

    /// #EXP-1710: a viewer who may not approve sees `Needs approval` as a
    /// plain status: the same label and glyph, not primary, its tap the
    /// Checks rows (readonly without rows).
    public static func viewerPill(_ pill: StatusPill, canApprove: Bool, checks: Int) -> StatusPill {
        guard pill.tap == .approve, !canApprove else { return pill }
        return StatusPill(
            label: pill.label, glyph: pill.glyph, tone: pill.tone, primary: false,
            tap: checks > 0 ? .checks : nil, confirm: nil
        )
    }

    /// One `statusPill` input (the fixture's `input`).
    public struct PillInput: Equatable, Sendable {
        public let state: String
        public let wait: String?
        public let position: Int?
        public let batchSize: Int?
        public let holdReason: String?
        public let stack: Bool
        public let checks: Int

        public init(
            state: String, wait: String? = nil, position: Int? = nil, batchSize: Int? = nil,
            holdReason: String? = nil, stack: Bool = false, checks: Int = 3
        ) {
            self.state = state
            self.wait = wait
            self.position = position
            self.batchSize = batchSize
            self.holdReason = holdReason
            self.stack = stack
            self.checks = checks
        }

        public var pill: StatusPill {
            MergeTrain.statusPill(
                state: state, wait: wait, position: position, batchSize: batchSize,
                holdReason: holdReason, stack: stack, checks: checks
            )
        }
    }

    /// The styleguide `pr-checks` specimen: EVERY merge-train.json
    /// `statusPill` case's input, in fixture order (`MergeTrainTests` locks it).
    public static let specimen: [PillInput] = [
        PillInput(state: "draft", wait: "draft"),
        PillInput(state: "checks", wait: "checks"),
        PillInput(state: "checks", wait: "session"),
        PillInput(state: "queued", position: 2),
        PillInput(state: "queued", position: 1),
        PillInput(state: "queued", position: 11),
        PillInput(state: "queued", position: 23),
        PillInput(state: "testing", batchSize: 4),
        PillInput(state: "testing", batchSize: 1),
        PillInput(state: "merged"),
        PillInput(state: "conflict"),
        PillInput(state: "failing", wait: "failing"),
        PillInput(state: "held", holdReason: "person"),
        PillInput(state: "held", holdReason: "bounces"),
        PillInput(state: "queued", position: 3, checks: 0),
        PillInput(state: "checks", wait: "approval", checks: 0),
        PillInput(state: "checks", wait: "approval", stack: true),
    ]

    // MARK: - The `…` menu

    public enum MenuEntry: String, Sendable, Equatable {
        case checks
        case mergeNow = "merge-now"
        case hold
        case resume
        case closePr = "close-pr"
    }

    /// The header `…` (the thread menu): checks
    /// (any viewer, rows), Merge now (owners; a live PR not draft, merged or
    /// conflict), Hold | Resume auto-merge (members; a live PR not draft or
    /// merged), Close PR (members, live PR). `state` = contract prState of
    /// the PR, nil = none.
    public static func mergeMenuEntries(
        state: String?, member: Bool, owner: Bool, checks: Int, mergeState: String?
    ) -> [MenuEntry] {
        guard let state else { return [] }
        var entries: [MenuEntry] = checks > 0 ? [.checks] : []
        let live = state == DomainContract.prStateOpen || state == "draft"
        guard live, member else { return entries }
        let ms = mergeState ?? DomainContract.prMergeStateChecks
        let draft = ms == DomainContract.prMergeStateDraft
        let merged = ms == DomainContract.prMergeStateMerged
        if owner && !draft && !merged && ms != DomainContract.prMergeStateConflict { entries.append(.mergeNow) }
        if !draft && !merged { entries.append(ms == DomainContract.prMergeStateHeld ? .resume : .hold) }
        entries.append(.closePr)
        return entries
    }

    // MARK: - Needs you

    public struct StuckRow: Equatable, Sendable {
        public enum Action: String, Sendable { case retry, mergeNow = "merge-now", closePr = "close-pr" }
        public let title: String
        public let meta: String
        public let actions: [Action]
    }

    /// The Needs you row of a stuck PR, or nil: `held` by the limits, or a
    /// `conflict`/`failing` whose opener's device is offline 2 h+. A person's
    /// Hold is never a row.
    public static func stuckRow(
        number: Int, state: String?, holdReason: String?, deviceOfflineSince: Date?, now: Date, owner: Bool
    ) -> StuckRow? {
        let ms = normalizedState(state)
        var meta: String?
        if ms == DomainContract.prMergeStateHeld && holdReason == "bounces" {
            meta = Copy.NeedsYou.bounces.replacingOccurrences(of: "{count}", with: "\(Limits.bounces)")
        } else if ms == DomainContract.prMergeStateHeld && holdReason == "timeout" {
            meta = Copy.NeedsYou.timeout
        } else if ms == DomainContract.prMergeStateConflict || ms == DomainContract.prMergeStateFailing,
                  let since = deviceOfflineSince,
                  now.timeIntervalSince(since) >= Double(Limits.offlineHours) * 3600 {
            meta = Copy.NeedsYou.offline
        }
        guard let meta else { return nil }
        return StuckRow(
            title: Copy.NeedsYou.title.replacingOccurrences(of: "{number}", with: "\(number)"),
            meta: meta,
            actions: owner ? [.retry, .mergeNow, .closePr] : [.retry, .closePr]
        )
    }
}
