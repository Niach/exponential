// @exp/site-shell — the shell every public Exponential site shares
// (apps/marketing = exponential.at, apps/ui-site = ui.exponential.at).
export { ExpLogo } from "./logo"
export { SiteHeaderBar, SiteFooterBar, useScrolled } from "./shell"
export type { ShellLink } from "./shell"
export { GitHubStarsButton } from "./GitHubStarsButton"
export { GITHUB_REPO, useGitHubStars, formatStars } from "./use-github-stars"
export { DocsLayout, DocsSection, DocsCode, DocsCallout, EnvVar } from "./docs"
export type { DocsNavEntry } from "./docs"
