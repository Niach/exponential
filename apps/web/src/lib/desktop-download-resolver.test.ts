import { afterEach, beforeEach, describe, expect, it } from "vitest"
import {
  isDesktopOs,
  pickDesktopDownloads,
  resetDesktopDownloadCache,
  resolveDesktopDownload,
  type GithubRelease,
} from "./desktop-download-resolver"
import { DESKTOP_ASSET_NAMES } from "./desktop-download"
import { resetGithubClientForTests } from "./integrations/github-client"

const DMG = DESKTOP_ASSET_NAMES.macos
const EXE = DESKTOP_ASSET_NAMES.windows
const APPIMAGE = DESKTOP_ASSET_NAMES.linux

function release(tag: string, assets: string[], extra: Partial<GithubRelease> = {}): GithubRelease {
  return {
    tag_name: tag,
    assets: assets.map((name) => ({
      name,
      browser_download_url: `https://dl/${tag}/${name}`,
      state: `uploaded`,
    })),
    ...extra,
  }
}

describe(`pickDesktopDownloads`, () => {
  it(`falls back per OS to the newest release with the asset`, () => {
    const targets = pickDesktopDownloads([
      // A fresh release: .exe still building, .dmg lost to a notarization flake.
      release(`desktop-v0.14.120`, [APPIMAGE]),
      release(`desktop-v0.14.119`, [APPIMAGE, DMG]),
      release(`desktop-v0.14.118`, [APPIMAGE, DMG, EXE]),
    ])
    expect(targets.linux?.url).toBe(`https://dl/desktop-v0.14.120/${APPIMAGE}`)
    expect(targets.macos?.url).toBe(`https://dl/desktop-v0.14.119/${DMG}`)
    expect(targets.windows?.url).toBe(`https://dl/desktop-v0.14.118/${EXE}`)
  })

  it(`compares versions, not list order or strings`, () => {
    const targets = pickDesktopDownloads([
      release(`desktop-v0.14.9`, [DMG]),
      release(`desktop-v0.14.100`, [DMG]),
      release(`desktop-v0.14.20`, [DMG]),
    ])
    expect(targets.macos?.url).toContain(`desktop-v0.14.100/`)
  })

  it(`ignores other tag families, drafts, prereleases and partial uploads`, () => {
    const partial = release(`desktop-v0.14.5`, [DMG])
    partial.assets![0]!.state = `starter`
    const targets = pickDesktopDownloads([
      release(`android-v9.0.0`, [DMG]),
      release(`v9.0.0`, [DMG]),
      release(`desktop-v0.14.7`, [DMG], { draft: true }),
      release(`desktop-v0.14.6`, [DMG], { prerelease: true }),
      partial,
      release(`desktop-v0.14.1`, [DMG]),
    ])
    expect(targets.macos?.url).toContain(`desktop-v0.14.1/`)
  })

  it(`has no target for an asset no release carries`, () => {
    expect(pickDesktopDownloads([release(`desktop-v1.0.0`, [DMG])]).windows).toBeUndefined()
  })
})

describe(`resolveDesktopDownload`, () => {
  let clock = 1_000_000
  let calls = 0
  let answer: () => Response | Promise<Response>
  const deps = {
    now: () => clock,
    sleep: () => Promise.resolve(),
    fetch: () => {
      calls++
      return Promise.resolve(answer())
    },
  }
  const list = (...releases: GithubRelease[]) => () => Response.json(releases)

  beforeEach(() => {
    clock = 1_000_000
    calls = 0
    resetDesktopDownloadCache()
    resetGithubClientForTests()
  })
  afterEach(() => resetGithubClientForTests())

  it(`answers from one list for every OS while it is fresh`, async () => {
    answer = list(release(`desktop-v2.0.0`, [DMG]), release(`desktop-v1.0.0`, [DMG, EXE]))
    expect(await resolveDesktopDownload(`macos`, deps)).toEqual({
      url: `https://dl/desktop-v2.0.0/${DMG}`,
      fromRelease: true,
    })
    expect((await resolveDesktopDownload(`windows`, deps)).url).toContain(`desktop-v1.0.0/`)
    expect(calls).toBe(1)
  })

  it(`re-reads the list after the cache window, without holding the click`, async () => {
    answer = list(release(`desktop-v1.0.0`, [DMG]))
    await resolveDesktopDownload(`macos`, deps)
    clock += 5 * 60_000
    answer = list(release(`desktop-v2.0.0`, [DMG]))
    // The stale target answers while the refresh runs.
    expect((await resolveDesktopDownload(`macos`, deps)).url).toContain(`desktop-v1.0.0/`)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect((await resolveDesktopDownload(`macos`, deps)).url).toContain(`desktop-v2.0.0/`)
    expect(calls).toBe(2)
  })

  it(`keeps the last good list when GitHub fails, and backs off`, async () => {
    answer = list(release(`desktop-v1.0.0`, [DMG]))
    await resolveDesktopDownload(`macos`, deps)
    clock += 5 * 60_000
    answer = () => new Response(`nope`, { status: 404 })
    await resolveDesktopDownload(`macos`, deps)
    await new Promise((resolve) => setTimeout(resolve, 0))
    const before = calls
    expect((await resolveDesktopDownload(`macos`, deps)).url).toContain(`desktop-v1.0.0/`)
    expect(calls).toBe(before)
  })

  it(`falls back to the latest link when no list was ever read`, async () => {
    answer = () => new Response(`nope`, { status: 404 })
    expect(await resolveDesktopDownload(`linux`, deps)).toEqual({
      url: `https://github.com/Niach/exponential/releases/latest/download/${APPIMAGE}`,
      fromRelease: false,
    })
  })
})

describe(`isDesktopOs`, () => {
  it(`takes the three desktop OSes only`, () => {
    expect([`macos`, `windows`, `linux`].every(isDesktopOs)).toBe(true)
    expect([`ios`, `constructor`, ``].some(isDesktopOs)).toBe(false)
  })
})
