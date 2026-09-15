import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { USER_AVATAR_SIZES, UserAvatar, type UserAvatarSize } from "./user-avatar"

// EXP-887: every surface that draws a PERSON goes through this one component,
// so the sizes it offers and the initials it falls back to are locked here —
// the sweep replaced ~17 hand-rolled Avatar/AvatarFallback/getInitials trios
// with it and none of them may change size on the way.

function fallback(node: HTMLElement): HTMLElement {
  return node.querySelector(`[data-slot=avatar-fallback]`)!
}

describe(`UserAvatar`, () => {
  it(`offers exactly the sizes the app draws, each with its own type scale`, () => {
    // Reordering or renaming a key silently resizes a call site.
    expect(Object.keys(USER_AVATAR_SIZES)).toEqual([
      `16`,
      `20`,
      `24`,
      `28`,
      `32`,
      `48`,
    ])
    for (const size of Object.keys(USER_AVATAR_SIZES)) {
      const { container } = render(
        <UserAvatar
          user={{ id: `u1`, name: `Ada Lovelace` }}
          size={Number(size) as UserAvatarSize}
        />
      )
      const scale = USER_AVATAR_SIZES[Number(size) as UserAvatarSize]
      const root = container.querySelector(`[data-slot=avatar]`)!
      expect(root.className).toContain(scale.root)
      expect(fallback(container).className).toContain(scale.fallback)
    }
  })

  it(`defaults to 24px`, () => {
    const { container } = render(<UserAvatar user={{ id: `u1`, name: `Ada` }} />)
    expect(
      container.querySelector(`[data-slot=avatar]`)!.className
    ).toContain(USER_AVATAR_SIZES[24].root)
  })

  it(`falls back to the EMAIL for a name-less account, never a bare "?"`, () => {
    const { container } = render(
      <UserAvatar user={{ id: `u1`, email: `ada@lovelace.dev` }} />
    )
    // `getInitials` splits on spaces, so an address yields its first letter —
    // a hued "A", which is still that person, not a shrug.
    expect(fallback(container).textContent).toBe(`A`)
  })

  it(`paints the person's hue off their id and degrades without a user`, () => {
    const { container } = render(<UserAvatar user={{ id: `u1`, name: `Ada` }} />)
    expect(fallback(container).getAttribute(`style`)).toContain(`--avatar-`)

    const { container: empty } = render(<UserAvatar user={null} />)
    expect(fallback(empty).textContent).toBe(`?`)
    expect(fallback(empty).className).toContain(`bg-muted`)
  })
})
