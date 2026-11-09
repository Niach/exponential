import { cleanup, render, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { setWebMcpAppContext } from "@/lib/webmcp/app-context"
import { WebMcpReadTools, WebMcpWriteTools } from "@/components/webmcp-tools"

// EXP-245 WebMCP surface, locked across the @mcp-b/react-webmcp 4 → 6 upgrade:
// the same tool names, input schemas (property names + required set),
// annotations, and the 4.x agent wire format (pretty JSON text; failures as
// `Error: <message>` with isError) are registered on `document.modelContext`.

const mocks = vi.hoisted(() => {
  const collection = (rows: unknown[] = []) => ({
    preload: async () => {},
    toArray: rows,
  })
  return {
    boards: [
      {
        id: `b1`,
        teamId: `t1`,
        slug: `core`,
        name: `Core`,
        deletedAt: null,
      },
    ],
    collection,
    notificationsMarkAllRead: vi.fn(async () => ({})),
  }
})

vi.mock(`@/lib/collections`, () => ({
  agentCollection: mocks.collection(),
  boardCollection: mocks.collection(mocks.boards),
  commentCollection: mocks.collection(),
  issueCollection: mocks.collection(),
  issueLabelCollection: mocks.collection(),
  labelCollection: mocks.collection(),
  notificationCollection: mocks.collection(),
  pullRequestCollection: mocks.collection(),
  pullRequestIssueCollection: mocks.collection(),
  teamMemberCollection: mocks.collection(),
  userCollection: mocks.collection(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    notifications: {
      markAllRead: { mutate: mocks.notificationsMarkAllRead },
      markRead: { mutate: vi.fn() },
    },
  },
}))

interface RegisteredTool {
  name: string
  description: string
  inputSchema?: {
    type?: string
    properties?: Record<string, unknown>
    required?: string[]
  }
  annotations?: Record<string, unknown>
  execute: (input: unknown, options: { signal: AbortSignal }) => Promise<unknown>
}

let registered: Map<string, RegisteredTool>

beforeEach(() => {
  registered = new Map()
  Object.defineProperty(document, `modelContext`, {
    configurable: true,
    value: {
      registerTool: vi.fn(async (tool: RegisteredTool, options?: { signal?: AbortSignal }) => {
        registered.set(tool.name, tool)
        options?.signal?.addEventListener(`abort`, () => registered.delete(tool.name))
      }),
    },
  })
  setWebMcpAppContext({
    teamId: `t1`,
    teamSlug: `acme`,
    teamName: `Acme`,
    boardSlug: `core`,
    issueIdentifier: null,
    userId: `u1`,
    userName: `Ada`,
    userEmail: `ada@example.com`,
    isMember: true,
    navigate: vi.fn(),
  })
})

afterEach(() => {
  cleanup()
  setWebMcpAppContext(null)
  delete (document as { modelContext?: unknown }).modelContext
})

const READ = { readOnlyHint: true, idempotentHint: true }
const IDEMPOTENT = { idempotentHint: true }

// name → [input properties, required, annotations]
const EXPECTED_READ: Record<string, [string[], string[], object | undefined]> = {
  get_context: [[], [], READ],
  list_boards: [[], [], READ],
  list_issues: [[`board`, `statuses`, `assignee`, `limit`], [], READ],
  search_issues: [[`query`, `limit`], [`query`], READ],
  get_issue: [[`issue`], [], READ],
  list_notifications: [[`unreadOnly`], [], READ],
  navigate: [[`to`, `board`, `issue`], [`to`], IDEMPOTENT],
}

const EXPECTED_WRITE: Record<string, [string[], string[], object | undefined]> = {
  create_issue: [
    [`board`, `title`, `description`, `status`, `priority`, `assignee`, `dueDate`, `labels`],
    [`title`],
    undefined,
  ],
  update_issue: [
    [`issue`, `title`, `status`, `priority`, `assignee`, `description`, `dueDate`],
    [],
    undefined,
  ],
  comment_on_issue: [[`issue`, `body`], [`body`], undefined],
  change_issue_labels: [[`issue`, `add`, `remove`], [], undefined],
  set_issue_subscription: [[`issue`, `subscribed`], [`subscribed`], IDEMPOTENT],
  mark_notifications_read: [[`id`], [], IDEMPOTENT],
}

function expectSurface(expected: Record<string, [string[], string[], object | undefined]>) {
  for (const [name, [properties, required, annotations]] of Object.entries(expected)) {
    const tool = registered.get(name)
    expect(tool, name).toBeDefined()
    expect(tool!.description.length, name).toBeGreaterThan(0)
    expect(tool!.annotations, name).toEqual(annotations)
    expect(tool!.inputSchema?.type, name).toBe(`object`)
    expect(Object.keys(tool!.inputSchema?.properties ?? {}), name).toEqual(properties)
    expect(tool!.inputSchema?.required ?? [], name).toEqual(required)
  }
}

const call = (name: string, input: unknown) =>
  registered.get(name)!.execute(input, { signal: new AbortController().signal })

describe(`WebMCP tool registrations`, () => {
  it(`registers the read tools with their names, schemas and annotations`, async () => {
    render(<WebMcpReadTools />)
    await waitFor(() => expect(registered.size).toBe(Object.keys(EXPECTED_READ).length))
    expect([...registered.keys()].sort()).toEqual(Object.keys(EXPECTED_READ).sort())
    expectSurface(EXPECTED_READ)
  })

  it(`registers the write tools with their names, schemas and annotations`, async () => {
    render(<WebMcpWriteTools />)
    await waitFor(() => expect(registered.size).toBe(Object.keys(EXPECTED_WRITE).length))
    expect([...registered.keys()].sort()).toEqual(Object.keys(EXPECTED_WRITE).sort())
    expectSurface(EXPECTED_WRITE)
  })

  it(`keeps the enum and bounds in the published schemas`, async () => {
    render(<WebMcpReadTools />)
    await waitFor(() => expect(registered.has(`navigate`)).toBe(true))
    const props = (name: string) =>
      registered.get(name)!.inputSchema!.properties as Record<string, Record<string, unknown>>
    expect(props(`navigate`).to.enum).toEqual([`board`, `issue`, `inbox`, `chats`])
    expect(props(`list_issues`).limit).toMatchObject({ type: `integer`, minimum: 1, maximum: 100 })
    expect(props(`search_issues`).query).toMatchObject({ type: `string`, minLength: 1, maxLength: 256 })
  })

  it(`answers agents in the 4.x wire format`, async () => {
    render(<WebMcpReadTools />)
    await waitFor(() => expect(registered.has(`get_context`)).toBe(true))
    const result = (await call(`get_context`, {})) as {
      content: { type: string; text: string }[]
      isError?: boolean
    }
    expect(result.isError).toBeUndefined()
    expect(result.content).toHaveLength(1)
    expect(result.content[0].type).toBe(`text`)
    const text = result.content[0].text
    expect(text).toContain(`\n  "team": {`)
    expect(JSON.parse(text)).toMatchObject({
      team: { slug: `acme`, name: `Acme` },
      board: { slug: `core`, name: `Core` },
      issue: null,
      isMember: true,
    })
  })

  it(`returns handler failures as an Error: text with isError`, async () => {
    render(<WebMcpReadTools />)
    await waitFor(() => expect(registered.has(`get_issue`)).toBe(true))
    expect(await call(`get_issue`, { issue: `EXP-404` })).toEqual({
      content: [
        {
          type: `text`,
          text: `Error: No issue "EXP-404" in this team. Check the identifier (e.g. via list_issues or search_issues).`,
        },
      ],
      isError: true,
    })
  })

  it(`rejects invalid input before the handler runs`, async () => {
    render(<WebMcpWriteTools />)
    await waitFor(() => expect(registered.has(`mark_notifications_read`)).toBe(true))
    const result = (await call(`mark_notifications_read`, { id: 42 })) as {
      content: { text: string }[]
      isError: boolean
    }
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toMatch(/^Error: Invalid tool input/)
    expect(mocks.notificationsMarkAllRead).not.toHaveBeenCalled()
  })

  it(`unregisters every tool on unmount`, async () => {
    const view = render(<WebMcpReadTools />)
    await waitFor(() => expect(registered.size).toBe(Object.keys(EXPECTED_READ).length))
    view.unmount()
    expect(registered.size).toBe(0)
  })
})
