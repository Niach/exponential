// EXP-569 launcher resolution + placement: the pure logic both renders (the
// loader's standalone button and the bundle's Preact button) share.
import { afterEach, describe, expect, it, vi } from "vitest"
import {
  defaultLauncher,
  isMobileViewport,
  launcherButtonClass,
  launcherOrigin,
  launcherPlacementCss,
  mobileMediaQuery,
  panelSideOffset,
  resolveLauncher,
  sanitizeIconSvg,
  watchMobileViewport,
} from "./launcher"
import type { WidgetRemoteConfig } from "./types"

afterEach(() => {
  vi.unstubAllGlobals()
})

const config = (form: Record<string, unknown>): WidgetRemoteConfig =>
  ({ enabled: true, form }) as never

describe(`resolveLauncher`, () => {
  it(`falls back to the per-device defaults`, () => {
    expect(resolveLauncher({ key: `k` }, null, false)).toEqual({
      ...defaultLauncher.desktop,
      iconSvg: null,
    })
    expect(resolveLauncher({ key: `k` }, null, true)).toEqual({
      ...defaultLauncher.mobile,
      iconSvg: null,
    })
    // The defaults themselves are part of the server contract.
    expect(defaultLauncher.desktop).toEqual({
      mode: `fab`,
      position: `bottom-right`,
    })
    expect(defaultLauncher.mobile).toEqual({
      mode: `tab`,
      position: `middle-right`,
    })
  })

  it(`uses the served launcher per device`, () => {
    const served = config({
      launcher: {
        desktop: { mode: `fab`, position: `top-left` },
        mobile: { mode: `tab`, position: `bottom-left` },
      },
    })
    expect(resolveLauncher({ key: `k` }, served, false)).toMatchObject({
      mode: `fab`,
      position: `top-left`,
    })
    expect(resolveLauncher({ key: `k` }, served, true)).toMatchObject({
      mode: `tab`,
      position: `bottom-left`,
    })
  })

  it(`re-validates served values field by field`, () => {
    const junk = config({
      launcher: {
        desktop: { mode: `pill`, position: `top-left` },
        mobile: `nonsense`,
      },
    })
    // Bad mode degrades alone; the valid position survives.
    expect(resolveLauncher({ key: `k` }, junk, false)).toMatchObject({
      mode: `fab`,
      position: `top-left`,
    })
    expect(resolveLauncher({ key: `k` }, junk, true)).toMatchObject(
      defaultLauncher.mobile
    )
  })

  // EXP-672: the served legacy `position` read shim is gone (every stored
  // row carries `launcher`) — a stray field from an old server is ignored.
  it(`ignores a legacy served position`, () => {
    const legacy = config({ position: `bottom-left` })
    expect(resolveLauncher({ key: `k` }, legacy, false)).toMatchObject(
      defaultLauncher.desktop
    )
    expect(resolveLauncher({ key: `k` }, legacy, true)).toMatchObject(
      defaultLauncher.mobile
    )
    const both = config({
      position: `bottom-left`,
      launcher: {
        desktop: { mode: `tab`, position: `middle-left` },
        mobile: { mode: `tab`, position: `middle-right` },
      },
    })
    expect(resolveLauncher({ key: `k` }, both, false)).toMatchObject({
      mode: `tab`,
      position: `middle-left`,
    })
  })

  it(`init launcher fields win over everything, per field`, () => {
    const served = config({
      launcher: {
        desktop: { mode: `tab`, position: `top-left` },
        mobile: { mode: `tab`, position: `middle-right` },
      },
    })
    const resolved = resolveLauncher(
      { key: `k`, launcher: { desktop: { position: `bottom-right` } } },
      served,
      false
    )
    // Overridden position, served mode kept.
    expect(resolved).toMatchObject({ mode: `tab`, position: `bottom-right` })
  })

  it(`a legacy init position maps to a fab on both devices`, () => {
    const served = config({
      launcher: {
        desktop: { mode: `tab`, position: `top-left` },
        mobile: { mode: `tab`, position: `middle-right` },
      },
    })
    const options = { key: `k`, position: `bottom-right` } as const
    expect(resolveLauncher(options, served, false)).toMatchObject({
      mode: `fab`,
      position: `bottom-right`,
    })
    expect(resolveLauncher(options, served, true)).toMatchObject({
      mode: `fab`,
      position: `bottom-right`,
    })
  })

  it(`passing launcher disables the legacy init position entirely`, () => {
    // The cache-skew bridge: new hosts send both, new loaders ignore
    // `position` even for the device the launcher option doesn't mention.
    const resolved = resolveLauncher(
      {
        key: `k`,
        position: `bottom-left`,
        launcher: { desktop: { mode: `fab`, position: `bottom-right` } },
      },
      null,
      true
    )
    expect(resolved).toMatchObject(defaultLauncher.mobile)
  })

  it(`carries the served iconSvg through sanitization`, () => {
    const svg = `<svg viewBox="0 0 24 24"><path d="M1 1"/></svg>`
    const served = config({
      launcher: {
        desktop: { mode: `fab`, position: `bottom-right` },
        mobile: { mode: `tab`, position: `middle-right` },
        iconSvg: svg,
      },
    })
    expect(resolveLauncher({ key: `k` }, served, false).iconSvg).toBe(svg)
    const hostile = config({
      launcher: { iconSvg: `<svg><script>alert(1)</script></svg>` },
    })
    expect(resolveLauncher({ key: `k` }, hostile, false).iconSvg).toBeNull()
  })
})

describe(`launcherPlacementCss`, () => {
  it(`fab keeps 20px side margins, both edges clear the safe area`, () => {
    expect(
      launcherPlacementCss({ mode: `fab`, position: `bottom-right` })
    ).toBe(
      `bottom:20px;bottom:calc(20px + env(safe-area-inset-bottom, 0px));right:20px;`
    )
    expect(launcherPlacementCss({ mode: `fab`, position: `top-left` })).toBe(
      `top:40px;top:calc(40px + env(safe-area-inset-top, 0px));left:20px;`
    )
  })

  it(`middle positions center via a wrapper transform`, () => {
    expect(
      launcherPlacementCss({ mode: `fab`, position: `middle-left` })
    ).toBe(`top:50%;transform:translateY(-50%);left:20px;`)
  })

  it(`tabs sit flush against the screen edge`, () => {
    expect(
      launcherPlacementCss({ mode: `tab`, position: `middle-right` })
    ).toBe(`top:50%;transform:translateY(-50%);right:0;`)
    expect(launcherPlacementCss({ mode: `tab`, position: `top-left` })).toBe(
      `top:40px;top:calc(40px + env(safe-area-inset-top, 0px));left:0;`
    )
  })
})

describe(`launcherOrigin / launcherButtonClass / panelSideOffset`, () => {
  it(`the hover scale grows away from the anchored edges`, () => {
    expect(launcherOrigin(`bottom-right`)).toBe(`bottom right`)
    expect(launcherOrigin(`top-left`)).toBe(`top left`)
    expect(launcherOrigin(`middle-right`)).toBe(`center right`)
  })

  it(`tab classes carry the edge side`, () => {
    expect(launcherButtonClass({ mode: `fab`, position: `top-left` })).toBe(
      `exp-fab`
    )
    expect(
      launcherButtonClass({ mode: `tab`, position: `middle-right` })
    ).toBe(`exp-fab exp-tab exp-tab-right`)
    expect(launcherButtonClass({ mode: `tab`, position: `bottom-left` })).toBe(
      `exp-fab exp-tab exp-tab-left`
    )
  })

  it(`the panel clears a middle launcher horizontally`, () => {
    expect(panelSideOffset({ mode: `fab`, position: `bottom-right` })).toBe(
      `20px`
    )
    expect(panelSideOffset({ mode: `tab`, position: `middle-right` })).toBe(
      `48px`
    )
    expect(panelSideOffset({ mode: `fab`, position: `middle-left` })).toBe(
      `76px`
    )
  })
})

describe(`sanitizeIconSvg`, () => {
  it(`accepts server-generated lucide markup`, () => {
    const svg = `<svg aria-hidden="true" xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 6a13 13 0 0 0 8.4-2.8"/><circle cx="12" cy="12" r="3"/></svg>`
    expect(sanitizeIconSvg(svg)).toBe(svg)
  })

  it(`rejects anything script-shaped or overweight`, () => {
    expect(sanitizeIconSvg(`<svg><script>x</script></svg>`)).toBeNull()
    expect(sanitizeIconSvg(`<svg onload="x"><path/></svg>`)).toBeNull()
    expect(sanitizeIconSvg(`<svg><a href="javascript:x">x</a></svg>`)).toBeNull()
    expect(sanitizeIconSvg(`<svg><use href="#x"/></svg>`)).toBeNull()
    expect(sanitizeIconSvg(`<svg><image x="1"/></svg>`)).toBeNull()
    expect(sanitizeIconSvg(`<svg><foreignObject/></svg>`)).toBeNull()
    expect(sanitizeIconSvg(`<div>not svg</div>`)).toBeNull()
    expect(sanitizeIconSvg(`<svg>${`x`.repeat(10_001)}</svg>`)).toBeNull()
    expect(sanitizeIconSvg(42)).toBeNull()
    expect(sanitizeIconSvg(null)).toBeNull()
  })
})

describe(`viewport device split`, () => {
  it(`isMobileViewport reads the 767px media query`, () => {
    const matchMedia = vi.fn((query: string) => ({
      matches: query === mobileMediaQuery,
    }))
    vi.stubGlobal(`matchMedia`, matchMedia)
    expect(isMobileViewport()).toBe(true)
    expect(matchMedia).toHaveBeenCalledWith(mobileMediaQuery)
    vi.stubGlobal(`matchMedia`, vi.fn(() => ({ matches: false })))
    expect(isMobileViewport()).toBe(false)
    vi.stubGlobal(`matchMedia`, undefined)
    expect(isMobileViewport()).toBe(false)
  })

  it(`watchMobileViewport subscribes and unsubscribes`, () => {
    const add = vi.fn()
    const remove = vi.fn()
    vi.stubGlobal(
      `matchMedia`,
      vi.fn(() => ({
        matches: false,
        addEventListener: add,
        removeEventListener: remove,
      }))
    )
    const onChange = () => {}
    const unsubscribe = watchMobileViewport(onChange)
    expect(add).toHaveBeenCalledWith(`change`, onChange)
    unsubscribe()
    expect(remove).toHaveBeenCalledWith(`change`, onChange)
  })

  it(`watchMobileViewport is a no-op without matchMedia`, () => {
    vi.stubGlobal(`matchMedia`, undefined)
    expect(() => watchMobileViewport(() => {})()).not.toThrow()
  })
})
