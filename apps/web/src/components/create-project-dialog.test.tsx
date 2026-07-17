import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { CreateProjectDialog } from "@/components/create-project-dialog"
import { OWNER_ONLY_PUBLIC_HINT } from "@/components/project-form-fields"
import type { Workspace } from "@/db/schema"

const mockState = vi.hoisted(() => ({
  createProject: vi.fn(),
  isOwner: true,
}))

vi.mock(`@/hooks/use-create-project`, () => ({
  useCreateProject: () => ({ createProject: mockState.createProject }),
}))

vi.mock(`@/hooks/use-workspace-permissions`, () => ({
  useWorkspacePermissions: () => ({ isOwner: mockState.isOwner }),
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
    mockState.isOwner = true
  })

  it(`renders quickstart presets and the form together (no wizard step)`, () => {
    renderDialog()

    expect(screen.getByText(`Quickstart`)).toBeTruthy()
    expect(screen.getByRole(`button`, { name: /Dev board/ })).toBeTruthy()
    // Form fields visible without picking a template first.
    expect(screen.getByLabelText(`Name`)).toBeTruthy()
    expect(screen.getByLabelText(`Prefix`)).toBeTruthy()
    expect(screen.getByRole(`switch`)).toBeTruthy()
    expect(screen.getByRole(`button`, { name: `Create project` })).toBeTruthy()
  })

  it(`applies feedback quickstart presets without clobbering a typed name`, () => {
    renderDialog()

    fireEvent.change(screen.getByLabelText(`Name`), {
      target: { value: `My App` },
    })
    expect(
      (screen.getByLabelText(`Prefix`) as HTMLInputElement).value
    ).toBe(`MA`)

    const publicSwitch = screen.getByRole(`switch`)
    expect(publicSwitch.getAttribute(`aria-checked`)).toBe(`false`)

    fireEvent.click(screen.getByRole(`button`, { name: /Feedback board/ }))

    expect(publicSwitch.getAttribute(`aria-checked`)).toBe(`true`)
    // Presets never touch typed name/prefix.
    expect((screen.getByLabelText(`Name`) as HTMLInputElement).value).toBe(
      `My App`
    )
    expect(
      (screen.getByLabelText(`Prefix`) as HTMLInputElement).value
    ).toBe(`MA`)
  })

  it(`submits without any quickstart selection`, async () => {
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
      isPublic: false,
      repository: undefined,
    })
  })

  it(`locks the public quickstart and switch for non-owners`, () => {
    mockState.isOwner = false
    renderDialog()

    const feedbackCell = screen.getByRole(`button`, {
      name: /Feedback board/,
    }) as HTMLButtonElement
    expect(feedbackCell.disabled).toBe(true)
    expect(feedbackCell.title).toBe(OWNER_ONLY_PUBLIC_HINT)

    const publicSwitch = screen.getByRole(`switch`) as HTMLButtonElement
    expect(publicSwitch.disabled).toBe(true)
    expect(screen.getByText(OWNER_ONLY_PUBLIC_HINT)).toBeTruthy()
  })
})
