// EXP-630: the provider-agnostic import contract. An adapter (lib/import/
// linear/ today) turns a foreign tracker into ONE `ImportBundle`; the applier
// (lib/import/apply.ts) only ever sees this shape — opaque `key` strings, a
// `boardKey` per issue, ISO timestamps. Everything here is zod because the
// bundle is also a public seam: `imports.ingest` accepts a raw bundle, so a
// script, a CLI or an agent can drive an import with no Linear involved.
//
// The wizard's read model (`ImportPreview`), the operator's decisions
// (`ImportPlan`) and the worker's progress live here too — they are stored
// as jsonb on `import_jobs` and re-parsed on every read.
import { z } from "zod"
import {
  hexColorSchema,
  issuePrioritySchema,
  issueStatusCategorySchema,
  issueStatusSchema,
} from "@exp/db-schema/domain"

export const IMPORT_BUNDLE_VERSION = 1

export const importJobStatusValues = [
  `draft`,
  `previewing`,
  `ready`,
  `running`,
  `completed`,
  `failed`,
  `cancelled`,
] as const
export type ImportJobStatus = (typeof importJobStatusValues)[number]
export const importJobStatusSchema = z.enum(importJobStatusValues)

export const IMPORT_TERMINAL_STATUSES: ReadonlySet<ImportJobStatus> = new Set([
  `completed`,
  `failed`,
  `cancelled`,
])

export const importEntityKindValues = [
  `board`,
  `status`,
  `label`,
  `user`,
  `issue`,
  `comment`,
  `attachment`,
] as const
export type ImportEntityKind = (typeof importEntityKindValues)[number]

const keySchema = z.string().min(1).max(200)
const isoDateSchema = z.string().datetime({ offset: true })
const nameSchema = z.string().min(1).max(255)

export const bundleBoardSchema = z.object({
  key: keySchema,
  name: nameSchema,
  // The source's identifier prefix (Linear team key). Longer than our 4-char
  // cap is fine here — the plan carries the prefix the board is created with.
  prefix: z.string().min(1).max(20),
  icon: z.string().max(64).nullish(),
})

export const bundleStatusSchema = z.object({
  key: keySchema,
  category: issueStatusCategorySchema,
  name: nameSchema,
  color: hexColorSchema,
  // An adapter hint ("this is clearly the duplicate state"); the default
  // plan still matches by category + name and the operator has the last word.
  builtinKey: issueStatusSchema.nullish(),
})

export const bundleLabelSchema = z.object({
  key: keySchema,
  name: nameSchema,
  color: hexColorSchema,
})

export const bundleUserSchema = z.object({
  key: keySchema,
  name: z.string().max(255),
  email: z.string().max(255).nullish(),
  active: z.boolean().optional(),
})

export const bundleCommentSchema = z.object({
  key: keySchema,
  // Null = the source has no author (a bot, a deleted account). Every
  // comment lands under a real Exponential user (comments.author_id is NOT
  // NULL): an unmapped author becomes the importer plus an attribution line.
  authorKey: keySchema.nullish(),
  body: z.string(),
  createdAt: isoDateSchema,
  updatedAt: isoDateSchema.nullish(),
  editedAt: isoDateSchema.nullish(),
  // Threads are ONE level deep here; a reply to a reply re-roots.
  parentKey: keySchema.nullish(),
})

const eventBase = {
  key: keySchema,
  actorKey: keySchema.nullish(),
  createdAt: isoDateSchema,
}

// Only the kinds our timeline folds (lib/activity/fold.ts). Adapters drop
// everything else.
export const bundleEventSchema = z.discriminatedUnion(`type`, [
  z.object({
    ...eventBase,
    type: z.literal(`status_changed`),
    fromStatusKey: keySchema.nullish(),
    toStatusKey: keySchema.nullish(),
  }),
  z.object({
    ...eventBase,
    type: z.literal(`assignee_changed`),
    fromUserKey: keySchema.nullish(),
    toUserKey: keySchema.nullish(),
  }),
  z.object({
    ...eventBase,
    type: z.literal(`priority_changed`),
    from: issuePrioritySchema.nullish(),
    to: issuePrioritySchema.nullish(),
  }),
  z.object({
    ...eventBase,
    type: z.literal(`label_added`),
    labelKey: keySchema,
  }),
  z.object({
    ...eventBase,
    type: z.literal(`label_removed`),
    labelKey: keySchema,
  }),
])

// A file the description or a comment references by an opaque `ref` the
// adapter's fetchAsset understands (a Linear upload URL). `commentKey` null =
// used in the description.
export const bundleAssetSchema = z.object({
  key: keySchema,
  ref: z.string().min(1).max(2000),
  filename: z.string().max(500).nullish(),
  contentType: z.string().max(255).nullish(),
  sizeBytes: z.number().int().nonnegative().nullish(),
  commentKey: keySchema.nullish(),
})

export const bundleIssueSchema = z.object({
  key: keySchema,
  boardKey: keySchema,
  // The source number; preserved when the plan says so, else the trigger
  // allocates a fresh one and `externalRef` keeps the old handle.
  number: z.number().int().positive().nullish(),
  title: z.string().min(1).max(500),
  description: z.string().nullish(),
  externalRef: z.string().min(1).max(100),
  externalUrl: z.string().max(2000).nullish(),
  statusKey: keySchema,
  priority: issuePrioritySchema,
  assigneeKey: keySchema.nullish(),
  creatorKey: keySchema.nullish(),
  createdAt: isoDateSchema,
  updatedAt: isoDateSchema,
  // The source's completion stamp when it has one; the writer applies our
  // own rule (only a completed/cancelled/duplicate target keeps it).
  completedAt: isoDateSchema.nullish(),
  dueDate: z
    .string()
    .regex(/^\d{4}-\d{2}-\d{2}$/)
    .nullish(),
  labelKeys: z.array(keySchema).default([]),
  duplicateOfKey: keySchema.nullish(),
  relatedKeys: z.array(keySchema).default([]),
  blocksKeys: z.array(keySchema).default([]),
  comments: z.array(bundleCommentSchema).default([]),
  events: z.array(bundleEventSchema).default([]),
  assets: z.array(bundleAssetSchema).default([]),
})

export const importBundleSchema = z.object({
  version: z.literal(IMPORT_BUNDLE_VERSION),
  // The entity-map namespace (`linear`, `jira`, …): re-importing the same
  // source into the same team creates nothing new.
  source: z.string().regex(/^[a-z][a-z0-9-]{0,31}$/),
  // Human name for attribution lines ("Imported from Linear").
  sourceLabel: z.string().min(1).max(64),
  boards: z.array(bundleBoardSchema),
  statuses: z.array(bundleStatusSchema),
  labels: z.array(bundleLabelSchema),
  users: z.array(bundleUserSchema),
  issues: z.array(bundleIssueSchema),
})

export type ImportBundle = z.infer<typeof importBundleSchema>
export type BundleBoard = z.infer<typeof bundleBoardSchema>
export type BundleStatus = z.infer<typeof bundleStatusSchema>
export type BundleLabel = z.infer<typeof bundleLabelSchema>
export type BundleUser = z.infer<typeof bundleUserSchema>
export type BundleIssue = z.infer<typeof bundleIssueSchema>
export type BundleComment = z.infer<typeof bundleCommentSchema>
export type BundleEvent = z.infer<typeof bundleEventSchema>
export type BundleAsset = z.infer<typeof bundleAssetSchema>

// ---------------------------------------------------------------------------
// Preview — what discovery found, before the operator decides anything.
// `teams[].key` are the board keys issues route to under `team` routing (and
// the fallback boards under `project` routing); `projects[].key` the board
// keys under `project` routing. The adapter owns the key strings; the wizard
// only needs to know which list applies.
// ---------------------------------------------------------------------------

export const importPreviewSchema = z.object({
  sourceLabel: z.string(),
  workspace: z.object({
    name: z.string(),
    url: z.string().nullish(),
  }),
  teams: z.array(
    z.object({
      key: keySchema,
      name: z.string(),
      prefix: z.string(),
      issueCount: z.number().int().nonnegative(),
    })
  ),
  projects: z.array(
    z.object({
      key: keySchema,
      name: z.string(),
      teamKey: keySchema.nullish(),
      issueCount: z.number().int().nonnegative(),
    })
  ),
  statuses: z.array(
    z.object({
      key: keySchema,
      teamKey: keySchema.nullish(),
      name: z.string(),
      category: issueStatusCategorySchema,
      color: hexColorSchema,
      issueCount: z.number().int().nonnegative(),
    })
  ),
  labels: z.array(
    z.object({
      key: keySchema,
      name: z.string(),
      color: hexColorSchema,
      issueCount: z.number().int().nonnegative(),
    })
  ),
  users: z.array(
    z.object({
      key: keySchema,
      name: z.string(),
      email: z.string().nullish(),
      active: z.boolean(),
      issueCount: z.number().int().nonnegative(),
      commentCount: z.number().int().nonnegative(),
    })
  ),
  counts: z.object({
    issues: z.number().int().nonnegative(),
    comments: z.number().int().nonnegative(),
    assets: z.number().int().nonnegative(),
    assetBytes: z.number().int().nonnegative(),
    events: z.number().int().nonnegative(),
  }),
  // Whether the source has a second grouping (Linear projects) issues can
  // route by. False = the routing choice is hidden.
  supportsProjectRouting: z.boolean(),
  warnings: z.array(z.string()),
})
export type ImportPreview = z.infer<typeof importPreviewSchema>

// ---------------------------------------------------------------------------
// Plan — the operator's mapping. Keyed by bundle keys; every entry the
// bundle ends up needing must be present (the dry run says which are not).
// ---------------------------------------------------------------------------

export const importRoutingValues = [`team`, `project`] as const
export type ImportRouting = (typeof importRoutingValues)[number]

export const BOARD_PREFIX_PATTERN = /^[A-Za-z][A-Za-z0-9]{0,3}$/

const numberingSchema = z.enum([`preserve`, `allocate`])

export const boardPlanSchema = z.discriminatedUnion(`mode`, [
  z.object({
    mode: z.literal(`create`),
    name: nameSchema,
    prefix: z.string().trim().regex(BOARD_PREFIX_PATTERN),
    icon: z.string().max(64).nullish(),
    numbering: numberingSchema,
  }),
  z.object({
    mode: z.literal(`existing`),
    boardId: z.string().uuid(),
    numbering: numberingSchema,
  }),
  z.object({ mode: z.literal(`skip`) }),
])

export const statusPlanSchema = z.discriminatedUnion(`mode`, [
  z.object({ mode: z.literal(`builtin`), builtinKey: issueStatusSchema }),
  z.object({ mode: z.literal(`existing`), statusId: z.string().uuid() }),
  z.object({
    mode: z.literal(`create`),
    name: nameSchema,
    color: hexColorSchema,
    category: issueStatusCategorySchema,
  }),
])

export const labelPlanSchema = z.discriminatedUnion(`mode`, [
  z.object({ mode: z.literal(`create`) }),
  z.object({ mode: z.literal(`existing`), labelId: z.string().uuid() }),
  z.object({ mode: z.literal(`skip`) }),
])

export const userPlanSchema = z.discriminatedUnion(`mode`, [
  z.object({ mode: z.literal(`member`), userId: z.string().min(1) }),
  // Sends a team invite to the source email; content is attributed to the
  // importer until (and after — imports never rewrite) they accept.
  z.object({ mode: z.literal(`invite`) }),
  z.object({ mode: z.literal(`self`) }),
])

export const importPlanSchema = z.object({
  routing: z.enum(importRoutingValues),
  importHistory: z.boolean(),
  boards: z.record(keySchema, boardPlanSchema),
  statuses: z.record(keySchema, statusPlanSchema),
  labels: z.record(keySchema, labelPlanSchema),
  users: z.record(keySchema, userPlanSchema),
})
export type ImportPlan = z.infer<typeof importPlanSchema>
export type BoardPlan = z.infer<typeof boardPlanSchema>
export type StatusPlan = z.infer<typeof statusPlanSchema>
export type LabelPlan = z.infer<typeof labelPlanSchema>
export type UserPlan = z.infer<typeof userPlanSchema>

// ---------------------------------------------------------------------------
// Worker read model.
// ---------------------------------------------------------------------------

export const importPhaseValues = [
  `discovering`,
  `users`,
  `boards`,
  `statuses`,
  `labels`,
  `issues`,
  `links`,
  `done`,
] as const
export type ImportPhase = (typeof importPhaseValues)[number]

export const importProgressSchema = z.object({
  phase: z.enum(importPhaseValues),
  done: z.number().int().nonnegative(),
  total: z.number().int().nonnegative(),
  warnings: z.array(z.string()),
  // Storage keys uploaded for a batch whose transaction has not committed
  // yet — reclaimed by a resume when the previous owner died in between.
  pendingKeys: z.array(z.string()).optional(),
})
export type ImportProgress = z.infer<typeof importProgressSchema>

export const importCountsSchema = z.object({
  boards: z.number().int().nonnegative(),
  statuses: z.number().int().nonnegative(),
  labels: z.number().int().nonnegative(),
  invites: z.number().int().nonnegative(),
  issues: z.number().int().nonnegative(),
  comments: z.number().int().nonnegative(),
  attachments: z.number().int().nonnegative(),
  events: z.number().int().nonnegative(),
  relations: z.number().int().nonnegative(),
  skippedIssues: z.number().int().nonnegative(),
})
export type ImportCounts = z.infer<typeof importCountsSchema>

export function emptyImportCounts(): ImportCounts {
  return {
    boards: 0,
    statuses: 0,
    labels: 0,
    invites: 0,
    issues: 0,
    comments: 0,
    attachments: 0,
    events: 0,
    relations: 0,
    skippedIssues: 0,
  }
}

export const dryRunResultSchema = z.object({
  blockers: z.array(z.string()),
  warnings: z.array(z.string()),
  counts: z.object({
    boardsToCreate: z.number().int().nonnegative(),
    statusesToCreate: z.number().int().nonnegative(),
    labelsToCreate: z.number().int().nonnegative(),
    invites: z.number().int().nonnegative(),
    issues: z.number().int().nonnegative(),
    alreadyImported: z.number().int().nonnegative(),
    skippedIssues: z.number().int().nonnegative(),
    comments: z.number().int().nonnegative(),
    attachments: z.number().int().nonnegative(),
    assetBytes: z.number().int().nonnegative(),
    events: z.number().int().nonnegative(),
  }),
})
export type DryRunResult = z.infer<typeof dryRunResultSchema>

// Bound on accumulated warnings per job so a pathological source cannot grow
// the progress jsonb without limit.
export const IMPORT_WARNINGS_CAP = 200

export function pushWarning(list: string[], warning: string): void {
  if (list.length < IMPORT_WARNINGS_CAP) list.push(warning)
  else if (list.length === IMPORT_WARNINGS_CAP) {
    list.push(`… further warnings omitted`)
  }
}
