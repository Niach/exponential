import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { avatarHueIndex } from "@exponential_app/eui-react/primitives"
import { USER_AVATAR_SIZES, UserAvatar, avatarHueKey, type UserAvatarSize } from "./user-avatar"

const fixture = JSON.parse(
  readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), `..`, `..`, `exponential-ui`, `fixtures`, `avatar.json`),
    `utf8`
  )
) as {
  keys: Array<{
    case: string
    id?: string
    name?: string
    email?: string
    key: string | null
    hue: number | null
  }>
}

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
    // #EXP-1629: the cross-client rule reads the address's local part, so
    // `ada@` is a hued "AD", the same letters on every client.
    expect(fallback(container).textContent).toBe(`AD`)
  })

  it(`paints the person's hue off their id and degrades without a user`, () => {
    const { container } = render(<UserAvatar user={{ id: `u1`, name: `Ada` }} />)
    expect(fallback(container).getAttribute(`style`)).toContain(`--avatar-`)

    const { container: empty } = render(<UserAvatar user={null} />)
    expect(fallback(empty).textContent).toBe(`?`)
    expect(fallback(empty).className).toContain(`bg-muted`)
  })

  // #EXP-1383 (agent-settings.json `actor`): an agent where a person would
  // be is THE AGENT MARK at the avatar's size: its glyph in its colour on
  // the muted disc, never initials.
  it(`draws an agent as its mark at the avatar's size`, () => {
    const { container } = render(
      <UserAvatar
        size={28}
        user={{ id: `u1`, name: `Ada Lovelace` }}
        agent={{ icon: `bot`, color: `#22c55e` }}
      />
    )
    const disc = container.querySelector(`[data-slot=agent-avatar]`)!
    expect(disc.className).toContain(USER_AVATAR_SIZES[28].root)
    expect(disc.className).toContain(`bg-muted`)
    expect(container.querySelector(`[data-slot=avatar-fallback]`)).toBeNull()
    const glyph = disc.querySelector(`svg`)!
    expect(glyph.getAttribute(`class`)).toContain(USER_AVATAR_SIZES[28].glyph)
    expect(glyph.getAttribute(`style`)).toContain(`color`)
  })

  // #EXP-1754 (fixtures/avatar.json `keys`): the hue key is the id, else the
  // name, else the email (trimmed; blank = absent); nothing = muted grey.
  it(`replays the fixture's hue keys`, () => {
    expect(fixture.keys.length).toBeGreaterThan(0)
    for (const row of fixture.keys) {
      expect(avatarHueKey(row.id, row.name, row.email) ?? null, row.case).toBe(row.key)
      const { container, unmount } = render(
        <UserAvatar user={{ id: row.id, name: row.name, email: row.email }} />
      )
      const node = fallback(container)
      if (row.key === null) {
        expect(node.getAttribute(`style`) ?? ``, row.case).not.toContain(`--avatar-`)
        expect(node.className, row.case).toContain(`bg-muted`)
      } else {
        expect(avatarHueIndex(row.key), row.case).toBe(row.hue)
        expect(node.getAttribute(`style`), row.case).toContain(`--avatar-${row.hue}`)
      }
      unmount()
    }
  })

  it(`takes an explicit hue key over the user's fields`, () => {
    const { container } = render(
      <UserAvatar user={{ name: `Anonymous visitor` }} hueKey={null} />
    )
    expect(fallback(container).className).toContain(`bg-muted`)
    expect(fallback(container).textContent).toBe(`AV`)
  })
})
