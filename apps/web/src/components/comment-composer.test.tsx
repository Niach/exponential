import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

// #FEED-219: the composer's field is a synced draft; the shape and the
// tRPC writes are faked here (`synced` = the shape's rows).
const drafts = vi.hoisted(() => ({
  synced: [] as Array<{ key: string; text: string }>,
  upsert: vi.fn(),
  remove: vi.fn(),
  echo: true,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      // Read the key the hook narrows by off its `eq(d.key, key)`.
      let key: string | null = null
      const query = {
        from: () => ({
          where: (fn: (row: { d: { key: string } }) => { args?: Array<{ value?: unknown }> }) => {
            const clause = fn({ d: { key: `key` } })
            key = (clause.args?.[1]?.value as string | undefined) ?? null
            return query
          },
        }),
      }
      const built = build(query)
      if (!built) return { data: undefined }
      return { data: drafts.synced.filter((row) => key === null || row.key === key) }
    },
  }
})
vi.mock(`@/lib/collections`, () => ({
  composerDraftCollection: {
    utils: {
      awaitTxId: () => (drafts.echo ? Promise.resolve(true) : new Promise(() => undefined)),
    },
  },
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    composerDrafts: {
      upsert: { mutate: (input: unknown) => (drafts.upsert(input), Promise.resolve({ txId: 1 })) },
      delete: { mutate: (input: unknown) => (drafts.remove(input), Promise.resolve({ txId: 2 })) },
    },
  },
}))

import { CommentComposer } from "@/components/comment-composer"
import { resetComposerDrafts } from "@/lib/composer-drafts"

// EXP-568 — the composer grew the same four insert affordances the editor rail
// carries, so the action row is a cross-client contract now. The `#` button in
// particular has to respect the autocomplete's token-start rule, or it inserts
// a dead character instead of opening the picker.

function renderComposer(props?: Partial<{ onEmptyBlur: () => void }>) {
  return render(
    <CommentComposer
      issueId="issue-1"
      users={[]}
      onSubmit={vi.fn().mockResolvedValue(undefined)}
      {...props}
    />
  )
}

describe(`CommentComposer action row`, () => {
  it(`offers image, file, issue-ref, emoji and send`, () => {
    renderComposer()
    for (const label of [
      `Add image`,
      `Attach files`,
      `Insert issue reference`,
      `Insert emoji`,
      `Send comment`,
    ]) {
      expect(screen.getByLabelText(label)).toBeTruthy()
    }
  })

  it(`disables send while there is nothing to post`, () => {
    renderComposer()
    const send = screen.getByLabelText(`Send comment`) as HTMLButtonElement
    expect(send.disabled).toBe(true)

    fireEvent.change(screen.getByPlaceholderText(`Leave a reply…`), {
      target: { value: `hi` },
    })
    expect(
      (screen.getByLabelText(`Send comment`) as HTMLButtonElement).disabled
    ).toBe(false)
  })

  it(`inserts a bare # into an empty composer`, () => {
    renderComposer()
    fireEvent.click(screen.getByLabelText(`Insert issue reference`))
    expect(
      (screen.getByPlaceholderText(`Leave a reply…`) as HTMLTextAreaElement)
        .value
    ).toBe(`#`)
  })

  it(`prepends a space when the caret sits against a word`, () => {
    renderComposer()
    const textarea = screen.getByPlaceholderText(
      `Leave a reply…`
    ) as HTMLTextAreaElement
    fireEvent.change(textarea, { target: { value: `see` } })
    textarea.setSelectionRange(3, 3)
    fireEvent.click(screen.getByLabelText(`Insert issue reference`))
    expect(
      (screen.getByPlaceholderText(`Leave a reply…`) as HTMLTextAreaElement)
        .value
    ).toBe(`see #`)
  })

  it(`does not prepend a space after existing whitespace`, () => {
    renderComposer()
    const textarea = screen.getByPlaceholderText(
      `Leave a reply…`
    ) as HTMLTextAreaElement
    fireEvent.change(textarea, { target: { value: `see ` } })
    textarea.setSelectionRange(4, 4)
    fireEvent.click(screen.getByLabelText(`Insert issue reference`))
    expect(
      (screen.getByPlaceholderText(`Leave a reply…`) as HTMLTextAreaElement)
        .value
    ).toBe(`see #`)
  })
})

describe(`CommentComposer onEmptyBlur`, () => {
  it(`fires when focus leaves an empty card`, () => {
    const onEmptyBlur = vi.fn()
    const { container } = renderComposer({ onEmptyBlur })
    const outside = document.createElement(`button`)
    document.body.appendChild(outside)

    fireEvent.blur(screen.getByPlaceholderText(`Leave a reply…`), {
      relatedTarget: outside,
    })
    expect(onEmptyBlur).toHaveBeenCalledTimes(1)
    expect(container).toBeTruthy()
    outside.remove()
  })

  it(`stays put while the card holds text`, () => {
    const onEmptyBlur = vi.fn()
    renderComposer({ onEmptyBlur })
    const textarea = screen.getByPlaceholderText(`Leave a reply…`)
    fireEvent.change(textarea, { target: { value: `draft` } })

    const outside = document.createElement(`button`)
    document.body.appendChild(outside)
    fireEvent.blur(textarea, { relatedTarget: outside })
    expect(onEmptyBlur).not.toHaveBeenCalled()
    outside.remove()
  })

  it(`ignores focus moves inside the card`, () => {
    const onEmptyBlur = vi.fn()
    renderComposer({ onEmptyBlur })
    fireEvent.blur(screen.getByPlaceholderText(`Leave a reply…`), {
      relatedTarget: screen.getByLabelText(`Insert emoji`),
    })
    expect(onEmptyBlur).not.toHaveBeenCalled()
  })

  it(`ignores the blur a file chooser causes`, () => {
    const onEmptyBlur = vi.fn()
    renderComposer({ onEmptyBlur })
    fireEvent.click(screen.getByLabelText(`Attach files`))

    const outside = document.createElement(`button`)
    document.body.appendChild(outside)
    fireEvent.blur(screen.getByPlaceholderText(`Leave a reply…`), {
      relatedTarget: outside,
    })
    expect(onEmptyBlur).not.toHaveBeenCalled()
    outside.remove()
  })
})

// #FEED-219 (composer-draft.json): a tab switch unmounts the issue route, a
// reload starts the page over — the text must come back from the draft, and
// leave once it is posted.
describe(`CommentComposer synced draft`, () => {
  const draft = { target: { kind: `comment` as const, id: `issue-1` }, teamId: `team-1` }
  const reset = () => {
    resetComposerDrafts()
    drafts.synced = []
    drafts.upsert.mockClear()
    drafts.remove.mockClear()
    drafts.echo = true
  }
  const field = () => screen.getByPlaceholderText(`Leave a reply…`) as HTMLTextAreaElement

  it(`restores typed text after an unmount before the write echoes`, async () => {
    reset()
    drafts.echo = false
    const first = render(
      <CommentComposer issueId="issue-1" users={[]} onSubmit={vi.fn()} draft={draft} />
    )
    fireEvent.change(field(), { target: { value: `half typed` } })
    first.unmount()
    // The unmount flushed the pending write at once.
    await waitFor(() =>
      expect(drafts.upsert).toHaveBeenCalledWith({
        teamId: `team-1`,
        key: `comment:issue-1`,
        text: `half typed`,
      })
    )

    render(<CommentComposer issueId="issue-1" users={[]} onSubmit={vi.fn()} draft={draft} />)
    expect(field().value).toBe(`half typed`)
  })

  it(`shows the synced draft on a fresh load`, () => {
    reset()
    drafts.synced = [{ key: `comment:issue-1`, text: `from the phone` }]
    render(<CommentComposer issueId="issue-1" users={[]} onSubmit={vi.fn()} draft={draft} />)
    expect(field().value).toBe(`from the phone`)
  })

  it(`deletes the draft once the comment posts`, async () => {
    reset()
    const onSubmit = vi.fn().mockResolvedValue(undefined)
    render(<CommentComposer issueId="issue-1" users={[]} onSubmit={onSubmit} draft={draft} />)
    fireEvent.change(field(), { target: { value: `ship it` } })
    fireEvent.click(screen.getByLabelText(`Send comment`))
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(`ship it`, [], `team`))
    await waitFor(() => expect(drafts.remove).toHaveBeenCalledWith({ key: `comment:issue-1` }))
    expect(field().value).toBe(``)
  })

  it(`Cancel discards a reply draft`, async () => {
    reset()
    drafts.synced = [{ key: `reply:c-1`, text: `a reply` }]
    const onCancel = vi.fn()
    render(
      <CommentComposer
        issueId="issue-1"
        users={[]}
        onSubmit={vi.fn()}
        onCancel={onCancel}
        draft={{ target: { kind: `reply`, id: `c-1` }, teamId: `team-1` }}
      />
    )
    expect(field().value).toBe(`a reply`)
    fireEvent.click(screen.getByText(`Cancel`))
    expect(onCancel).toHaveBeenCalled()
    await waitFor(() => expect(drafts.remove).toHaveBeenCalledWith({ key: `reply:c-1` }))
  })
})

// SLOP-4 (fixture `reporter-reply.json`): the "Reply to reporter" pill sits in
// the composer's leading row only when the issue's submission carries a
// reporter email, starts OFF, swaps the placeholder while ON and sends
// `audience: 'reporter'`.
describe(`CommentComposer reply-to-reporter toggle`, () => {
  it(`offers no pill without a reporter`, () => {
    renderComposer()
    expect(screen.queryByText(`Reply to reporter`)).toBeNull()
  })

  it(`toggles the placeholder and the audience`, async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined)
    render(
      <CommentComposer
        issueId="issue-1"
        users={[]}
        onSubmit={onSubmit}
        reporter={{ name: `Emma Fischer` }}
      />
    )
    const pill = screen.getByText(`Reply to reporter`).closest(`button`)!
    expect(pill.getAttribute(`aria-pressed`)).toBe(`false`)
    const field = screen.getByPlaceholderText(`Leave a reply…`)
    fireEvent.click(pill)
    expect(pill.getAttribute(`aria-pressed`)).toBe(`true`)
    expect(field.getAttribute(`placeholder`)).toBe(
      `Reply to Emma Fischer… (emailed to them)`
    )
    fireEvent.change(field, { target: { value: `on it` } })
    fireEvent.click(screen.getByLabelText(`Send comment`))
    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith(`on it`, [], `reporter`)
    )
  })

  it(`turns the pill off after a send so a follow-up stays internal`, async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined)
    render(
      <CommentComposer
        issueId="issue-1"
        users={[]}
        onSubmit={onSubmit}
        reporter={{ name: `Emma Fischer` }}
      />
    )
    const pill = screen.getByText(`Reply to reporter`).closest(`button`)!
    fireEvent.click(pill)
    fireEvent.change(screen.getByRole(`textbox`), {
      target: { value: `on it` },
    })
    fireEvent.click(screen.getByLabelText(`Send comment`))
    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith(`on it`, [], `reporter`)
    )
    await waitFor(() => expect(pill.getAttribute(`aria-pressed`)).toBe(`false`))

    fireEvent.change(screen.getByPlaceholderText(`Leave a reply…`), {
      target: { value: `internal note` },
    })
    fireEvent.click(screen.getByLabelText(`Send comment`))
    await waitFor(() =>
      expect(onSubmit).toHaveBeenLastCalledWith(`internal note`, [], `team`)
    )
  })
})

// EXP-1543 (fixtures/composer-send.json): the field stays editable while the
// comment is in flight; a success clears only what went out.
describe(`CommentComposer send in flight (EXP-1543)`, () => {
  function deferredSubmit() {
    let resolve!: () => void
    let reject!: (error: Error) => void
    const onSubmit = vi.fn(
      () =>
        new Promise<void>((res, rej) => {
          resolve = res
          reject = rej
        })
    )
    return { onSubmit, resolve: () => resolve(), reject: (error: Error) => reject(error) }
  }

  function setup() {
    const pending = deferredSubmit()
    render(<CommentComposer issueId="issue-1" users={[]} onSubmit={pending.onSubmit} />)
    const field = screen.getByPlaceholderText(`Leave a reply…`) as HTMLTextAreaElement
    const type = (value: string) => fireEvent.change(field, { target: { value } })
    return { ...pending, field, type }
  }

  it(`text typed while the comment was in flight stays`, async () => {
    const { onSubmit, resolve, field, type } = setup()
    type(`ship it`)
    fireEvent.click(screen.getByLabelText(`Send comment`))
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(`ship it`, [], `team`))
    expect(field.disabled).toBe(false)
    type(`ship it and then the docs`)
    await act(async () => resolve())
    expect(field.value).toBe(`ship it and then the docs`)
  })

  it(`an untouched field clears once the comment lands`, async () => {
    const { onSubmit, resolve, field, type } = setup()
    type(`ship it `)
    fireEvent.click(screen.getByLabelText(`Send comment`))
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(`ship it`, [], `team`))
    await act(async () => resolve())
    await waitFor(() => expect(field.value).toBe(``))
  })

  it(`a failed send keeps what is in the field now`, async () => {
    const { onSubmit, reject, field, type } = setup()
    type(`ship it`)
    fireEvent.click(screen.getByLabelText(`Send comment`))
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    type(`ship it now`)
    await act(async () => reject(new Error(`down`)))
    expect(field.value).toBe(`ship it now`)
  })
})
