import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { AuthFormShell } from "./auth-form-shell"

describe(`AuthFormShell`, () => {
  it(`heads the form with a title and a description`, () => {
    render(
      <AuthFormShell
        title="Sign in"
        description="Enter the code we sent you"
        footer={<a href="/auth/signup">Create an account</a>}
      >
        <input aria-label="Email" />
      </AuthFormShell>
    )
    expect(screen.getByText(`Sign in`)).toBeTruthy()
    expect(screen.getByText(`Enter the code we sent you`)).toBeTruthy()
    expect(screen.getByLabelText(`Email`)).toBeTruthy()
    expect(screen.getByText(`Create an account`)).toBeTruthy()
  })

  it(`drops the whole header once a flow settles`, () => {
    const { container } = render(
      <AuthFormShell footer={<span>Back</span>}>
        <p>Device connected</p>
      </AuthFormShell>
    )
    expect(container.querySelector(`h1`)).toBeNull()
    expect(container.querySelector(`svg`)).toBeTruthy()
    expect(screen.getByText(`Device connected`)).toBeTruthy()
  })

  it(`always carries the brand mark and the two legal links`, () => {
    const { container } = render(
      <AuthFormShell footer={null}>
        <span />
      </AuthFormShell>
    )
    // EXP-1176: the mark alone, drawn big; the wordmark is gone.
    expect(container.querySelector(`svg`)?.getAttribute(`width`)).toBe(`56`)
    expect(screen.queryByText(`Exponential`)).toBeNull()
    expect(container.querySelector(`[data-slot="card"]`)).toBeNull()
    expect(screen.getByRole(`link`, { name: `Privacy` }).getAttribute(`href`))
      .toBe(`https://exponential.at/privacy/`)
    expect(screen.getByRole(`link`, { name: `Terms` }).getAttribute(`href`))
      .toBe(`https://exponential.at/terms/`)
  })
})
