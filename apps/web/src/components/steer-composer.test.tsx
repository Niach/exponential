import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// Wave D: the LIVE steer composer takes any file beside its images — the
// same staging as the launch composer, the same wire shape.

const h = vi.hoisted(() => ({
  upload: vi.fn(),
  toastError: vi.fn(),
}))

vi.mock(`@/lib/storage/issue-image-upload`, () => ({
  uploadSessionImageFile: h.upload,
}))

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@exp/ui")>()),
  toast: { error: h.toastError },
}))

import { SteerComposer } from "@/components/steer-composer"
import { createSteerSessionStore } from "@/lib/steer-session-store"

function makeStore() {
  return createSteerSessionStore(
    `session-1`,
    {
      mintTicket: () => new Promise(() => {}),
      createSocket: () => {
        throw new Error(`no socket in this test`)
      },
    },
    () => {}
  )
}

let urls = 0
beforeEach(() => {
  h.upload.mockReset()
  h.toastError.mockReset()
  vi.stubGlobal(`URL`, Object.assign(URL, {
    createObjectURL: vi.fn(() => `blob:${++urls}`),
    revokeObjectURL: vi.fn(),
  }))
})

afterEach(() => {
  vi.unstubAllGlobals()
})

function mount(onSend = vi.fn(() => true)) {
  const store = makeStore()
  const view = render(
    <SteerComposer
      store={store}
      live
      onSend={onSend}
      working={false}
      sessionId="session-1"
      agent={null}
      config={null}
      users={[]}
    />
  )
  const input = view.container.querySelector(
    `input[type="file"]`
  ) as HTMLInputElement
  const pick = (files: File[]) =>
    act(() => {
      fireEvent.change(input, { target: { files } })
    })
  return { store, input, pick, onSend, view }
}

describe(`SteerComposer files`, () => {
  it(`opens the chooser with no type filter`, () => {
    const { input } = mount()
    expect(input.hasAttribute(`accept`)).toBe(false)
    expect(screen.getByRole(`button`, { name: `Add file or image` })).toBeTruthy()
  })

  it(`stages a file as a named tile with no marker and sends its link`, async () => {
    h.upload
      .mockResolvedValueOnce({ id: `img-1`, filename: `a.png` })
      .mockResolvedValueOnce({ id: `file-1`, filename: `notes.pdf` })
    const { store, pick, onSend, view } = mount()
    act(() => store.setDraftText(`look`))
    pick([
      new File([`x`], `a.png`, { type: `image/png` }),
      new File([`x`], `notes.pdf`, { type: `application/pdf` }),
    ])
    expect(store.getDraftSnapshot().text).toBe(`look [Image #1]`)
    expect(
      view.container.querySelector(`[data-slot="attachment-file-tile"]`)!
        .textContent
    ).toBe(`notes.pdf`)
    fireEvent.keyDown(screen.getByRole(`textbox`), { key: `Enter` })
    await waitFor(() =>
      expect(onSend).toHaveBeenCalledWith(
        `look [Image #1]\n\n![image](/api/attachments/img-1)\n[notes.pdf](/api/attachments/file-1)`
      )
    )
    store.dispose()
  })

  it(`removing a file leaves the image markers alone`, () => {
    const { store, pick } = mount()
    pick([
      new File([`x`], `notes.pdf`, { type: `application/pdf` }),
      new File([`x`], `a.png`, { type: `image/png` }),
    ])
    fireEvent.click(screen.getByRole(`button`, { name: `Remove notes.pdf` }))
    expect(store.getDraftSnapshot().text).toBe(`[Image #1]`)
    expect(store.getDraftSnapshot().images).toHaveLength(1)
    store.dispose()
  })

  it(`toasts the one rejection copy`, () => {
    const { store, pick } = mount()
    const huge = new File([`x`], `huge.zip`, { type: `application/zip` })
    Object.defineProperty(huge, `size`, { value: 51 * 1024 * 1024 })
    pick([huge])
    expect(h.toastError).toHaveBeenCalledWith(
      `Images up to 10 MB and files up to 50 MB can be attached`
    )
    store.dispose()
  })
})
