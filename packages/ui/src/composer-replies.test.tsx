import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/session-thread.json"
import { composerReplies } from "@exp/domain-contract/session-tree"
import { ComposerReplies } from "./composer-frame"

// EXP-1211: the quick replies row over the fixture's resolved replies.
describe(`ComposerReplies`, () => {
  const replies = composerReplies(fixture.repliesCases[1].input as never)!

  it(`renders one pill per open reply and sends the tapped one`, () => {
    const onSend = vi.fn()
    const onDismiss = vi.fn()
    render(<ComposerReplies replies={replies} onSend={onSend} onDismiss={onDismiss} />)
    const pills = screen.getAllByTestId(`thread-reply`)
    expect(pills.map((pill) => pill.textContent)).toEqual([
      `Raise the version numbers`,
      `Open the PR`,
    ])
    fireEvent.click(pills[0]!)
    expect(onSend).toHaveBeenCalledWith(`m2`, `a`, `Raise the version numbers`)
    fireEvent.click(screen.getByRole(`button`, { name: fixture.copy.replies.dismiss }))
    expect(onDismiss).toHaveBeenCalledWith(`m2`)
  })

  it(`disables a reply whose send is in flight`, () => {
    render(
      <ComposerReplies replies={replies} onSend={() => {}} onDismiss={() => {}} pending={new Set([`b`])} />
    )
    const pills = screen.getAllByTestId(`thread-reply`)
    expect((pills[1] as HTMLButtonElement).disabled).toBe(true)
    expect((pills[0] as HTMLButtonElement).disabled).toBe(false)
  })
})
