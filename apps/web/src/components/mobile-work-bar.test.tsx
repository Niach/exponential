import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { MobileWorkBar, MobileWorkCapsule } from "@/components/mobile-work-bar"

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
