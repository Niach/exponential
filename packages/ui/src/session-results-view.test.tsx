import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { SessionResultsView } from "./session-results-view"
import type { SessionResultEntry } from "./session-results"

// EXP-879: the results face — one band per topic, one tile per entry, the
// label under the shot and the shared lightbox behind a tap.

const entry = (over: Partial<SessionResultEntry>): SessionResultEntry => ({
  topic: `Issue detail`,
  label: `web`,
  attachmentId: `att-1`,
  width: 1280,
  height: 800,
  ...over,
})

describe(`SessionResultsView`, () => {
  it(`bands the topics and draws one tile per entry`, () => {
    render(
      <SessionResultsView
        attachmentSrc={(id) => `/api/attachments/${id}`}
        results={[
          entry({}),
          entry({ label: `ios`, attachmentId: `att-2` }),
          entry({
            topic: `Board list`,
            label: `web`,
            attachmentId: `att-3`,
            width: null,
            height: null,
          }),
        ]}
      />
    )
    const bands = document.querySelectorAll(`[data-slot="glass-section-header"]`)
    expect(bands.length).toBe(2)
    expect(screen.getByText(`Issue detail`)).toBeTruthy()
    expect(screen.getByText(`Board list`)).toBeTruthy()

    const images = Array.from(document.querySelectorAll(`img`))
    expect(images.map((img) => img.getAttribute(`src`))).toEqual([
      `/api/attachments/att-1`,
      `/api/attachments/att-2`,
      `/api/attachments/att-3`,
    ])
    expect(images[0].getAttribute(`alt`)).toBe(`web`)
    expect(images[0].getAttribute(`loading`)).toBe(`lazy`)
    // Every tile is the same height; the probed aspect gives the width.
    expect(images[0].style.height).toBe(`320px`)
    expect(images[0].style.aspectRatio).toBe(`512 / 320`)
    // Unmeasured shots fall back to a 4:3 frame.
    expect(images[2].style.aspectRatio).toBe(`427 / 320`)
    // The caption repeats the label under the shot.
    expect(screen.getAllByText(`web`).length).toBe(2)
  })

  it(`opens the lightbox on the tapped tile`, () => {
    render(
      <SessionResultsView
        attachmentSrc={(id) => `/api/attachments/${id}`}
        results={[entry({ label: `android` })]}
      />
    )
    expect(document.querySelector(`[role="dialog"]`)).toBeNull()
    fireEvent.click(screen.getByTestId(`session-result-att-1`))
    const dialog = document.querySelector(`[role="dialog"]`)
    expect(dialog).toBeTruthy()
    const preview = dialog?.querySelector(`img`)
    expect(preview?.getAttribute(`src`)).toBe(`/api/attachments/att-1`)
    expect(preview?.getAttribute(`alt`)).toBe(`android`)
  })

  it(`draws nothing without results`, () => {
    render(
      <SessionResultsView
        attachmentSrc={(id) => `/api/attachments/${id}`}
        results={[]}
      />
    )
    expect(document.querySelectorAll(`img`).length).toBe(0)
  })
})
