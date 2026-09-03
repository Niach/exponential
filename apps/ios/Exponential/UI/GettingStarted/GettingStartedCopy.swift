import Foundation

/// Every string the getting-started checklist renders (EXP-698 r5).
///
/// One place per platform, byte-identical across all four: web's
/// `getting-started-copy.ts`, the IDE's `getting_started::copy`, Android's
/// `GettingStartedCopy.kt` and this file. A drift test
/// (`apps/web/src/components/getting-started/getting-started-copy.test.ts`)
/// reads the native sources and asserts each value appears verbatim, so every
/// literal below is a PLAIN double-quoted single-line string — no
/// interpolation, no concatenation, no escapes.
///
/// Mobile carries seven of the ten entries: the widget, helpdesk and MCP steps
/// are web/IDE-only surfaces, so the phone neither shows them nor counts them.
enum GettingStartedCopy {
    static let desktopTitle = "Get the desktop app"
    static let desktopDescription = "Runs coding sessions on your machine and registers it as one of your devices."
    static let desktopAction = "Download the desktop app"

    static let githubTitle = "Connect a GitHub repo"
    static let githubDescription = "Boards attach repositories; pull requests and coding sessions flow back into issues."
    static let githubAction = "Connect GitHub"

    static let inviteTitle = "Invite your team"
    static let inviteDescription = "Teammates share boards, reviews, and the support inbox."
    static let inviteAction = "Invite in team settings"

    static let boardTitle = "Create a board"
    static let boardDescription = "Boards hold your issues; connect a repository to code on one."
    static let boardAction = "Create a board"

    static let codingTitle = "Start coding with an agent"
    static let codingDescription = "Start coding on an issue hands it to your agent, which plans, implements, and opens the PR."
    static let codingAction = "Open Devices"

    static let actionTitle = "Create an action"
    static let actionDescription = "Reusable agent runs for your team, written by your agent from a description."
    static let actionAction = "New action"

    static let serverTitle = "Set up a server"
    static let serverDescription = "Run the headless daemon on an always-on machine to take remote Start coding requests."
    static let serverAction = "Copy install command"

    /// The section header and its progress readout.
    static let sectionTitle = "Getting started"

    static func progress(done: Int, total: Int) -> String {
        "\(done)/\(total) done"
    }
}

/// One checklist step. The order of the cases IS the display order, minus the
/// three web-only entries (widget, helpdesk, mcp).
enum GettingStartedEntryKey: String, CaseIterable, Identifiable {
    case desktop
    case github
    case invite
    case board
    case coding
    case action
    case server

    var id: String { rawValue }

    var title: String {
        switch self {
        case .desktop: GettingStartedCopy.desktopTitle
        case .github: GettingStartedCopy.githubTitle
        case .invite: GettingStartedCopy.inviteTitle
        case .board: GettingStartedCopy.boardTitle
        case .coding: GettingStartedCopy.codingTitle
        case .action: GettingStartedCopy.actionTitle
        case .server: GettingStartedCopy.serverTitle
        }
    }

    var entryDescription: String {
        switch self {
        case .desktop: GettingStartedCopy.desktopDescription
        case .github: GettingStartedCopy.githubDescription
        case .invite: GettingStartedCopy.inviteDescription
        case .board: GettingStartedCopy.boardDescription
        case .coding: GettingStartedCopy.codingDescription
        case .action: GettingStartedCopy.actionDescription
        case .server: GettingStartedCopy.serverDescription
        }
    }

    var actionLabel: String {
        switch self {
        case .desktop: GettingStartedCopy.desktopAction
        case .github: GettingStartedCopy.githubAction
        case .invite: GettingStartedCopy.inviteAction
        case .board: GettingStartedCopy.boardAction
        case .coding: GettingStartedCopy.codingAction
        case .action: GettingStartedCopy.actionAction
        case .server: GettingStartedCopy.serverAction
        }
    }
}

/// One-line hints for locked entries, keyed by entry + the step that unlocks
/// it. Same wording as web's `lockedHint`, minus the widget arm.
func gettingStartedLockedHint(
    entry: GettingStartedEntryKey,
    lockedBy: GettingStartedEntryKey
) -> String {
    if entry == .coding && lockedBy == .desktop {
        return "Connect a machine first — coding sessions run on the desktop app or a registered server."
    }
    if entry == .coding && lockedBy == .github {
        return "Connect a GitHub repo first. Coding sessions need a repo-backed board."
    }
    if entry == .coding && lockedBy == .board {
        return "Create a board with a repository first."
    }
    if entry == .action {
        return "Connect a machine first — the action creator runs on the desktop app or a registered server."
    }
    return "Complete \"\(lockedBy.title)\" first."
}
