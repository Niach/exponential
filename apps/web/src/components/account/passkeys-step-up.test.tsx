import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/step-up.json"
import { PasskeysSection } from "@/components/account/passkeys-section"

// #EXP-2143 follow-up, the web case of fixtures/step-up.json: Add passkey on
// a day-old session opens "Confirm it's you" over the Add dialog, the mailed
// code proves it, and the add runs again by itself with the typed name. No
// sign-out, no error toast.

const mock = vi.hoisted(() => ({
  addPasskey: vi.fn(),
  listUserPasskeys: vi.fn(),
  toastError: vi.fn(),
}))

vi.mock(`sonner`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("sonner")>()),
  toast: { success: vi.fn(), error: mock.toastError },
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { users: { deletePasskey: { mutate: vi.fn() } } },
}))

vi.mock(`@/lib/auth/client`, () => ({
  authClient: {
    passkey: { addPasskey: mock.addPasskey, listUserPasskeys: mock.listUserPasskeys },
  },
}))

const calls: Array<{ path: string; body: unknown }> = []

beforeEach(() => {
  calls.length = 0
  mock.addPasskey.mockReset()
  mock.listUserPasskeys.mockReset()
  mock.toastError.mockReset()
  mock.listUserPasskeys.mockResolvedValue({ data: [], error: null })
  vi.stubGlobal(
    `fetch`,
    vi.fn(async (url: string, init?: RequestInit) => {
      const path = String(url).replace(`/api/auth`, ``)
      const body = init?.body ? JSON.parse(String(init.body)) : undefined
      calls.push({ path, body })
      const answer = (status: number, json: unknown) =>
        new Response(JSON.stringify(json), { status, headers: { "content-type": `application/json` } })
      if (path === `/step-up/methods`) return answer(200, { methods: [`email`, `password`], email: `danny@example.com` })
      if (path === `/step-up/email/send`) return answer(200, { sent: true })
      if (path === `/step-up/email/verify`) {
        return (body as { code: string }).code === `123456`
          ? answer(200, { fresh: true })
          : answer(400, { code: `INVALID_OTP`, message: `That code is not right.` })
      }
      return answer(404, {})
    })
  )
})

describe(`Add passkey on a stale session (step-up.json web case)`, () => {
  it(`confirms it's you with the mailed code, then the add resumes with the typed name`, async () => {
    mock.addPasskey
      .mockResolvedValueOnce({ data: null, error: { code: `SESSION_NOT_FRESH`, message: `Session is not fresh` } })
      .mockResolvedValueOnce({ data: { id: `pk-1` }, error: null })

    render(<PasskeysSection passkeyEnabled waysIn={2} />)
    fireEvent.click(await screen.findByText(`Add passkey`))
    fireEvent.change(screen.getByLabelText(`Name`), { target: { value: `Pixel 9` } })
    fireEvent.click(screen.getAllByRole(`button`, { name: `Add passkey` }).at(-1)!)

    // The sheet, on the first method, with the code already mailed.
    expect(await screen.findByText(fixture.copy.title)).toBeTruthy()
    expect(screen.getByText(`Enter the code we sent to danny@example.com.`)).toBeTruthy()
    await waitFor(() => expect(calls.map((c) => c.path)).toContain(`/step-up/email/send`))
    expect(screen.getByText(fixture.copy.switch.password)).toBeTruthy()

    // A wrong code reads inline; the sheet stays.
    fireEvent.change(screen.getByLabelText(fixture.copy.field.email), { target: { value: `000000` } })
    fireEvent.click(screen.getByRole(`button`, { name: fixture.copy.confirm }))
    expect(await screen.findByText(fixture.errors.INVALID_OTP)).toBeTruthy()

    // The right code: the session is fresh and the add runs again by itself.
    fireEvent.change(screen.getByLabelText(fixture.copy.field.email), { target: { value: `123456` } })
    fireEvent.click(screen.getByRole(`button`, { name: fixture.copy.confirm }))
    await waitFor(() => expect(mock.addPasskey).toHaveBeenCalledTimes(2))
    expect(mock.addPasskey.mock.calls[1]![0]).toEqual({ name: `Pixel 9` })
    await waitFor(() => expect(screen.queryByText(fixture.copy.title)).toBeNull())
    expect(mock.toastError).not.toHaveBeenCalled()
  })

  it(`cancel drops the add quietly and keeps the Add dialog with its name`, async () => {
    mock.addPasskey.mockResolvedValue({ data: null, error: { code: `SESSION_NOT_FRESH` } })
    render(<PasskeysSection passkeyEnabled waysIn={2} />)
    fireEvent.click(await screen.findByText(`Add passkey`))
    fireEvent.change(screen.getByLabelText(`Name`), { target: { value: `Pixel 9` } })
    fireEvent.click(screen.getAllByRole(`button`, { name: `Add passkey` }).at(-1)!)
    expect(await screen.findByText(fixture.copy.title)).toBeTruthy()
    fireEvent.click(screen.getAllByRole(`button`, { name: `Cancel` }).at(-1)!)
    await waitFor(() => expect(screen.queryByText(fixture.copy.title)).toBeNull())
    expect((screen.getByLabelText(`Name`) as HTMLInputElement).value).toBe(`Pixel 9`)
    expect(mock.addPasskey).toHaveBeenCalledTimes(1)
    expect(mock.toastError).not.toHaveBeenCalled()
  })
})
