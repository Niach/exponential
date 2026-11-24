export type View = 'issue' | 'list'

export type Issue = {
  id: string
  identifier: string
  title: string
  status: string
  priority: string
  url?: string | null
  pullRequests?: {
    pullRequestId: string
    number: number
    url: string
    state: string
    status: string
    checks: string
  }[]
  sessions?: {
    sessionId: string
    rootId: string
    status: string
    busy: boolean
    needsInput: boolean
    branch?: string | null
  }[]
}

export type ListRow = {
  id: string
  identifier: string
  title: string
  status: string
  priority: string
}

declare module 'claude-code' {
  interface PluginState {
    'exponential-code': {
      view: View
      pinned: string | null
      issue: Issue | null
      list: ListRow[] | null
      note: string | null
    }
  }
}
