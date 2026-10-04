// SLOP-7: every user-visible string of the GitHub surfaces — the guided
// page (`/integrations/github`: Connect GitHub → Install the app → Pick a
// repository), the connection block of Settings › Repositories and the
// Add-repository picker. ONE flow, one vocabulary: a person CONNECTS their
// GitHub account, INSTALLS the Exponential app on the accounts whose
// repositories they want, and ADDS repositories they can push to. The
// natives mirror the settings/picker halves (desktop `github_connect.rs`,
// iOS `GithubCopy.swift`, Android `GithubCopy.kt`), each locked by its own
// test; `github-connect-copy.test.ts` locks these. Typographic ’ everywhere.

const joinLabels = (labels: readonly string[]) => labels.join(`, `)

/** The account label: the login, else lower-case `installation {id}`. */
export const githubInstallationLabel = (inst: {
  accountLogin?: string | null
  installationId?: number
}) => inst.accountLogin || `installation ${inst.installationId}`

// ── A. The guided page ─────────────────────────────────────────────────
export const GH_PAGE_TITLE = `Connect GitHub`
export const GH_PAGE_INTRO = `Three steps, and Start coding works on your repositories.`
export const GH_STEP_CONNECT_TITLE = `Connect your GitHub account`
export const GH_STEP_CONNECT_BODY = `Exponential lists the repositories you can push to. Any login works — this only links GitHub to your account.`
export const GH_STEP_CONNECT_MET = `GitHub connected`
export const GH_STEP_INSTALL_TITLE = `Install the Exponential app`
export const GH_STEP_INSTALL_BODY = `Install it on the GitHub account or organization that owns the repositories, and grant the ones you want. You come right back here.`
export const GH_STEP_INSTALL_MET = `App installed`
export const GH_STEP_REPO_TITLE = `Pick a repository`
export const ghStepRepoBodyForBoard = (board: string) =>
  `The repository Start coding clones for ${board}.`
export const GH_STEP_REPO_BODY = `Add the repositories the team codes on. Each board points at one.`
export const GH_STEP_REPO_MET = `Repository added`
export const GH_DONE_TITLE = `All set`
export const ghDoneBody = (repo: string) =>
  `${repo} is connected. Start coding on any issue of its board.`
export const GH_DONE_BODY_NO_BOARD = `Your repositories are connected. Point a board at one from its settings, or pick one when you create a board.`
export const GH_RETURN_TO_APP = `Return to the app`
export const GH_CONTINUE = `Continue to Exponential`
export const GH_CLOSE_WINDOW = `You can close this window.`
export const GH_INSTALLED_SIGNED_OUT_TITLE = `The Exponential app is installed`
export const GH_INSTALLED_SIGNED_OUT_BODY = `Return to the app you started from to pick a repository, or sign in here to continue.`
export const GH_SIGN_IN = `Sign in`
export const GH_LINK_FAILED = `GitHub didn’t finish connecting. Try again.`

// ── B. Connection block (Settings › Repositories) ──────────────────────
export const GH_SECTION_TITLE = `Repositories`
export const GH_ADD_REPOSITORY = `Add repository`
export const GH_STATUS_FAILED = `Couldn’t reach GitHub connect state.`
export const GH_RETRY = `Retry`
export const GH_NOT_CONFIGURED = `GitHub isn’t configured on this server.`
export const GH_NOT_LINKED = `No GitHub account connected`
export const GH_CONNECT_GITHUB = `Connect GitHub`
export const ghConnectedAs = (login: string) => `Connected as ${login}`
export const GH_CONNECTED = `GitHub connected`
export const GH_RECONNECT_NEEDED = `Your GitHub connection expired.`
export const GH_RECONNECT = `Reconnect`
export const GH_RECONNECT_GITHUB = `Reconnect GitHub`
export const GH_DISCONNECT = `Disconnect`
export const GH_DISCONNECT_CONFIRM_TITLE = `Disconnect GitHub`
export const GH_DISCONNECT_BODY = `This unlinks GitHub from your account. Repositories already added keep working; adding more needs a reconnect.`
export const GH_CANCEL = `Cancel`
export const GH_NOT_INSTALLED = `The Exponential app isn’t installed on any of your GitHub accounts yet.`
export const GH_INSTALL_APP = `Install the app`
export const GH_INSTALL_ANOTHER = `Install on another account`
export const GH_ACCOUNTS_HEADER = `GitHub accounts with the app installed`
export const GH_CONFIGURE = `Configure`
export const ghConfigureTitle = (label: string) =>
  `Configure which repositories ${label} grants on GitHub`
export const GH_INSTALLATION_CAPTION = `Repositories come from these accounts. Configure one to grant more.`
export const ghSuspendedLine = (labels: readonly string[]) =>
  `GitHub suspended the Exponential app for ${joinLabels(labels)}. Unsuspend it on GitHub.`
export const GH_MANAGE = `Manage`
export const GH_NO_REPOSITORIES = `No repositories added yet.`

// ── C. Add-repository picker ───────────────────────────────────────────
export const GH_PICKER_TITLE = `Add repository`
export const GH_PICKER_LOADING = `Loading your GitHub repositories…`
export const GH_PICKER_NOT_CONFIGURED = `GitHub isn’t configured on this server, so repositories can’t be added.`
export const GH_PICKER_NOT_LINKED = `Connect your GitHub account to pick a repository. You’ll come right back here.`
export const GH_PICKER_NOT_INSTALLED = `Install the Exponential app on the GitHub account that owns the repository. You’ll come right back here.`
export const GH_PICKER_CONNECTED_CHECK = `I’ve done that`
export const GH_SUSPENDED_ACCOUNT_FALLBACK = `a connected account`
export const ghPickerSuspendedBanner = (labels: readonly string[]) =>
  `GitHub suspended the Exponential app for ${joinLabels(labels)}. Its repositories can’t be added until you unsuspend it on GitHub.`
export const GH_PICKER_RECONNECT_BANNER = `Your GitHub connection expired. Reconnect to list your repositories.`
export const GH_SEARCH_PLACEHOLDER = `Search repositories…`
export const GH_NO_MATCH = `No repositories found.`
export const GH_NONE_PUSHABLE = `The app is installed, but none of its repositories lets you push. Grant one on GitHub, then refresh.`
export const GH_FOOTER_EXPLAIN = `Only repositories you can push to, on accounts where the app is installed, appear here. Missing one? Grant it on GitHub, then refresh.`
export const GH_CAP_NOTE = `Showing the first 500 repositories per account — use the field below for the rest.`
export const GH_REFRESH = `Refresh`
export const GH_LOOKUP_PLACEHOLDER = `owner/name`
export const GH_LOOKUP_A11Y = `Add repository by name`
export const GH_LOOK_UP = `Look up`
export const GH_ADD_FORBIDDEN = `GitHub says you can’t push to this repository, or your connection expired. Reconnect GitHub and try again.`
export const GH_UPGRADE = `Upgrade`
