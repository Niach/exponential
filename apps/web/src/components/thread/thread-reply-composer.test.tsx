import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { CodingSession } from "@/db/schema"

// EXP-1284 (review T9/T10): `Reply in this thread`. A reply to my ended
// session RESUMES it and holds the text until the resume is live; a resume
// that never comes up times out after 30 s — the text comes back and a toast
// offers Retry with the SAME start id. With nothing of mine to resume (or no
// machine to resume on) the reply starts a new session in the thread.

const mocks = vi.hoisted(() => ({
  startSession: vi.fn(),
  failedAction: vi.fn(),
  sendMessage: vi.fn(() => true),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { steer: { startSession: { mutate: mocks.startSession } } },
}))
vi.mock(`@/lib/action-error`, () => ({
  failedAction: mocks.failedAction,
  SKIP_ERROR_TOAST: {},
}))
vi.mock(`@/lib/steer-session-store`, () => ({
  acquireSteerSession: () => ({
    connect: vi.fn(),
    subscribe: () => () => {},
    getSnapshot: () => ({ phase: { kind: `idle` } }),
    sendMessage: mocks.sendMessage,
  }),
}))

import {
  THREAD_REPLY_RESUME_TIMEOUT_MS,
  ThreadReplyComposer,
} from "@/components/thread/thread-reply-composer"

const ended = { id: `r1`, deviceId: `dev-1` } as CodingSession

function type(text: string) {
  const field = screen.getByLabelText(`Reply in this thread`)
  fireEvent.change(field, { target: { value: text } })
  fireEvent.keyDown(field, { key: `Enter` })
}

beforeEach(() => {
  vi.useFakeTimers()
  mocks.startSession.mockReset()
  mocks.startSession.mockResolvedValue({ ok: true })
  mocks.failedAction.mockReset()
  mocks.sendMessage.mockClear()
})

afterEach(() => {
  vi.useRealTimers()
})

describe(`ThreadReplyComposer`, () => {
  it(`resumes my ended session, then gives the text back after 30 s with Retry`, async () => {
    const onStart = vi.fn()
    render(
      <ThreadReplyComposer
        target={{ kind: `resume`, runId: `r1` }}
        resumeRun={ended}
        onStart={onStart}
      />
    )
    type(`ship it`)
    await act(async () => {
      await Promise.resolve()
    })
    expect(mocks.startSession).toHaveBeenCalledTimes(1)
    const sent = mocks.startSession.mock.calls[0]![0]
    expect(sent).toMatchObject({ resumeSessionId: `r1`, deviceId: `dev-1` })
    expect(sent).not.toHaveProperty(`threadId`)
    const field = screen.getByLabelText(`Reply in this thread`) as HTMLTextAreaElement
    expect(field.value).toBe(``)

    await act(async () => {
      vi.advanceTimersByTime(THREAD_REPLY_RESUME_TIMEOUT_MS)
    })
    expect(field.value).toBe(`ship it`)
    expect(mocks.failedAction).toHaveBeenCalledTimes(1)
    expect(mocks.failedAction.mock.calls[0]![0]).toBe(`resume`)
    expect(mocks.failedAction.mock.calls[0]![1]).toBe(`the session`)

    // Retry reuses the start id the machine may already hold.
    const retry = mocks.failedAction.mock.calls[0]![3].retry as () => void
    await act(async () => {
      retry()
      await Promise.resolve()
    })
    expect(mocks.startSession.mock.calls[1]![0].startId).toBe(sent.startId)
    expect(onStart).not.toHaveBeenCalled()
  })

  it(`starts a new session in the thread when there is nothing to resume on`, () => {
    const onStart = vi.fn()
    render(
      <ThreadReplyComposer
        target={{ kind: `resume`, runId: `r1` }}
        resumeRun={{ id: `r1`, deviceId: null } as CodingSession}
        onStart={onStart}
      />
    )
    type(`try again`)
    expect(onStart).toHaveBeenCalledWith(`try again`)
    expect(mocks.startSession).not.toHaveBeenCalled()
  })

  it(`sends straight to my live session`, () => {
    render(
      <ThreadReplyComposer target={{ kind: `send`, runId: `r2` }} resumeRun={null} onStart={vi.fn()} />
    )
    type(`more`)
    expect(mocks.sendMessage).toHaveBeenCalledWith(`more`)
  })
})
