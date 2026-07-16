import { describe, expect, it } from "vitest"
import {
  isReadableIframe,
  maskEmailsInText,
  maskEmailsInTree,
  piiMaskPlugin,
} from "./pii-mask"
import type { CaptureContext } from "@zumer/snapdom"

describe(`maskEmailsInText`, () => {
  it(`redacts an email keeping length and the @`, () => {
    const masked = maskEmailsInText(`contact danny@example.com please`)
    expect(masked).toBe(`contact •••••@••••••••••• please`)
    expect(masked).toHaveLength(`contact danny@example.com please`.length)
  })

  it(`redacts multiple emails in one string`, () => {
    const masked = maskEmailsInText(`a@b.co and long.name+tag@sub.domain.org`)
    expect(masked).not.toContain(`a@b.co`)
    expect(masked).not.toContain(`long.name+tag@sub.domain.org`)
    expect(masked.match(/@/g)).toHaveLength(2)
  })

  it(`leaves non-email text untouched`, () => {
    expect(maskEmailsInText(`mention @handle, price 5@10`)).toBe(
      `mention @handle, price 5@10`
    )
    expect(maskEmailsInText(`no at sign here`)).toBe(`no at sign here`)
  })
})

describe(`maskEmailsInTree`, () => {
  it(`masks emails in nested text nodes`, () => {
    const root = document.createElement(`div`)
    root.innerHTML = `<header><span>Signed in as <b>danny@example.com</b></span></header><p>plain text</p>`
    maskEmailsInTree(root)
    expect(root.textContent).not.toContain(`danny@example.com`)
    expect(root.textContent).toContain(`•••••@•••••••••••`)
    expect(root.textContent).toContain(`plain text`)
  })

  it(`masks the whole value of an email input, even partially typed`, () => {
    const root = document.createElement(`div`)
    const input = document.createElement(`input`)
    input.type = `email`
    input.value = `danny@`
    input.setAttribute(`value`, `danny@`)
    root.appendChild(input)
    maskEmailsInTree(root)
    expect(input.value).toBe(`•••••@`)
    expect(input.getAttribute(`value`)).toBe(`•••••@`)
  })

  it(`masks emails inside text inputs and textareas`, () => {
    const root = document.createElement(`div`)
    const input = document.createElement(`input`)
    input.type = `text`
    input.value = `reply to danny@example.com today`
    input.setAttribute(`value`, input.value)
    const textarea = document.createElement(`textarea`)
    textarea.value = `cc: other@example.org`
    root.append(input, textarea)
    maskEmailsInTree(root)
    expect(input.value).toBe(`reply to •••••@••••••••••• today`)
    expect(input.getAttribute(`value`)).toBe(`reply to •••••@••••••••••• today`)
    expect(textarea.value).toBe(`cc: •••••@•••••••••••`)
    expect(textarea.textContent).toBe(`cc: •••••@•••••••••••`)
  })

  it(`leaves non-email input values alone`, () => {
    const root = document.createElement(`div`)
    const input = document.createElement(`input`)
    input.type = `text`
    input.value = `hello world`
    root.appendChild(input)
    maskEmailsInTree(root)
    expect(input.value).toBe(`hello world`)
  })

  it(`skips script and style content`, () => {
    const root = document.createElement(`div`)
    root.innerHTML = `<script>window.email = "danny@example.com"</script><style>/* danny@example.com */</style>`
    maskEmailsInTree(root)
    expect(root.querySelector(`script`)?.textContent).toContain(
      `danny@example.com`
    )
    expect(root.querySelector(`style`)?.textContent).toContain(
      `danny@example.com`
    )
  })

  it(`does not touch nodes outside the given root`, () => {
    const outside = document.createElement(`div`)
    outside.textContent = `danny@example.com`
    document.body.appendChild(outside)
    try {
      const root = document.createElement(`div`)
      root.textContent = `danny@example.com`
      maskEmailsInTree(root)
      expect(root.textContent).not.toContain(`danny@example.com`)
      expect(outside.textContent).toBe(`danny@example.com`)
    } finally {
      outside.remove()
    }
  })
})

describe(`isReadableIframe`, () => {
  it(`flags an attached same-origin iframe`, () => {
    const iframe = document.createElement(`iframe`)
    document.body.appendChild(iframe)
    try {
      expect(isReadableIframe(iframe)).toBe(true)
    } finally {
      iframe.remove()
    }
  })

  it(`ignores non-iframe elements`, () => {
    expect(isReadableIframe(document.createElement(`div`))).toBe(false)
    expect(isReadableIframe(document.body)).toBe(false)
  })

  it(`treats an unreadable iframe document as not readable`, () => {
    const iframe = document.createElement(`iframe`)
    // Cross-origin access throws — the predicate must swallow it and say no.
    Object.defineProperty(iframe, `contentDocument`, {
      get() {
        throw new DOMException(`Blocked a frame`, `SecurityError`)
      },
    })
    Object.defineProperty(iframe, `contentWindow`, {
      get() {
        throw new DOMException(`Blocked a frame`, `SecurityError`)
      },
    })
    expect(isReadableIframe(iframe)).toBe(false)
  })
})

describe(`piiMaskPlugin`, () => {
  it(`masks the cloned tree via afterClone`, async () => {
    const clone = document.createElement(`div`)
    clone.textContent = `Signed in as danny@example.com`
    await piiMaskPlugin.afterClone?.({
      element: document.body,
      clone,
    } as CaptureContext)
    expect(clone.textContent).toBe(`Signed in as •••••@•••••••••••`)
  })

  it(`no-ops when the context has no clone`, async () => {
    await expect(
      Promise.resolve(
        piiMaskPlugin.afterClone?.({ element: document.body } as CaptureContext)
      )
    ).resolves.toBeUndefined()
  })
})
