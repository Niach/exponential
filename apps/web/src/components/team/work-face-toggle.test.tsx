import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  ISSUE_FACE_LABEL,
  RUN_FACE_LABEL,
  RUNS_FACE_LABEL,
  runFaceLabel,
  WorkFaceToggle,
} from "@/components/team/work-face-toggle"
import { RUN_TITLE_CLASS, WorkHeader } from "@exp/ui"
import { ISSUE_TITLE_FIELD_CLASS } from "@/components/issue-title-field"
import {
  MERGE_PR_LABEL,
  RESUME_LABEL,
  STOP_LABEL,
} from "@/components/run-action-pills"

// EXP-877: the unified work header's face toggle — byte-identical labels
// with the IDE (`work_header.rs`), hidden faces rather than disabled ones,
// and no control at all under two faces. EXP-895 moved the diff face's own
// label into `@exp/ui` `DiffCounts` (locked by `diff-counts.test.tsx`, U+2212
// minus and the `--diff-*` token colours).

describe(`WorkFaceToggle`, () => {
  it(`renders nothing under two faces`, () => {
    const { container } = render(
      <WorkFaceToggle
        face="issue"
        items={[{ face: `issue`, label: ISSUE_FACE_LABEL, onSelect: vi.fn() }]}
      />
    )
    expect(container.innerHTML).toBe(``)
  })

  it(`lists only the faces it is given and selects by face`, () => {
    const onRun = vi.fn()
    render(
      <WorkFaceToggle
        face="issue"
        items={[
          { face: `issue`, label: ISSUE_FACE_LABEL, onSelect: vi.fn() },
          { face: `run`, label: RUN_FACE_LABEL, onSelect: onRun },
        ]}
      />
    )
    expect(screen.getByTestId(`work-face-toggle`)).toBeTruthy()
    expect(screen.getByText(`Issue`)).toBeTruthy()
    expect(screen.getByText(`Run`)).toBeTruthy()
    // No diff segment: `DiffCounts`'s `+N −M` (U+2212) is absent.
    expect(screen.queryByText(/^\+\d+ \u2212\d+$/)).toBeNull()
    fireEvent.mouseDown(screen.getByText(`Run`))
    fireEvent.click(screen.getByText(`Run`))
    expect(onRun).toHaveBeenCalled()
  })

})

describe(`WorkHeader`, () => {
  it(`is the fixed band with the title, the trailing cluster and the tray`, () => {
    render(
      <WorkHeader
        title={<h1 className={RUN_TITLE_CLASS}>Chat</h1>}
        trailing={<button type="button">act</button>}
        tray={<div data-testid="tray" />}
      />
    )
    const header = screen.getByTestId(`work-header`)
    expect(header.className).toContain(`shrink-0`)
    expect(header.querySelector(`.max-w-4xl`)).not.toBeNull()
    expect(screen.getByText(`Chat`)).toBeTruthy()
    expect(screen.getByText(`act`)).toBeTruthy()
    expect(screen.getByTestId(`tray`)).toBeTruthy()
  })

  it(`a run title shares the issue title field's size and padding`, () => {
    // The baseline must not move when the face flips: every size/padding
    // token of the Textarea appears on the static title too.
    for (const token of [`text-2xl`, `font-semibold`, `px-5`, `pt-4`, `pb-1`]) {
      expect(RUN_TITLE_CLASS.split(/\s+/)).toContain(token)
      expect(
        ISSUE_TITLE_FIELD_CLASS.split(/\s+/).map((t) => t.replace(/^!/, ``))
      ).toContain(token)
    }
  })
})

describe(`shared strings (×2 with the IDE)`, () => {
  it(`are byte-identical`, () => {
    expect(ISSUE_FACE_LABEL).toBe(`Issue`)
    expect(RUN_FACE_LABEL).toBe(`Run`)
    // EXP-886: the plural once the issue has more than one run of mine.
    expect(RUNS_FACE_LABEL).toBe(`Runs`)
    expect(runFaceLabel(false)).toBe(`Run`)
    expect(runFaceLabel(true)).toBe(`Runs`)
    expect(STOP_LABEL).toBe(`Stop`)
    expect(RESUME_LABEL).toBe(`Resume`)
    expect(MERGE_PR_LABEL).toBe(`Merge PR`)
  })
})
