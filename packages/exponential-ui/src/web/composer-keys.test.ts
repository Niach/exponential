import { describe, expect, test } from "bun:test"
import { composerMenuKey } from "./composer-keys"

const none = { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }

describe(`#EDE-65 Composer menu keys`, () => {
  test(`the five keys are their own DOM names`, () => {
    expect([`ArrowUp`, `ArrowDown`, `Enter`, `Tab`, `Escape`].map((key) => composerMenuKey({ ...none, key }))).toEqual([`ArrowUp`, `ArrowDown`, `Enter`, `Tab`, `Escape`])
  })
  test(`other keys stay the field's`, () => {
    for (const key of [`a`, `ArrowLeft`, `Backspace`, `Home`, ` `]) expect(composerMenuKey({ ...none, key })).toBeUndefined()
  })
  test(`a held modifier is never intercepted`, () => {
    expect(composerMenuKey({ ...none, shiftKey: true, key: `Enter` })).toBeUndefined()
    expect(composerMenuKey({ ...none, shiftKey: true, key: `Tab` })).toBeUndefined()
    expect(composerMenuKey({ ...none, ctrlKey: true, key: `Enter` })).toBeUndefined()
    expect(composerMenuKey({ ...none, altKey: true, key: `ArrowDown` })).toBeUndefined()
    expect(composerMenuKey({ ...none, metaKey: true, key: `ArrowUp` })).toBeUndefined()
  })
  test(`an IME composition is never intercepted`, () => {
    expect(composerMenuKey({ ...none, key: `Enter`, isComposing: true })).toBeUndefined()
    expect(composerMenuKey({ ...none, key: `Enter`, keyCode: 229 })).toBeUndefined()
  })
})
