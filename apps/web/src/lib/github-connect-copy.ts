// FEED-42: every user-visible string of the two GitHub connect surfaces —
// the connection block (Settings › Repositories) and the Add-repository
// picker. The four clients mirror these BYTE-IDENTICALLY (desktop
// `github_connect.rs`, iOS `GithubCopy.swift`, Android `GithubCopy.kt`), each
// gated by a test against the literals in `github-connect-copy.test.ts`.
// Typographic ’ everywhere.

const joinLabels = (labels: readonly string[]) => labels.join(`, `)

/** The account label: the login, else lower-case `installation {id}`. */
export const githubInstallationLabel = (inst: {
  accountLogin?: string | null
  installationId?: number
}) => inst.accountLogin || `installation ${inst.installationId}`

// ── A. Connection block ────────────────────────────────────────────────
export const GH_SECTION_TITLE = `Repositories`
export const GH_ADD_REPOSITORY = `Add repository`
export const GH_SECTION_INTRO = `Connect a GitHub account or organization first, then add its repositories to share them with the team — everyone can code on a shared repo. Point a board at one to make it the clone target for “Start coding”.`
export const GH_STATUS_FAILED = `Couldn’t reach GitHub connect state.`
export const GH_RETRY = `Retry`
export const GH_NOT_CONFIGURED = `GitHub isn’t configured on this server.`
export const GH_NOT_INSTALLED = `No GitHub account connected`
export const GH_CONNECT_GITHUB = `Connect GitHub`
export const GH_INSTALL_ON_ACCOUNT = `Install on an account`
export const ghSuspendedLine = (labels: readonly string[]) =>
  `GitHub suspended the Exponential app for ${joinLabels(labels)}. Unsuspend it on GitHub.`
export const GH_MANAGE = `Manage`
export const GH_CONNECTED_HEADER = `GitHub accounts connected to this team`
export const GH_CONFIGURE = `Configure`
export const ghConfigureTitle = (label: string) =>
  `Configure which repositories ${label} grants on GitHub`
export const GH_UNLINK_TITLE = `Disconnect this GitHub account from the team`
export const GH_INSTALLATION_CAPTION = `An installation is per GitHub account or organization. Repositories come from the accounts listed here.`
export const GH_CONNECT_ANOTHER = `Connect another account`
export const GH_REFRESH_ACCESS = `Refresh access`
export const ghReauthLine = (labels: readonly string[]) =>
  `Reconnect GitHub to refresh which repositories you can access from ${joinLabels(labels)}.`
export const GH_RECONNECT = `Reconnect`
export const ghStaleLine = (label: string) =>
  `No one’s GitHub connection covers ${label} anymore — reconnecting can’t refresh it.`
export const GH_DISCONNECT_ACCOUNT = `Disconnect account`
export const GH_DISCONNECT_CONFIRM_TITLE = `Disconnect GitHub account`
export const ghDisconnectLiveBody = (label: string) =>
  `This disconnects ${label} from the team. Repositories connected through it must be removed first.`
export const ghDisconnectStaleBody = (label: string) =>
  `This removes ${label} from the team. Nobody’s GitHub connection covers it, so no repositories are lost.`
export const GH_CANCEL = `Cancel`
export const GH_DISCONNECT = `Disconnect`
export const GH_NO_REPOSITORIES = `No repositories connected yet.`

// ── B. Add-repository picker ───────────────────────────────────────────
export const GH_PICKER_TITLE = `Add repository`
export const GH_PICKER_LOADING = `Loading your GitHub repositories…`
export const GH_PICKER_NOT_CONFIGURED = `GitHub isn’t configured on this server, so repositories can’t be connected.`
export const GH_PICKER_NOT_INSTALLED = `Connect the Exponential GitHub App to pick a repository. You’ll come right back here.`
export const GH_PICKER_CONNECTED_CHECK = `I’ve connected`
export const GH_SUSPENDED_ACCOUNT_FALLBACK = `a connected account`
export const ghPickerSuspendedBanner = (labels: readonly string[]) =>
  `GitHub suspended the Exponential app for ${joinLabels(labels)}. Its repositories can’t be connected until you unsuspend it on GitHub.`
/** Re-auth banner; `empty` = the listing has no repos at all. */
export const ghPickerReauthBanner = (
  labels: readonly string[],
  empty: boolean
) =>
  empty
    ? `Reconnect GitHub to load the repositories you can access${labels.length > 0 ? ` from ${joinLabels(labels)}` : ``}.`
    : `Reconnect GitHub${labels.length > 0 ? ` (${joinLabels(labels)})` : ``} to refresh. Repos created or shared with you since your last connect won’t appear until you do.`
export const GH_RECONNECT_GITHUB = `Reconnect GitHub`
export const GH_SEARCH_PLACEHOLDER = `Search repositories…`
export const GH_NO_MATCH = `No repositories found.`
export const GH_NONE_GRANTED = `None of your connected GitHub accounts grants a repository yet.`
export const GH_FOOTER_EXPLAIN = `Only repositories your GitHub installation grants appear here. Missing one? Grant it on GitHub, then refresh.`
export const GH_CAP_NOTE = `Showing the first 500 repositories per account — use the field below for the rest.`
export const GH_REFRESH = `Refresh`
export const GH_INSTALL_ANOTHER = `Install on another account`
export const GH_LOOKUP_PLACEHOLDER = `owner/name`
export const GH_LOOKUP_A11Y = `Add repository by name`
export const GH_LOOK_UP = `Look up`
export const GH_ADD_FORBIDDEN = `GitHub says you don’t have access to this repository, or your connection is stale. Reconnect GitHub and try again.`
export const GH_UPGRADE = `Upgrade`
