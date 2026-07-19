import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { CreateProjectDialog } from "@/components/create-project-dialog"
import type { Workspace } from "@/db/schema"

const mockState = vi.hoisted(() => ({
  createProject: vi.fn(),
}))

vi.mock(`@/hooks/use-create-project`, () => ({
  useCreateProject: () => ({ createProject: mockState.createProject }),
}))

vi.mock(`@/lib/runtime-config`, () => ({
  getRuntimeConfig: async () => ({
    creemProProductId: null,
    creemBusinessProductId: null,
    creemBusinessYearlyProductId: null,
  }),
}))

vi.mock(`@/components/connected-repo-picker`, () => ({
  ConnectedRepoPicker: () => <div data-testid="repo-picker" />,
}))

vi.mock(`@/components/upgrade-dialog`, () => ({
  UpgradeDialog: () => null,
}))

const workspace = {
  id: `workspace-1`,
  slug: `acme`,
  name: `Acme`,
} as Workspace

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

function renderDialog() {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  return render(
    <CreateProjectDialog
      open
      onOpenChange={vi.fn()}
      workspace={workspace}
    />
  )
}

describe(`CreateProjectDialog`, () => {
  beforeEach(() => {
    mockState.createProject.mockReset()
  })

  it(`renders the plain form (no quickstart templates, no public switch)`, () => {
    renderDialog()

    expect(screen.getByLabelText(`Name`)).toBeTruthy()
    expect(screen.getByLabelText(`Prefix`)).toBeTruthy()
    expect(screen.getByText(`Repository (optional)`)).toBeTruthy()
    expect(screen.getByRole(`button`, { name: `Create project` })).toBeTruthy()
    // EXP-180: templates and public boards are gone.
    expect(screen.queryByText(`Quickstart`)).toBeNull()
    expect(screen.queryByRole(`switch`)).toBeNull()
    expect(screen.queryByText(/Feedback board/)).toBeNull()
  })

  it(`derives the prefix from the typed name`, () => {
    renderDialog()

    fireEvent.change(screen.getByLabelText(`Name`), {
      target: { value: `My App` },
    })
    expect(
      (screen.getByLabelText(`Prefix`) as HTMLInputElement).value
    ).toBe(`MA`)
  })

  it(`disables submit until a name is typed`, () => {
    renderDialog()

    const submit = screen.getByRole(`button`, {
      name: `Create project`,
    }) as HTMLButtonElement
    expect(submit.disabled).toBe(true)

    fireEvent.change(screen.getByLabelText(`Name`), {
      target: { value: `Backend API` },
    })
    expect(submit.disabled).toBe(false)
  })

  it(`submits name/prefix/color/icon without any public flag`, async () => {
    mockState.createProject.mockResolvedValue({ ok: true })
    renderDialog()

    fireEvent.change(screen.getByLabelText(`Name`), {
      target: { value: `Backend API` },
    })
    fireEvent.click(screen.getByRole(`button`, { name: `Create project` }))

    await waitFor(() => {
      expect(mockState.createProject).toHaveBeenCalledTimes(1)
    })
    expect(mockState.createProject).toHaveBeenCalledWith({
      workspaceId: `workspace-1`,
      name: `Backend API`,
      prefix: `BA`,
      color: `#6366f1`,
      icon: `code`,
      repository: undefined,
    })
  })
})
