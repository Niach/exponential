import { describe, expect, it } from 'vitest'
import { isGlobalSearchKey } from './global-search-shortcut'

const press = (key: string, over: Partial<{ metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; defaultPrevented: boolean }> = {}) => ({
  key,
  metaKey: false,
  ctrlKey: false,
  shiftKey: false,
  defaultPrevented: false,
  ...over,
})

// #EXP-2025: find-in-chat.json keyCases, on web's window listener.
describe(`isGlobalSearchKey`, () => {
  it(`opens the global search on Ctrl+K and Cmd+K`, () => {
    expect(isGlobalSearchKey(press(`k`, { ctrlKey: true }))).toBe(true)
    expect(isGlobalSearchKey(press(`k`, { metaKey: true }))).toBe(true)
    expect(isGlobalSearchKey(press(`K`, { ctrlKey: true }))).toBe(true)
  })

  it(`opens it on Ctrl/Cmd+F outside a chat`, () => {
    expect(isGlobalSearchKey(press(`f`, { ctrlKey: true }))).toBe(true)
    expect(isGlobalSearchKey(press(`f`, { metaKey: true }))).toBe(true)
  })

  it(`leaves Ctrl/Cmd+F to an open chat's find in chat`, () => {
    expect(isGlobalSearchKey(press(`f`, { ctrlKey: true, defaultPrevented: true }))).toBe(false)
  })

  it(`keeps Ctrl/Cmd+K global inside a chat (find in chat never takes it)`, () => {
    expect(isGlobalSearchKey(press(`k`, { metaKey: true }))).toBe(true)
  })

  it(`ignores a plain letter`, () => {
    expect(isGlobalSearchKey(press(`k`))).toBe(false)
    expect(isGlobalSearchKey(press(`f`))).toBe(false)
  })
})
