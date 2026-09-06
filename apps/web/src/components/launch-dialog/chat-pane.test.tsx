import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

// EXP-758: the Chat tab's repository is OPTIONAL (EXP-739), so the picker has
// to be able to walk back — a "No repository" entry that reports the empty
// string the dialog shell and the server read as repo-less. Rendered through
// the mobile branch of `GlassPickerRow`: it is plain buttons, where the
// desktop branch is a Radix Select that jsdom cannot open.

vi.mock(`@/hooks/use-mobile`, () => ({
  useIsMobile: () => true,
}))

import { ChatPane, NO_REPO } from "@/components/launch-dialog/chat-pane"

const repos = [
  { id: `repo-1`, fullName: `niach/exponential` },
  { id: `repo-2`, fullName: `niach/other` },
]

function renderPane(repoId: string, onRepoChange: (value: string) => void) {
  return render(
    <ChatPane
      prompt=""
      onPromptChange={() => {}}
      repoId={repoId}
      onRepoChange={onRepoChange}
      repos={repos}
      teamId="team-1"
    />
  )
}

describe(`ChatPane repository picker`, () => {
  it(`clears a picked repository back to repo-less`, () => {
    const onRepoChange = vi.fn()
    renderPane(`repo-1`, onRepoChange)
    // The row shows what is picked; open it and take the sentinel.
    fireEvent.click(screen.getByText(`niach/exponential`))
    fireEvent.click(screen.getByRole(`button`, { name: `No repository` }))
    expect(onRepoChange).toHaveBeenCalledWith(``)
    // The sentinel never leaks out of the picker.
    expect(onRepoChange).not.toHaveBeenCalledWith(NO_REPO)
  })

  it(`reads as No repository while none is picked, and still picks one`, () => {
    const onRepoChange = vi.fn()
    renderPane(``, onRepoChange)
    fireEvent.click(screen.getByText(`No repository`))
    fireEvent.click(screen.getByRole(`button`, { name: `niach/other` }))
    expect(onRepoChange).toHaveBeenCalledWith(`repo-2`)
  })
})
