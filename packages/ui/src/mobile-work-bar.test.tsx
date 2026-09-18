import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { MobileWorkBar, MobileWorkCapsule } from "./mobile-work-bar"

// EXP-893: the phone's ONE floating bar — three slots, or an expanded node
// that replaces the left two while the trailing circle stays mounted.

describe(`MobileWorkBar`, () => {
  it(`lays out leading, capsule and trailing`, () => {
    render(
      <MobileWorkBar
        leading={<button data-testid="leading">L</button>}
        capsule={<MobileWorkCapsule data-testid="capsule">C</MobileWorkCapsule>}
        trailing={<button data-testid="trailing">T</button>}
      />
    )
    expect(screen.getByTestId(`leading`)).toBeTruthy()
    expect(screen.getByTestId(`capsule`)).toBeTruthy()
    expect(screen.getByTestId(`trailing`)).toBeTruthy()
  })

  it(`keeps the trailing circle mounted but hidden while expanded`, () => {
    render(
      <MobileWorkBar
        leading={<button data-testid="leading">L</button>}
        capsule={<MobileWorkCapsule data-testid="capsule">C</MobileWorkCapsule>}
        trailing={<button data-testid="trailing">T</button>}
        expanded={<div data-testid="expanded">composer</div>}
      />
    )
    expect(screen.queryByTestId(`leading`)).toBeNull()
    expect(screen.queryByTestId(`capsule`)).toBeNull()
    expect(screen.getByTestId(`expanded`)).toBeTruthy()
    const trailing = screen.getByTestId(`trailing`)
    expect(trailing.parentElement?.className).toContain(`hidden`)
  })

  // EXP-916: the Reviews page's layout — Android's centred cluster, no
  // stretched placeholder where the capsule is missing.
  it(`cluster centres the slots and never pads an empty capsule`, () => {
    render(
      <MobileWorkBar
        cluster
        leading={<button data-testid="leading">L</button>}
        trailing={<button data-testid="trailing">T</button>}
      />
    )
    const bar = screen.getByTestId(`mobile-work-bar`)
    expect(bar.dataset.layout).toBe(`cluster`)
    expect(bar.className).toContain(`justify-center`)
    expect(bar.querySelector(`span.flex-1`)).toBeNull()
  })

  it(`stretch leaves a spacer where the capsule is missing`, () => {
    render(<MobileWorkBar trailing={<button data-testid="trailing">T</button>} />)
    const bar = screen.getByTestId(`mobile-work-bar`)
    expect(bar.dataset.layout).toBe(`stretch`)
    expect(bar.querySelector(`span.flex-1`)).not.toBeNull()
  })

  it(`renders nothing while hidden`, () => {
    const { container } = render(
      <MobileWorkBar
        hidden
        trailing={<button data-testid="trailing">T</button>}
      />
    )
    expect(container.innerHTML).toBe(``)
  })
})
