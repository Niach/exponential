import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { SessionResultsView } from "./session-results-view"
import { parseSessionResultGroups, type SessionResultEntry } from "./session-results"

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

  // EXP-1128: a full-page capture takes the 4:3 frame, top-cropped under a
  // Tall pill, and opens in the scrolling lightbox; its landscape sibling is
  // untouched.
  it(`frames a tall capture at 4:3, top-cropped with a Tall badge`, () => {
    render(
      <SessionResultsView
        attachmentSrc={(id) => `/api/attachments/${id}`}
        results={[
          entry({ label: `web-mobile`, attachmentId: `tall-1`, width: 780, height: 25094 }),
          entry({ attachmentId: `att-2` }),
        ]}
      />
    )
    const images = Array.from(document.querySelectorAll(`img`))
    expect(images[0].style.height).toBe(`320px`)
    expect(images[0].style.aspectRatio).toBe(`427 / 320`)
    expect(images[0].className).toContain(`object-top`)
    expect(images[0].getAttribute(`data-tall`)).toBe(`true`)
    expect(images[1].className).not.toContain(`object-top`)
    expect(images[1].getAttribute(`data-tall`)).toBeNull()
    expect(screen.getAllByTestId(`session-result-tall`)).toHaveLength(1)
    expect(screen.getByText(`Tall`)).toBeTruthy()

    fireEvent.click(screen.getByTestId(`session-result-tall-1`))
    const scroller = document.querySelector(`[data-testid="preview-tall-scroll"]`)
    expect(scroller).toBeTruthy()
    expect(scroller?.querySelector(`img`)?.getAttribute(`src`)).toBe(
      `/api/attachments/tall-1`
    )
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

// EXP-933: a topic's report text renders above its tiles, through the app's
// renderer when one is passed.
describe(`SessionResultsView report text`, () => {
  it(`renders each topic's text, a text-only topic included`, () => {
    const { getAllByTestId, getByText } = render(
      <SessionResultsView
        groups={parseSessionResultGroups([
          { topic: `Summary`, text: `Did **it**` },
          { topic: `nav`, text: `The nav` },
          { topic: `nav`, label: `web`, attachmentId: `a1`, width: 100, height: 50 },
        ])}
        attachmentSrc={(id) => `/api/attachments/${id}`}
        renderText={(text) => <span>md:{text}</span>}
      />
    )
    expect(getAllByTestId(`session-result-text`)).toHaveLength(2)
    expect(getByText(`md:Did **it**`)).toBeTruthy()
    expect(getAllByTestId(`session-result-a1`)).toHaveLength(1)
  })
})
