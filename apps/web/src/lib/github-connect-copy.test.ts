import { describe, expect, it } from "vitest"
import * as copy from "@/lib/github-connect-copy"

// SLOP-7: the ONE GitHub vocabulary. The natives mirror the settings and
// picker halves in their own tests (desktop `github_connect.rs`, iOS
// `GithubCopy.swift`, Android `GithubCopy.kt`) — change one, change all.

describe(`github-connect-copy (SLOP-7)`, () => {
  it(`locks the guided page strings`, () => {
    expect(copy.GH_PAGE_TITLE).toBe(`Connect GitHub`)
    expect(copy.GH_STEP_CONNECT_TITLE).toBe(`Connect your GitHub account`)
    expect(copy.GH_STEP_CONNECT_BODY).toBe(
      `Exponential lists the repositories you can push to. Any login works — this only links GitHub to your account.`
    )
    expect(copy.GH_STEP_INSTALL_TITLE).toBe(`Install the Exponential app`)
    expect(copy.GH_STEP_INSTALL_BODY).toBe(
      `Install it on the GitHub account or organization that owns the repositories, and grant the ones you want. You come right back here.`
    )
    expect(copy.GH_STEP_REPO_TITLE).toBe(`Pick a repository`)
    expect(copy.ghStepRepoBodyForBoard(`App`)).toBe(
      `The repository Start coding clones for App.`
    )
    expect(copy.ghDoneBody(`acme/app`)).toBe(
      `acme/app is connected. Start coding on any issue of its board.`
    )
    expect(copy.GH_RETURN_TO_APP).toBe(`Return to the app`)
    expect(copy.GH_INSTALLED_SIGNED_OUT_TITLE).toBe(`The Exponential app is installed`)
  })

  it(`locks the connection block strings`, () => {
    expect(copy.githubInstallationLabel({ accountLogin: null, installationId: 42 })).toBe(`installation 42`)
    expect(copy.githubInstallationLabel({ accountLogin: `acme`, installationId: 42 })).toBe(`acme`)
    expect(copy.GH_STATUS_FAILED).toBe(`Couldn’t reach GitHub connect state.`)
    expect(copy.GH_NOT_CONFIGURED).toBe(`GitHub isn’t configured on this server.`)
    expect(copy.GH_NOT_LINKED).toBe(`No GitHub account connected`)
    expect(copy.ghConnectedAs(`octocat`)).toBe(`Connected as octocat`)
    expect(copy.GH_RECONNECT_NEEDED).toBe(`Your GitHub connection expired.`)
    expect(copy.GH_DISCONNECT_BODY).toBe(
      `This unlinks GitHub from your account. Repositories already added keep working; adding more needs a reconnect.`
    )
    expect(copy.GH_NOT_INSTALLED).toBe(
      `The Exponential app isn’t installed on any of your GitHub accounts yet.`
    )
    expect(copy.GH_ACCOUNTS_HEADER).toBe(`GitHub accounts with the app installed`)
    expect(copy.ghSuspendedLine([`acme`, `octocat`])).toBe(
      `GitHub suspended the Exponential app for acme, octocat. Unsuspend it on GitHub.`
    )
    expect(copy.GH_NO_REPOSITORIES).toBe(`No repositories added yet.`)
  })

  it(`locks the picker strings`, () => {
    expect(copy.GH_PICKER_LOADING).toBe(`Loading your GitHub repositories…`)
    expect(copy.GH_PICKER_NOT_LINKED).toBe(
      `Connect your GitHub account to pick a repository. You’ll come right back here.`
    )
    expect(copy.GH_PICKER_NOT_INSTALLED).toBe(
      `Install the Exponential app on the GitHub account that owns the repository. You’ll come right back here.`
    )
    expect(copy.GH_PICKER_CONNECTED_CHECK).toBe(`I’ve done that`)
    expect(copy.ghPickerSuspendedBanner([copy.GH_SUSPENDED_ACCOUNT_FALLBACK])).toBe(
      `GitHub suspended the Exponential app for a connected account. Its repositories can’t be added until you unsuspend it on GitHub.`
    )
    expect(copy.GH_NONE_PUSHABLE).toBe(
      `The app is installed, but none of its repositories lets you push. Grant one on GitHub, then refresh.`
    )
    expect(copy.GH_FOOTER_EXPLAIN).toBe(
      `Only repositories you can push to, on accounts where the app is installed, appear here. Missing one? Grant it on GitHub, then refresh.`
    )
    expect(copy.GH_LOOKUP_A11Y).toBe(`Add repository by name`)
    expect(copy.GH_ADD_FORBIDDEN).toBe(
      `GitHub says you can’t push to this repository, or your connection expired. Reconnect GitHub and try again.`
    )
    expect(copy.GH_RECONNECT_GITHUB).toBe(`Reconnect GitHub`)
  })
})
