// FEED-131: the Solid Avatar's Radix image-loading behaviour.
import { describe, expect, it } from "vitest"
import { render } from "@solidjs/testing-library"
import { avatarFallbackHue } from "@exponential_app/eui/web/primitives"
import { Avatar, AvatarFallback, AvatarImage } from "./avatar"

describe(`Avatar`, () => {
  it(`paints the fallback while the image has not loaded`, () => {
    const { container } = render(() => (
      <Avatar>
        <AvatarImage src="/never.png" alt="" />
        <AvatarFallback>DS</AvatarFallback>
      </Avatar>
    ))
    expect(container.querySelector(`img`)).toBeNull()
    expect(container.querySelector(`[data-slot="avatar-fallback"]`)!.textContent).toBe(`DS`)
  })

  it(`delayMs holds the fallback back`, async () => {
    const { container } = render(() => (
      <Avatar>
        <AvatarFallback delayMs={20}>DS</AvatarFallback>
      </Avatar>
    ))
    expect(container.querySelector(`[data-slot="avatar-fallback"]`)).toBeNull()
    await new Promise((r) => setTimeout(r, 40))
    expect(container.querySelector(`[data-slot="avatar-fallback"]`)).toBeTruthy()
  })

  it(`a userId paints its hue instead of the muted chrome`, () => {
    const { container } = render(() => (
      <Avatar>
        <AvatarFallback userId="user-42">AB</AvatarFallback>
      </Avatar>
    ))
    const fallback = container.querySelector(`[data-slot="avatar-fallback"]`) as HTMLElement
    expect(fallback.className).not.toContain(`bg-muted`)
    expect(fallback.style.color).toBe(avatarFallbackHue(`user-42`))
  })
})
