import { describe, expect, it } from "vitest"
import * as copy from "@/lib/github-connect-copy"

// FEED-42: the four clients mirror these literals in their own tests
// (desktop `github_connect.rs`, iOS `GithubCopy.swift`, Android
// `GithubCopy.kt`) — change one, change all four.

describe(`github-connect-copy (FEED-42)`, () => {
  it(`locks the connection block strings`, () => {
    expect(copy.githubInstallationLabel({ accountLogin: null, installationId: 42 })).toBe(`installation 42`)
    expect(copy.githubInstallationLabel({ accountLogin: `acme`, installationId: 42 })).toBe(`acme`)
    expect(copy.GH_SECTION_INTRO).toBe(
      `Connect a GitHub account or organization first, then add its repositories to share them with the team — everyone can code on a shared repo. Point a board at one to make it the clone target for “Start coding”.`
    )
    expect(copy.GH_STATUS_FAILED).toBe(`Couldn’t reach GitHub connect state.`)
    expect(copy.GH_RETRY).toBe(`Retry`)
    expect(copy.GH_NOT_CONFIGURED).toBe(`GitHub isn’t configured on this server.`)
    expect(copy.GH_NOT_INSTALLED).toBe(`No GitHub account connected`)
    expect(copy.ghSuspendedLine([`acme`, `octocat`])).toBe(
      `GitHub suspended the Exponential app for acme, octocat. Unsuspend it on GitHub.`
    )
    expect(copy.GH_CONNECTED_HEADER).toBe(`GitHub accounts connected to this team`)
    expect(copy.GH_INSTALLATION_CAPTION).toBe(
      `An installation is per GitHub account or organization. Repositories come from the accounts listed here.`
    )
    expect(copy.ghReauthLine([`acme`])).toBe(
      `Reconnect GitHub to refresh which repositories you can access from acme.`
    )
    expect(copy.ghStaleLine(`acme`)).toBe(
      `No one’s GitHub connection covers acme anymore — reconnecting can’t refresh it.`
    )
    expect(copy.GH_DISCONNECT_CONFIRM_TITLE).toBe(`Disconnect GitHub account`)
    expect(copy.ghDisconnectLiveBody(`acme`)).toBe(
      `This disconnects acme from the team. Repositories connected through it must be removed first.`
    )
    expect(copy.ghDisconnectStaleBody(`acme`)).toBe(
      `This removes acme from the team. Nobody’s GitHub connection covers it, so no repositories are lost.`
    )
    expect(copy.GH_NO_REPOSITORIES).toBe(`No repositories connected yet.`)
  })

  it(`locks the picker strings`, () => {
    expect(copy.GH_PICKER_LOADING).toBe(`Loading your GitHub repositories…`)
    expect(copy.GH_PICKER_NOT_CONFIGURED).toBe(
      `GitHub isn’t configured on this server, so repositories can’t be connected.`
    )
    expect(copy.GH_PICKER_NOT_INSTALLED).toBe(
      `Connect the Exponential GitHub App to pick a repository. You’ll come right back here.`
    )
    expect(copy.GH_PICKER_CONNECTED_CHECK).toBe(`I’ve connected`)
    expect(copy.ghPickerSuspendedBanner([copy.GH_SUSPENDED_ACCOUNT_FALLBACK])).toBe(
      `GitHub suspended the Exponential app for a connected account. Its repositories can’t be connected until you unsuspend it on GitHub.`
    )
    expect(copy.ghPickerReauthBanner([`a`, `b`], false)).toBe(
      `Reconnect GitHub (a, b) to refresh. Repos created or shared with you since your last connect won’t appear until you do.`
    )
    expect(copy.ghPickerReauthBanner([], true)).toBe(
      `Reconnect GitHub to load the repositories you can access.`
    )
    expect(copy.ghPickerReauthBanner([`a`], true)).toBe(
      `Reconnect GitHub to load the repositories you can access from a.`
    )
    expect(copy.GH_NONE_GRANTED).toBe(
      `None of your connected GitHub accounts grants a repository yet.`
    )
    expect(copy.GH_FOOTER_EXPLAIN).toBe(
      `Only repositories your GitHub installation grants appear here. Missing one? Grant it on GitHub, then refresh.`
    )
    expect(copy.GH_CAP_NOTE).toBe(
      `Showing the first 500 repositories per account — use the field below for the rest.`
    )
    expect(copy.GH_LOOKUP_A11Y).toBe(`Add repository by name`)
    expect(copy.GH_ADD_FORBIDDEN).toBe(
      `GitHub says you don’t have access to this repository, or your connection is stale. Reconnect GitHub and try again.`
    )
    expect(copy.GH_RECONNECT_GITHUB).toBe(`Reconnect GitHub`)
  })
})
