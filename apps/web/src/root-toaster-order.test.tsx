import { readFileSync } from "node:fs"
import { join } from "node:path"
import { useEffect } from "react"
import { act, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"
import { Toaster, toast } from "@exp/ui"

// EXP-1031: the root mounts `<Toaster />` BEFORE `<Outlet />`. Effects run in
// tree order and sonner's Toaster subscribes in an effect without replaying
// what was published before, so a route that toasts on mount (the OAuth
// return in Settings › Account) is lost when the Toaster comes second.

globalThis.matchMedia ??= ((query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addEventListener() {},
  removeEventListener() {},
  addListener() {},
  removeListener() {},
  dispatchEvent: () => false,
})) as never

function ToastsOnMount({ text }: { text: string }) {
  useEffect(() => {
    toast.info(text)
  }, [text])
  return null
}

afterEach(() => {
  act(() => {
    toast.dismiss()
  })
})

describe(`root Toaster order`, () => {
  it(`__root.tsx renders <Toaster /> before <Outlet />`, () => {
    const source = readFileSync(
      join(import.meta.dirname, `routes`, `__root.tsx`),
      `utf8`
    )
    const body = source.slice(source.indexOf(`function RootComponent`))
    const toaster = body.indexOf(`<Toaster />`)
    const outlet = body.indexOf(`<Outlet />`)
    expect(toaster).toBeGreaterThan(-1)
    expect(outlet).toBeGreaterThan(-1)
    expect(toaster).toBeLessThan(outlet)
  })

  it(`a toast fired in a later sibling's mount effect shows`, async () => {
    render(
      <>
        <Toaster />
        <ToastsOnMount text="shown on first render" />
      </>
    )
    expect(await screen.findByText(`shown on first render`)).toBeTruthy()
  })

  it(`the same toast is lost when the Toaster comes after (why the order matters)`, async () => {
    render(
      <>
        <ToastsOnMount text="lost on first render" />
        <Toaster />
      </>
    )
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50))
    })
    expect(screen.queryByText(`lost on first render`)).toBeNull()
  })
})
