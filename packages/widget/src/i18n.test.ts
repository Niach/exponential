// FEED-113 locale resolution + the message table's shape.
import { afterEach, describe, expect, it, vi } from "vitest"
import {
  detectLocale,
  formatWait,
  messagesFor,
  resolveLocale,
  widgetMessages,
} from "./i18n"

afterEach(() => {
  document.documentElement.lang = ``
  vi.unstubAllGlobals()
})

const stubNavigatorLanguage = (language: string) =>
  vi.stubGlobal(`navigator`, { ...navigator, language })

describe(`detectLocale (auto)`, () => {
  it(`reads <html lang> first`, () => {
    stubNavigatorLanguage(`en-US`)
    document.documentElement.lang = `de-AT`
    expect(detectLocale()).toBe(`de`)
    document.documentElement.lang = `en`
    stubNavigatorLanguage(`de-DE`)
    expect(detectLocale()).toBe(`en`)
  })

  it(`falls back to navigator.language`, () => {
    stubNavigatorLanguage(`de-CH`)
    expect(detectLocale()).toBe(`de`)
    stubNavigatorLanguage(`fr-FR`)
    expect(detectLocale()).toBe(`en`)
  })
})

describe(`resolveLocale`, () => {
  it(`setLocale beats the init option beats auto`, () => {
    document.documentElement.lang = `de`
    expect(resolveLocale(null, undefined)).toBe(`de`)
    expect(resolveLocale(null, `en`)).toBe(`en`)
    expect(resolveLocale(`de`, `en`)).toBe(`de`)
    expect(resolveLocale(`auto`, `en`)).toBe(`de`)
    // Junk degrades to auto rather than throwing.
    expect(resolveLocale(`xx` as never, `yy` as never)).toBe(`de`)
  })

  it(`messagesFor picks the table off the runtime state`, () => {
    expect(
      messagesFor({ localeOverride: `de`, options: { locale: `en` } }).submit
    ).toBe(`Feedback senden`)
    expect(messagesFor({ options: { locale: `en` } }).submit).toBe(
      `Send feedback`
    )
  })
})

describe(`message table`, () => {
  it(`German carries every English key`, () => {
    expect(Object.keys(widgetMessages.de).sort()).toEqual(
      Object.keys(widgetMessages.en).sort()
    )
  })
})

describe(`formatWait`, () => {
  it.each([
    [0, `1 second`, `1 Sekunde`],
    [30, `30 seconds`, `30 Sekunden`],
    [59.2, `1 minute`, `1 Minute`],
    [60, `1 minute`, `1 Minute`],
    [450, `8 minutes`, `8 Minuten`],
    [3600, `1 hour`, `1 Stunde`],
    [7201, `3 hours`, `3 Stunden`],
  ])(`%s seconds → "%s" / "%s"`, (seconds, english, german) => {
    expect(formatWait(seconds, widgetMessages.en)).toBe(english)
    expect(formatWait(seconds, widgetMessages.de)).toBe(german)
  })
})
