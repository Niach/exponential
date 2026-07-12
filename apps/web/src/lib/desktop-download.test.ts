import { describe, expect, it } from "vitest"
import {
  DESKTOP_ASSET_URLS,
  DESKTOP_RELEASES_URL,
  desktopDownloadHref,
} from "./desktop-download"

// Asset names are a three-way contract: build-desktop.yml publishes them, the
// desktop self-updater (updater::expected_asset_name) downloads them, and
// these links (plus apps/marketing links.ts) point users at them.
describe(`desktopDownloadHref`, () => {
  it(`maps macOS user agents to the .dmg`, () => {
    expect(
      desktopDownloadHref(
        `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36`
      )
    ).toBe(DESKTOP_ASSET_URLS.macos)
    expect(DESKTOP_ASSET_URLS.macos).toMatch(
      /\/latest\/download\/Exponential-production\.dmg$/
    )
  })

  it(`maps Windows user agents to the raw .exe`, () => {
    expect(
      desktopDownloadHref(
        `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36`
      )
    ).toBe(DESKTOP_ASSET_URLS.windows)
    expect(DESKTOP_ASSET_URLS.windows).toMatch(
      /\/latest\/download\/Exponential-production-x86_64-windows\.exe$/
    )
  })

  it(`maps Linux user agents to the AppImage`, () => {
    expect(
      desktopDownloadHref(`Mozilla/5.0 (X11; Linux x86_64) Gecko/20100101`)
    ).toBe(DESKTOP_ASSET_URLS.linux)
    expect(DESKTOP_ASSET_URLS.linux).toMatch(
      /\/latest\/download\/Exponential-production-x86_64\.AppImage$/
    )
  })

  it(`sends mobile and unknown platforms to the releases page`, () => {
    // Android UAs contain "linux" — mobile detection must win.
    expect(
      desktopDownloadHref(
        `Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Mobile`
      )
    ).toBe(DESKTOP_RELEASES_URL)
    expect(
      desktopDownloadHref(
        `Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Mobile/15E148`
      )
    ).toBe(DESKTOP_RELEASES_URL)
    expect(desktopDownloadHref(``)).toBe(DESKTOP_RELEASES_URL)
  })
})
