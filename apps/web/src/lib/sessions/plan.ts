import { TRPCError } from "@trpc/server"
import { and, desc, eq, gt, isNotNull, isNull, sql } from "drizzle-orm"
import {
  CORE_CATALOG_ID,
  reduceNested,
  type NestedNode,
  type ReduceResult,
} from "@exponential_app/ui"
import {
  PLAN_CARD_SURFACE_MAX_BYTES,
  PLAN_CARD_SURFACE_MAX_COMPONENTS,
  PLAN_CARD_UPLOAD_SCHEME,
  type CardSurface,
  type MessageCard,
  type PlanCardPicture,
  type PlanCardQuestion,
} from "@exp/db-schema/domain"
import { PLAN_CARD_COPY, resolveUpload, type AskCardLike } from "@exp/domain-contract/plan-card"
import { db } from "@/db/connection"
import { attachments, messages, sessions } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { askText, busAsk, closeAskMirrors } from "@/lib/sessions/bus"
import { insertMessage, pendingInput, postOwnAskCard } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { mintGuideUploadToken } from "@/lib/storage/guide-upload-token"
import {
  rollbackSessionImage,
  sessionAttachmentValues,
  type PreparedSessionImage,
} from "@/lib/storage/session-attachment-upload"
import { appBaseUrl } from "@/lib/notification-email-policy"

// EXP-1280 custom plan mode: the plan / question card a session posts on
// ITSELF over MCP (`exponential_plan_propose`, `exponential_sessions_ask`;
// the behaviour = packages/domain-contract fixtures/plan-card.json). The
// card is an agent-authored `messages` row with `author_session_id` = the
// session and `consumed_at` NULL: a person answers IN it (`sessions.answer`),
// the device delivers the answer text as the next turn and consumes it.
// Pictures are named by label in the call; each gets a Guide-style signed
// upload (`/api/guide-uploads/$token`, a card target) that files the
// attachment on the card's message and resolves the label in the card.

export interface CardPictureInput {
  label: string
  caption?: string
}

export interface CardOptionInput {
  label: string
  description?: string
  picture?: string
}

export interface PlanQuestionInput {
  id: string
  prompt: string
  multi?: boolean
  custom?: boolean
  options: CardOptionInput[]
}

export interface CardSurfaceInput {
  root: Record<string, unknown>
  data?: Record<string, unknown>
}

export interface PlanProposeInput {
  plan: string
  pictures?: CardPictureInput[]
  questions?: PlanQuestionInput[]
  surface?: CardSurfaceInput
}

export interface SessionsAskInput {
  question: string
  options: CardOptionInput[]
  multi?: boolean
  custom?: boolean
  pictures?: CardPictureInput[]
  surface?: CardSurfaceInput
}

export interface CardUpload {
  label: string
  uploadUrl: string
  curl: string
}

const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

function duplicate(values: readonly string[]): string | null {
  const seen = new Set<string>()
  for (const value of values) {
    if (seen.has(value)) return value
    seen.add(value)
  }
  return null
}

function cardPictures(pictures: readonly CardPictureInput[] | undefined): PlanCardPicture[] {
  const list = pictures ?? []
  const twice = duplicate(list.map((picture) => picture.label))
  if (twice) throw bad(`pictures: the label "${twice}" is declared twice`)
  return list.map((picture) => ({
    label: picture.label,
    attachmentId: null,
    width: null,
    height: null,
    caption: picture.caption?.trim() || null,
  }))
}

function assertDeclared(declared: ReadonlySet<string>, label: string, where: string) {
  if (!declared.has(label)) {
    throw bad(`${where} names the picture "${label}", which is not in pictures: declare it as {label: "${label}"}`)
  }
}

function checkOptions(options: readonly CardOptionInput[], custom: boolean, declared: ReadonlySet<string>, where: string) {
  const twice = duplicate(options.map((option) => option.label.trim()))
  if (twice) throw bad(`${where}: the option "${twice}" is listed twice`)
  if (options.length === 0 && !custom) {
    throw bad(`${where} has no options and custom: false, so it cannot be answered: add options or allow custom`)
  }
  options.forEach((option, index) => {
    if (option.picture) assertDeclared(declared, option.picture, `${where}.options[${index}]`)
  })
}

const UPLOAD_EMBED = new RegExp(`\\]\\(${PLAN_CARD_UPLOAD_SCHEME}([^)\\s]*)\\)`, `g`)

/** Every `](upload:<label>)` embed in a markdown text. */
export function uploadEmbeds(markdown: string): string[] {
  return [...markdown.matchAll(UPLOAD_EMBED)].map((match) => match[1] ?? ``)
}

function countNodes(result: ReduceResult): number {
  type Node = { children?: Node[]; slots?: Record<string, Node> }
  const walk = (node: Node): number =>
    1 +
    (node.children ?? []).reduce((sum, child) => sum + walk(child), 0) +
    Object.values(node.slots ?? {}).reduce((sum, slot) => sum + walk(slot), 0)
  return (
    walk(result.root as Node) +
    Object.values(result.templates ?? {}).reduce((sum, node) => sum + walk(node as Node), 0)
  )
}

/** The inline surface: the core catalog only (no extensions), within the
 *  byte and component caps, with no reducer issue. */
export function validateCardSurface(surface: CardSurfaceInput | undefined): CardSurface | null {
  if (!surface) return null
  const bytes = new TextEncoder().encode(JSON.stringify(surface)).byteLength
  if (bytes > PLAN_CARD_SURFACE_MAX_BYTES) {
    throw bad(`surface is ${bytes} bytes; the cap is ${PLAN_CARD_SURFACE_MAX_BYTES}: simplify it`)
  }
  const root = surface.root
  if (typeof root.id !== `string` || typeof root.component !== `string`) {
    throw bad(`surface.root must be an Exponential UI node: {id, component, props?, children?}`)
  }
  let result: ReduceResult
  try {
    result = reduceNested(root as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
  } catch (error) {
    throw bad(`surface.root is not a valid Exponential UI tree: ${error instanceof Error ? error.message : String(error)}`)
  }
  const count = countNodes(result)
  if (count > PLAN_CARD_SURFACE_MAX_COMPONENTS) {
    throw bad(`surface has ${count} components; the cap is ${PLAN_CARD_SURFACE_MAX_COMPONENTS}`)
  }
  if (result.issues.length > 0) {
    const listed = result.issues.slice(0, 10).map((issue) => `${issue.id}: ${issue.message}`)
    const more = result.issues.length > 10 ? `; and ${result.issues.length - 10} more` : ``
    throw bad(`surface is not valid in the Exponential UI core catalog: ${listed.join(`; `)}${more}`)
  }
  return { root, data: surface.data ?? null }
}

/** `exponential_plan_propose`'s card (pure; BAD_REQUEST names the fix). */
export function buildPlanCard(input: PlanProposeInput): Extract<MessageCard, { kind: `plan` }> {
  const plan = input.plan.trim()
  if (!plan) throw bad(`plan is required: the plan as markdown`)
  const pictures = cardPictures(input.pictures)
  const declared = new Set(pictures.map((picture) => picture.label))
  for (const label of uploadEmbeds(plan)) assertDeclared(declared, label, `plan's ![…](upload:${label})`)
  const questionsInput = input.questions ?? []
  const twice = duplicate(questionsInput.map((question) => question.id.trim()))
  if (twice) throw bad(`questions: the id "${twice}" is used twice`)
  const questions: PlanCardQuestion[] = questionsInput.map((question, index) => {
    const custom = question.custom !== false
    checkOptions(question.options, custom, declared, `questions[${index}]`)
    return {
      id: question.id.trim(),
      prompt: question.prompt.trim(),
      multi: question.multi === true,
      custom,
      options: question.options.map((option) => ({
        label: option.label.trim(),
        description: option.description?.trim() || null,
        picture: option.picture ?? null,
      })),
    }
  })
  return {
    kind: `plan`,
    plan,
    pictures,
    questions,
    surface: validateCardSurface(input.surface),
    decision: null,
    fresh: null,
    feedback: null,
    answers: null,
    surfaceData: null,
  }
}

/** `exponential_sessions_ask`'s card (pure). The sidecars ride only when
 *  some option carries a description / a picture. */
export function buildQuestionCard(input: SessionsAskInput): Extract<MessageCard, { kind: `question` }> {
  const question = input.question.trim()
  if (!question) throw bad(`question is required`)
  const pictures = cardPictures(input.pictures)
  const declared = new Set(pictures.map((picture) => picture.label))
  const custom = input.custom !== false
  checkOptions(input.options, custom, declared, `options`)
  const descriptions = input.options.map((option) => option.description?.trim() || null)
  const optionPictures = input.options.map((option) => option.picture ?? null)
  return {
    kind: `question`,
    question,
    options: input.options.map((option) => option.label.trim()),
    ...(descriptions.some(Boolean) ? { optionDescriptions: descriptions } : {}),
    ...(optionPictures.some(Boolean) ? { optionPictures } : {}),
    pictures,
    multi: input.multi === true,
    custom,
    surface: validateCardSurface(input.surface),
    answer: null,
    answers: null,
    surfaceData: null,
  }
}

export const SURFACE_CARD_TITLE_MAX = 200

/** Chats (fixtures/chats.json `surfaceCard`): `exponential_session_self
 *  {op: "show"}`'s card, an inline preview validated like a plan card's
 *  surface (pure; BAD_REQUEST names the fix). */
export function buildSurfaceCard(input: {
  surface: CardSurfaceInput
  title?: string
}): Extract<MessageCard, { kind: `surface` }> {
  const surface = validateCardSurface(input.surface)
  if (!surface) throw bad(`surface is required: an Exponential UI core tree {root, data?}`)
  const title = input.title?.trim() || null
  if (title && title.length > SURFACE_CARD_TITLE_MAX) {
    throw bad(`title is ${title.length} characters; the cap is ${SURFACE_CARD_TITLE_MAX}`)
  }
  return { kind: `surface`, surface, title }
}

/** Posts a surface card as an AGENT message in the caller's own session
 *  (answers nothing, wakes nothing; `author_session_id` = the session, so
 *  S-3's `replace` finds it). → the message id. */
export async function postSurfaceCard(args: {
  sessionId: string
  card: Extract<MessageCard, { kind: `surface` }>
}): Promise<{ messageId: string }> {
  const messageId = await db.transaction(async (tx) => {
    const [session] = await tx
      .select({
        id: sessions.id,
        rootId: sessions.rootId,
        teamId: sessions.teamId,
        shared: sessions.shared,
        userId: sessions.userId,
        status: sessions.status,
      })
      .from(sessions)
      .where(eq(sessions.id, args.sessionId))
      .limit(1)
    if (!session) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
    if (session.status === `ended`) {
      throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This session has ended` })
    }
    await generateTxId(tx)
    return insertMessage(tx, session, { authorKind: `agent`, authorSessionId: session.id, card: args.card })
  })
  return { messageId }
}

type ShowCard = Extract<MessageCard, { kind: `surface` | `plan` | `question` }>

const isAsk = (card: { kind: string }) => card.kind === `plan` || card.kind === `question`

/**
 * S-3 `exponential_show {replace: true}`: the session's own card of the
 * CURRENT turn (newer than the last input the device delivered to it) is
 * edited in place: the same row, so every client's card updates instead of
 * a new one landing. A card a person already answered is final (refused);
 * a card of the other family (a plain surface vs an ask/plan) or none this
 * turn = a new card. → the edited row, or null to insert.
 */
export async function replaceTarget(
  tx: Tx,
  sessionId: string,
  card: ShowCard
): Promise<{ id: string; card: MessageCard } | null> {
  const [input] = await tx
    .select({ at: messages.createdAt })
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), pendingInput(), isNotNull(messages.consumedAt)))
    .orderBy(desc(messages.createdAt))
    .limit(1)
  const [row] = await tx
    .select({ id: messages.id, card: messages.card, answeredAt: messages.answeredAt })
    .from(messages)
    .where(
      and(
        eq(messages.sessionId, sessionId),
        eq(messages.authorSessionId, sessionId),
        sql`${messages.card}->>'kind' IN ('surface', 'question', 'plan')`,
        ...(input ? [gt(messages.createdAt, input.at)] : [])
      )
    )
    .orderBy(desc(messages.createdAt))
    .limit(1)
    .for(`update`)
  if (!row?.card) return null
  if (row.answeredAt) {
    throw new TRPCError({
      code: `CONFLICT`,
      message: `Your latest card was already answered and is final: post a new one (replace: false).`,
    })
  }
  if (isAsk(row.card) !== isAsk(card)) return null
  return { id: row.id, card: row.card }
}

/** S-3: THE show path (`exponential_show`): a plain surface, a question or
 *  a plan card on the caller's own session; `replace` edits this turn's
 *  card in place (`replaceTarget`), else a new card lands (an ask/plan the
 *  way `postAskCard` posts it). */
export async function postShowCard(args: {
  userId: string
  sessionId: string
  card: ShowCard
  replace: boolean
}): Promise<{ messageId: string; replaced: boolean; uploads: CardUpload[]; next?: string }> {
  const replaced = args.replace
    ? await db.transaction(async (tx) => {
        const [session] = await tx
          .select({ id: sessions.id, status: sessions.status })
          .from(sessions)
          .where(eq(sessions.id, args.sessionId))
          .limit(1)
          .for(`update`)
        if (!session) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
        if (session.status === `ended`) {
          throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This session has ended` })
        }
        const target = await replaceTarget(tx, session.id, args.card)
        if (!target) return null
        await generateTxId(tx)
        const now = new Date()
        await tx.update(messages).set({ card: args.card, updatedAt: now }).where(eq(messages.id, target.id))
        if (isAsk(args.card)) {
          // The parent's / watchers' ask mirrors keep pointing at the row;
          // their text follows (a neutral private line stays as it is).
          const before = askText(session, target.card as Extract<MessageCard, { kind: `question` | `plan` }>)
          await tx
            .update(messages)
            .set({ body: askText(session, args.card as Extract<MessageCard, { kind: `question` | `plan` }>), updatedAt: now })
            .where(
              and(
                eq(messages.authorSessionId, session.id),
                sql`${messages.card}->>'kind' = 'ask'`,
                sql`${messages.card}->>'messageId' = ${target.id}`,
                eq(messages.body, before)
              )
            )
        }
        return target.id
      })
    : null
  if (replaced) {
    return {
      messageId: replaced,
      replaced: true,
      uploads: isAsk(args.card)
        ? mintCardUploads({
            userId: args.userId,
            sessionId: args.sessionId,
            messageId: replaced,
            labels: ((args.card as Extract<MessageCard, { kind: `plan` | `question` }>).pictures ?? []).map(
              (picture) => picture.label
            ),
          })
        : [],
      ...(isAsk(args.card) ? { next: PLAN_CARD_COPY.proposeNext } : {}),
    }
  }
  if (isAsk(args.card)) {
    const posted = await postAskCard({
      userId: args.userId,
      sessionId: args.sessionId,
      card: args.card as Extract<MessageCard, { kind: `plan` | `question` }>,
    })
    return { ...posted, replaced: false }
  }
  const posted = await postSurfaceCard({
    sessionId: args.sessionId,
    card: args.card as Extract<MessageCard, { kind: `surface` }>,
  })
  return { ...posted, replaced: false, uploads: [] }
}

/** One signed upload per picture label of the card. */
export function mintCardUploads(args: {
  userId: string
  sessionId: string
  messageId: string
  labels: readonly string[]
}): CardUpload[] {
  return args.labels.map((label) => {
    const { token } = mintGuideUploadToken({
      sessionId: args.sessionId,
      topic: ``,
      label,
      userId: args.userId,
      card: { messageId: args.messageId },
    })
    const uploadUrl = `${appBaseUrl()}/api/guide-uploads/${token}`
    return {
      label,
      uploadUrl,
      // `--retry` (FEED-75) re-sends on the store's 503 throttling.
      curl: `curl -sS --retry 4 -F file=@${label}.png "${uploadUrl}"`,
    }
  })
}

/**
 * FEED-140: a new `plan` SUPERSEDES the plan this session posted that still
 * waits: the old card closes with decision `superseded` (answered AND
 * consumed, so the device never feeds it to the agent as an answer; plan
 * mode and needs-you stay as they are: the new card waits next). Only
 * TOOL-posted plans: an engine's plan card (it carries the parked ask's
 * `questionId`) stays open for its ask.
 */
export async function supersedeWaitingPlans(tx: Tx, sessionId: string): Promise<string[]> {
  const now = new Date()
  const superseded = await tx
    .update(messages)
    .set({
      card: sql`${messages.card} || '{"decision": "superseded"}'::jsonb`,
      answeredAt: now,
      consumedAt: now,
      updatedAt: now,
    })
    .where(
      and(
        eq(messages.sessionId, sessionId),
        eq(messages.authorSessionId, sessionId),
        isNull(messages.answeredAt),
        sql`${messages.card}->>'kind' = 'plan'`,
        // An ENGINE-posted plan (a parked ExitPlanMode ask, `questionId`)
        // is answered through its ask: superseding it would strand the ask.
        sql`${messages.card}->>'questionId' is null`
      )
    )
    .returning({ id: messages.id })
  const ids = superseded.map((row) => row.id)
  await closeAskMirrors(tx, sessionId, ids, now)
  return ids
}

/**
 * Posts a built plan/question card on the caller's OWN session (the MCP
 * layer resolved it). A live session only; ONE open plan at a time (a
 * second SUPERSEDES the waiting one, FEED-140). → the card's message id +
 * the picture uploads + what to do next.
 */
export async function postAskCard(args: {
  userId: string
  sessionId: string
  card: Extract<MessageCard, { kind: `plan` | `question` }>
}): Promise<{ messageId: string; uploads: CardUpload[]; next: string }> {
  const messageId = await db.transaction(async (tx) => {
    const [session] = await tx
      .select({
        id: sessions.id,
        parentId: sessions.parentId,
        rootId: sessions.rootId,
        teamId: sessions.teamId,
        shared: sessions.shared,
        userId: sessions.userId,
        status: sessions.status,
      })
      .from(sessions)
      .where(eq(sessions.id, args.sessionId))
      .limit(1)
      .for(`update`)
    if (!session) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
    if (session.status === `ended`) {
      throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This session has ended` })
    }
    await generateTxId(tx)
    if (args.card.kind === `plan`) await supersedeWaitingPlans(tx, session.id)
    const messageId = await postOwnAskCard(tx, session, args.card)
    // NEW-1: the card reaches the session's parent and watchers (one row,
    // two views); a parent may answer what it knows.
    await busAsk(tx, session, messageId, args.card)
    return messageId
  })
  return {
    messageId,
    uploads: mintCardUploads({
      userId: args.userId,
      sessionId: args.sessionId,
      messageId,
      labels: (args.card.pictures ?? []).map((picture) => picture.label),
    }),
    next: PLAN_CARD_COPY.proposeNext,
  }
}

const alreadyUploaded = () =>
  new TRPCError({
    code: `CONFLICT`,
    message: `This picture was already uploaded. Pictures of a card are final.`,
  })

/** Refuses before any storage work: the token's card is gone, has no such
 *  label, or that picture already landed. */
export async function assertCardPictureOpen(messageId: string, sessionId: string, label: string) {
  const [row] = await db
    .select({ card: messages.card, sessionId: messages.sessionId })
    .from(messages)
    .where(eq(messages.id, messageId))
    .limit(1)
  const card = (row?.card ?? null) as MessageCard | null
  if (!row || row.sessionId !== sessionId || !card || (card.kind !== `plan` && card.kind !== `question`)) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Card not found` })
  }
  const picture = (card.pictures ?? []).find((entry) => entry.label === label)
  if (!picture) throw new TRPCError({ code: `NOT_FOUND`, message: `This card has no picture "${label}"` })
  if (picture.attachmentId) throw alreadyUploaded()
}

/**
 * Files an uploaded card picture (the object is already put): the
 * attachments row owned by the card's message (read = the session's
 * visibility, /api/attachments) + the card with the label resolved
 * (`resolveUpload`: its `pictures` entry, a plan's `upload:<label>` embeds),
 * in ONE transaction under the message row's lock. An answered card still
 * takes its late pictures.
 */
export async function publishCardPicture(
  input: { messageId: string; sessionId: string; teamId: string; label: string; uploaderId: string },
  prepared: PreparedSessionImage
): Promise<{ label: string; card: MessageCard }> {
  try {
    return await db.transaction(async (tx) => {
      await generateTxId(tx)
      const [row] = await tx
        .select({ card: messages.card, sessionId: messages.sessionId })
        .from(messages)
        .where(eq(messages.id, input.messageId))
        .limit(1)
        .for(`update`)
      const card = (row?.card ?? null) as MessageCard | null
      if (!row || row.sessionId !== input.sessionId || !card || (card.kind !== `plan` && card.kind !== `question`)) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Card not found` })
      }
      const picture = (card.pictures ?? []).find((entry) => entry.label === input.label)
      if (!picture) throw new TRPCError({ code: `NOT_FOUND`, message: `This card has no picture "${input.label}"` })
      if (picture.attachmentId) throw alreadyUploaded()
      await tx.insert(attachments).values({
        ...sessionAttachmentValues(prepared, { teamId: input.teamId }, input.uploaderId),
        messageId: input.messageId,
      })
      const next = resolveUpload(card as AskCardLike, {
        label: input.label,
        attachmentId: prepared.attachmentId,
        width: prepared.width,
        height: prepared.height,
      }) as MessageCard
      await tx
        .update(messages)
        .set({ card: next, updatedAt: new Date() })
        .where(eq(messages.id, input.messageId))
      return { label: input.label, card: next }
    })
  } catch (error) {
    await rollbackSessionImage(prepared.storageKey)
    throw error
  }
}
