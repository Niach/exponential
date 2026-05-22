import {
  ShapeStream,
  isChangeMessage,
  snakeCamelMapper,
  type Message,
  type Offset,
  type Row,
} from "@electric-sql/client"
import type { CompanionConfig } from "./config"
import type { StateHandle } from "./state"
import type { Logger } from "./logger"
import type { Dispatcher, IssueEvent } from "./dispatcher"
import { readBotToken } from "./credentials"

export interface EventSource {
  stop(): Promise<void>
}

interface Args {
  config: CompanionConfig
  state: StateHandle
  log: Logger
  dispatcher: Dispatcher
}

const SHAPE_NAME = `assigned-issues`

// Subset of the issues row we care about. ColumnMapper auto-camelCases.
interface IssueShapeRow extends Row {
  id: string
  identifier: string
  title: string
  projectId: string
  assigneeId: string | null
  archivedAt: string | null
}

function rowEventType(
  msg: Message<IssueShapeRow>,
  botUserId: string
): IssueEvent[`type`] | null {
  if (!isChangeMessage(msg)) return null
  // Server-side filter already scopes to assignee_id = botUserId. Belt-and-
  // suspenders: re-check here so a misconfigured proxy can't push the daemon
  // to work on someone else's issues.
  switch (msg.headers.operation) {
    case `insert`:
      return msg.value.assigneeId === botUserId ? `assigned` : null
    case `update`: {
      const newAssignee = msg.value.assigneeId
      const oldAssignee = (msg.old_value?.assigneeId ?? null) as string | null
      if (newAssignee === botUserId && oldAssignee !== botUserId) return `assigned`
      if (oldAssignee === botUserId && newAssignee !== botUserId) return `unassigned`
      if (newAssignee === botUserId) return `updated`
      return null
    }
    case `delete`:
      return `unassigned`
    default:
      return null
  }
}

export async function startEventSource(args: Args): Promise<EventSource> {
  const { config, state, log, dispatcher } = args
  const token = await readBotToken()

  const resumed = state.loadOffset(SHAPE_NAME)
  const aborter = new AbortController()

  const url = `${config.exponential.baseUrl.replace(/\/$/, ``)}/api/shapes/assigned-issues`
  const stream = new ShapeStream<IssueShapeRow>({
    url,
    offset: (resumed?.offset as Offset | undefined) ?? `-1`,
    handle: resumed?.handle,
    columnMapper: snakeCamelMapper(),
    headers: {
      Authorization: `Bearer ${token}`,
    },
    signal: aborter.signal,
    onError: (err) => {
      log.error({ err: err.message }, `ShapeStream error`)
    },
  })

  const unsubscribe = stream.subscribe(
    (messages) => {
      for (const msg of messages) {
        if (isChangeMessage(msg)) {
          const type = rowEventType(msg, config.exponential.botUserId)
          if (!type) continue
          const row = msg.value
          if (row.archivedAt) {
            dispatcher.enqueue({
              type: `unassigned`,
              issueId: row.id,
              identifier: row.identifier,
              title: row.title,
              projectId: row.projectId,
              assigneeId: null,
            })
            continue
          }
          dispatcher.enqueue({
            type,
            issueId: row.id,
            identifier: row.identifier,
            title: row.title,
            projectId: row.projectId,
            assigneeId: row.assigneeId,
          })
        }
      }
      // Persist offset + handle after every batch so we can resume after
      // a crash without re-receiving the same rows.
      if (stream.shapeHandle) {
        state.saveOffset({
          shapeName: SHAPE_NAME,
          offset: stream.lastOffset,
          handle: stream.shapeHandle,
        })
      }
    },
    (err) => log.error({ err: err.message }, `ShapeStream subscribe error`)
  )

  log.info({ url, resumed: !!resumed }, `ShapeStream connected`)

  return {
    stop: async () => {
      unsubscribe()
      stream.unsubscribeAll()
      aborter.abort()
    },
  }
}
