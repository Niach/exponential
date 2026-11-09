// #EDE-65 lane M: Embed's sandboxed host plugin (embed-host.ts).

import { describe, expect, it } from "bun:test"
import { embedBoxStyle, embedDenied, embedHost } from "./embed-host"
import type { AnyHostPlugin } from "./host-types"

describe(`embedHost`, () => {
  it(`keeps the presentation, forwards events, denies host functions, drops the rest`, async () => {
    const icons = { star: `*` }
    const openUrl = () => {}
    const forward = () => {}
    const host = embedHost({ icons, openUrl, urls: { schemes: [`https`] }, onAction: () => {}, onInput: () => {}, functions: { f: () => 1 }, onFunctionCall: () => `ran`, routeCall: () => ({ value: 1 }), reportFailure: () => {}, surfaceCatalogId: () => `x`, onUpload: () => {}, optionSource: () => [], qrScanner: { start: () => false } } as unknown as AnyHostPlugin, forward)
    expect(Object.keys(host).sort()).toEqual([`icons`, `onAction`, `onFunctionCall`, `openUrl`, `urls`])
    expect(host.icons).toBe(icons)
    expect(host.openUrl).toBe(openUrl)
    expect(host.onAction).toBe(forward)
    expect(await host.onFunctionCall!({ surfaceId: `s`, componentId: `c`, name: `app_toast`, args: {} })).toEqual({ decision: `deny` })
    expect(embedDenied()).toBeInstanceOf(Promise)
  })
  it(`the full width of its slot unless the author placed it`, () => {
    expect(embedBoxStyle(undefined)).toEqual({ alignSelf: `stretch` })
    expect(embedBoxStyle({ width: 200 })).toEqual({ alignSelf: `stretch` })
    expect(embedBoxStyle({ alignSelf: `center` })).toBeUndefined()
  })
})

describe(`the host sheet stops at a nested surface`, () => {
  const { compileNodeSheet } = require(`./box-css`) as typeof import("./box-css")
  const text = { id: `t`, component: `Text`, props: { text: `x` }, children: [], style: { color: `$color.primary` } }
  it(`a tree with an Embed fences every node rule; one without is unchanged`, () => {
    const withEmbed = compileNodeSheet([{ id: `root`, component: `Box`, props: {}, children: [text, { id: `e`, component: `Embed`, props: {}, children: [] }] } as never], `host`).css
    const plain = compileNodeSheet([{ id: `root`, component: `Box`, props: {}, children: [{ ...text }] } as never], `host`).css
    expect(withEmbed).toContain(`:not(.xui-s-host .xui-surface .xui-n-t)`)
    expect(plain).not.toContain(`:not(`)
  })
})
