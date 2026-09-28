import Foundation

/// EXP-1121 — whether an issue can Start coding RIGHT NOW, as three ordered
/// steps every client derives from data it already has: GitHub connected, a
/// repository on the board, a device online. Start coding always renders for
/// a member (dashed amber while a step is missing, the caption naming the
/// FIRST missing one); a tap opens the "Ready to code?" checklist, each unmet
/// row carrying its fix.
///
/// Pure and FIXTURE-LOCKED ×4 (`packages/domain-contract/fixtures/
/// coding-readiness.json`): web `lib/coding-readiness.ts`, desktop
/// `domain::coding_readiness`, Android `domain/CodingReadiness.kt` return
/// byte-identical output, copy included.
///
/// Remote start (the steer relay) is NOT a step: an instance without it has
/// no remote coding at all, so the button stays hidden (`visible == false`).
public enum CodingReadiness {

    public enum StepKey: String, Equatable, Sendable, CaseIterable {
        case github, repository, device
    }

    /// `met` = green tick; `current` = the FIRST unmet step (highlighted,
    /// fixes shown); `pending` = unmet behind it (no fixes until current).
    public enum StepState: String, Equatable, Sendable {
        case met, current, pending
    }

    public enum Fix: String, Equatable, Sendable {
        case connectGithub = "connect_github"
        case chooseRepository = "choose_repository"
        case boardSettings = "board_settings"
        case openDevices = "open_devices"
        case getDesktopApp = "get_desktop_app"
        case setUpServer = "set_up_server"

        /// The fix button's label.
        public var label: String {
            switch self {
            case .connectGithub: Copy.fixConnectGithub
            case .chooseRepository: Copy.fixChooseRepository
            case .boardSettings: Copy.fixBoardSettings
            case .openDevices: Copy.fixOpenDevices
            case .getDesktopApp: Copy.fixGetDesktopApp
            case .setUpServer: Copy.fixSetUpServer
            }
        }
    }

    public struct Device: Equatable, Sendable {
        public let label: String
        /// Registered by the caller (a teammate's shared server is not).
        public let own: Bool
        public let online: Bool
        /// Unix ms, nil when never seen.
        public let lastSeenAtMs: Int64?

        public init(label: String, own: Bool, online: Bool, lastSeenAtMs: Int64?) {
            self.label = label
            self.own = own
            self.online = online
            self.lastSeenAtMs = lastSeenAtMs
        }
    }

    public struct Github: Equatable, Sendable {
        public let connected: Bool
        /// The connected account (`acme-inc`) for the met row's detail.
        public let label: String?

        public init(connected: Bool, label: String?) {
            self.connected = connected
            self.label = label
        }
    }

    public struct Input: Equatable, Sendable {
        public let isMember: Bool
        /// `steer.config.enabled`; nil while loading.
        public let remoteStartEnabled: Bool?
        public let teamName: String
        public let boardName: String
        /// The board's repository full name (`owner/name`), nil = none. A
        /// board with a `repositoryId` whose row has not resolved yet passes "".
        public let boardRepository: String?
        /// Asked only while the board has no repository: does the team have a
        /// GitHub installation or repository? nil = still loading.
        public let github: Github?
        /// Own devices + servers shared with the team; nil while loading.
        public let devices: [Device]?
        public let nowMs: Int64

        public init(
            isMember: Bool,
            remoteStartEnabled: Bool?,
            teamName: String,
            boardName: String,
            boardRepository: String?,
            github: Github?,
            devices: [Device]?,
            nowMs: Int64
        ) {
            self.isMember = isMember
            self.remoteStartEnabled = remoteStartEnabled
            self.teamName = teamName
            self.boardName = boardName
            self.boardRepository = boardRepository
            self.github = github
            self.devices = devices
            self.nowMs = nowMs
        }
    }

    public struct Step: Equatable, Sendable {
        public let key: StepKey
        public let state: StepState
        public let title: String
        public let body: String?
        /// Right-aligned detail of a met row (account, repo, device label).
        public let detail: String?
        public let fixes: [Fix]

        public init(key: StepKey, state: StepState, title: String, body: String?, detail: String?, fixes: [Fix]) {
            self.key = key
            self.state = state
            self.title = title
            self.body = body
            self.detail = detail
            self.fixes = fixes
        }
    }

    public struct Readiness: Equatable, Sendable {
        /// Non-member or no remote start on this instance: render nothing.
        public let visible: Bool
        /// Inputs still loading: the button shows but stays inert, no caption.
        public let loading: Bool
        public let ready: Bool
        public let metCount: Int
        public let total: Int
        public let steps: [Step]
        /// "1 of 3 set up. Fix the rest here."
        public let summary: String
        /// The one-line caption: the first missing step, nil when ready or
        /// loading.
        public let caption: String?

        public init(
            visible: Bool, loading: Bool, ready: Bool, metCount: Int, total: Int,
            steps: [Step], summary: String, caption: String?
        ) {
            self.visible = visible
            self.loading = loading
            self.ready = ready
            self.metCount = metCount
            self.total = total
            self.steps = steps
            self.summary = summary
            self.caption = caption
        }
    }

    // MARK: - Copy (byte-identical ×4)

    public enum Copy {
        public static let title = "Ready to code?"
        public static let start = "Start coding"
        public static let close = "Close"
        public static let captionGithub = "Needs GitHub"
        public static let captionRepository = "Needs a repository"
        public static let captionDevice = "No device online"
        public static let githubMet = "GitHub connected"
        public static let githubUnmet = "Connect GitHub"
        public static let githubBody =
            "Start coding clones a repository from a GitHub account or organization connected to the team."
        public static let repositoryMet = "Repository connected"
        public static let deviceMet = "Device online"
        public static let deviceUnmet = "A device online"
        public static let deviceNeverBody =
            "Coding runs on the desktop app or on a server running the CLI. You haven’t set one up yet."
        public static let fixConnectGithub = "Connect GitHub"
        public static let fixChooseRepository = "Choose repository"
        public static let fixBoardSettings = "Board settings"
        public static let fixOpenDevices = "Open Devices"
        public static let fixGetDesktopApp = "Get the desktop app"
        public static let fixSetUpServer = "Set up a server"
        public static let pickerSearch = "Search repositories…"
        public static let pickerMatchesBoard = "matches board"
        public static let pickerAddFromGithub = "Add another repository from GitHub…"
        public static let pickerEmpty = "No repositories connected to the team yet."
        public static let allSet = "All set."
        public static let oneLeft = "One step left."
        public static let fixRest = "Fix the rest here."
        public static let justNow = "just now"

        /// Every constant, keyed like web's `READINESS_COPY` (the fixture's
        /// `copy` table).
        public static let table: [String: String] = [
            "title": title,
            "start": start,
            "close": close,
            "captionGithub": captionGithub,
            "captionRepository": captionRepository,
            "captionDevice": captionDevice,
            "githubMet": githubMet,
            "githubUnmet": githubUnmet,
            "githubBody": githubBody,
            "repositoryMet": repositoryMet,
            "deviceMet": deviceMet,
            "deviceUnmet": deviceUnmet,
            "deviceNeverBody": deviceNeverBody,
            "fixConnectGithub": fixConnectGithub,
            "fixChooseRepository": fixChooseRepository,
            "fixBoardSettings": fixBoardSettings,
            "fixOpenDevices": fixOpenDevices,
            "fixGetDesktopApp": fixGetDesktopApp,
            "fixSetUpServer": fixSetUpServer,
            "pickerSearch": pickerSearch,
            "pickerMatchesBoard": pickerMatchesBoard,
            "pickerAddFromGithub": pickerAddFromGithub,
            "pickerEmpty": pickerEmpty,
            "allSet": allSet,
            "oneLeft": oneLeft,
            "fixRest": fixRest,
            "justNow": justNow,
        ]
    }

    public static func repositoryTitle(_ board: String) -> String {
        "Connect a repository to \(board)"
    }

    public static func repositoryBody(_ board: String) -> String {
        "Start coding clones the board’s repository. \(board) has none yet."
    }

    public static func deviceBody(_ team: String) -> String {
        "None of your devices, or the ones shared with \(team), is online."
    }

    public static func lastSeen(_ label: String, _ ago: String) -> String {
        "Your \(label) was last seen \(ago)."
    }

    public static func pickerUsedBy(_ board: String) -> String {
        "used by \(board)"
    }

    public static func summary(met: Int, total: Int) -> String {
        let tail = met == total
            ? Copy.allSet
            : (total - met == 1 ? Copy.oneLeft : Copy.fixRest)
        return "\(met) of \(total) set up. \(tail)"
    }

    /// "just now" / "5 min ago" / "2 h ago" / "3 d ago" (floored).
    public static func ago(nowMs: Int64, thenMs: Int64) -> String {
        let seconds = max(0, (nowMs - thenMs) / 1000)
        if seconds < 60 { return Copy.justNow }
        let minutes = seconds / 60
        if minutes < 60 { return "\(minutes) min ago" }
        let hours = minutes / 60
        if hours < 24 { return "\(hours) h ago" }
        return "\(hours / 24) d ago"
    }

    private static func caption(_ key: StepKey) -> String {
        switch key {
        case .github: Copy.captionGithub
        case .repository: Copy.captionRepository
        case .device: Copy.captionDevice
        }
    }

    /// The caller's most recently seen OWN device, for "Your … was last seen"
    /// (a teammate's shared server is never "yours"). First wins a tie.
    private static func lastSeenDevice(_ devices: [Device]) -> Device? {
        var best: Device?
        for device in devices {
            guard device.own, let seen = device.lastSeenAtMs else { continue }
            if best == nil || seen > (best?.lastSeenAtMs ?? 0) { best = device }
        }
        return best
    }

    public static func derive(_ input: Input) -> Readiness {
        let hasRepository = input.boardRepository != nil
        let visible = input.isMember && input.remoteStartEnabled != false
        let loading = input.remoteStartEnabled == nil
            || input.devices == nil
            || (!hasRepository && input.github == nil)

        let devices = input.devices ?? []
        let online = devices.first { $0.online }
        let githubMet = hasRepository || input.github?.connected == true
        func isMet(_ key: StepKey) -> Bool {
            switch key {
            case .github: githubMet
            case .repository: hasRepository
            case .device: online != nil
            }
        }
        let order: [StepKey] = [.github, .repository, .device]
        let firstMissing = order.first { !isMet($0) }
        func state(_ key: StepKey) -> StepState {
            isMet(key) ? .met : (key == firstMissing ? .current : .pending)
        }

        let seen = lastSeenDevice(devices)
        let lastSeenLine: String? = {
            guard let seen, let at = seen.lastSeenAtMs else { return nil }
            return lastSeen(seen.label, ago(nowMs: input.nowMs, thenMs: at))
        }()
        let registered = devices.contains { $0.own }

        let steps: [Step] = order.map { key in
            let s = state(key)
            switch key {
            case .github:
                return s == .met
                    ? Step(
                        key: key, state: s, title: Copy.githubMet, body: nil,
                        detail: input.github?.label, fixes: []
                    )
                    : Step(
                        key: key, state: s, title: Copy.githubUnmet, body: Copy.githubBody,
                        detail: nil, fixes: [.connectGithub]
                    )
            case .repository:
                if s == .met {
                    let repo = input.boardRepository ?? ""
                    return Step(
                        key: key, state: s, title: Copy.repositoryMet, body: nil,
                        detail: repo.isEmpty ? nil : repo, fixes: []
                    )
                }
                return Step(
                    key: key, state: s,
                    title: repositoryTitle(input.boardName),
                    body: repositoryBody(input.boardName),
                    detail: nil,
                    fixes: s == .current ? [.chooseRepository, .boardSettings] : []
                )
            case .device:
                if s == .met {
                    return Step(
                        key: key, state: s, title: Copy.deviceMet, body: nil,
                        detail: online?.label, fixes: []
                    )
                }
                if s == .pending {
                    return Step(
                        key: key, state: s, title: Copy.deviceUnmet, body: lastSeenLine,
                        detail: nil, fixes: []
                    )
                }
                let body = !registered
                    ? Copy.deviceNeverBody
                    : [deviceBody(input.teamName), lastSeenLine]
                        .compactMap { $0 }
                        .filter { !$0.isEmpty }
                        .joined(separator: " ")
                return Step(
                    key: key, state: s, title: Copy.deviceUnmet, body: body, detail: nil,
                    fixes: registered
                        ? [.openDevices, .getDesktopApp, .setUpServer]
                        : [.getDesktopApp, .setUpServer]
                )
            }
        }

        let metCount = order.filter(isMet).count
        return Readiness(
            visible: visible,
            loading: loading,
            ready: !loading && firstMissing == nil,
            metCount: metCount,
            total: order.count,
            steps: steps,
            summary: summary(met: metCount, total: order.count),
            caption: (!visible || loading || firstMissing == nil) ? nil : firstMissing.map(caption)
        )
    }
}
