import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray, isNull, or, sql } from "drizzle-orm"
import { z } from "zod"
import {
  CORE_CATALOG_ID,
  reduceNested,
  type NestedNode,
  type ReduceResult,
} from "@exponential_app/eui"
import {
  MESSAGE_BODY_MAX,
  PLAN_CARD_DESCRIPTION_MAX,
  PLAN_CARD_SURFACE_MAX_BYTES,
  PLAN_CARD_SURFACE_MAX_COMPONENTS,
  PLAN_CARD_UPLOAD_SCHEME,
  type CardSurface,
  type MessageCard,
  type PlanCardPicture,
  type PlanCardQuestion,
  sealedBodyMax,
} from "@exp/db-schema/domain"
import { PLAN_CARD_COPY, resolveUpload, type AskCardLike } from "@exp/domain-contract/plan-card"
import { isStoryCard } from "@exp/domain-contract/stories"
import { parseSealedValue } from "@exp/domain-contract/sealed-content"
import { db } from "@/db/connection"
import { attachments, messages, sessions } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { askText, busAsk, closeAskMirrors, type ListenerCopy } from "@/lib/sessions/bus"
import { isSealedBody, isSealedCard, validSealedBody, validSealedCard } from "@/lib/sessions/sealed"
import { isSessionPrincipal, loadSessionForUser } from "@/lib/sessions/access"
import { isAssistantConversation, statusCardTarget } from "@/lib/agents/assistant-chat"
import { notifyAssistantDecision } from "@/lib/integrations/notifications"
import { insertMessage, postOwnAskCard, inSession, messageSessionId } from "@/lib/sessions/messages"
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

/** EXP-1458: what a SEALED ask/plan carries beside its envelope: the ask
 *  text the device sealed for its own tree and one copy per listening tree
 *  (bus.ts `busAsk`). Its pictures ride the card's CLEAR `pictures`:
 *  [{label, attachmentId}] already uploaded (claimed here), [{label}] still
 *  to upload (minted here, filed by `publishCardPicture`). */
export interface SealedShowExtras {
  askText?: string | null
  listenerCopies?: ListenerCopy[]
}

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** EXP-1458: a sealed card's clear `pictures` → their attachment ids. A
 *  `{label}` entry without one is a picture still to upload (addendum 4):
 *  its upload is minted like a plaintext card's and filed on that entry. */
export function sealedCardAttachmentIds(card: MessageCard): string[] {
  if (!isSealedCard(card)) return []
  const pictures = (card as unknown as { pictures?: unknown }).pictures
  if (pictures === undefined || pictures === null) return []
  if (!Array.isArray(pictures)) throw bad(`sealedCard.pictures must be [{label, attachmentId?}]`)
  return pictures.flatMap((picture: unknown) => {
    const { label, attachmentId } = (picture ?? {}) as { label?: unknown; attachmentId?: unknown }
    if (attachmentId === undefined && typeof label === `string`) return []
    if (typeof attachmentId !== `string` || !UUID_RE.test(attachmentId)) {
      throw bad(`sealedCard.pictures: an uploaded picture names its attachmentId, one still to upload its label`)
    }
    return [attachmentId]
  })
}

/** EXP-1458 (addendum 4): a SEALED card with a late upload filed: the clear
 *  `pictures` entry of `label` takes the attachment id; `sealed` is never
 *  touched (readers fill the sealed picture from the clear entry). */
function fileSealedPicture(card: MessageCard, label: string, attachmentId: string): MessageCard {
  const pictures = ((card as unknown as { pictures?: Array<{ label?: string; attachmentId?: string }> }).pictures ?? []).map(
    (picture) => (picture.label === label && !picture.attachmentId ? { label, attachmentId } : picture)
  )
  return { ...card, pictures } as unknown as MessageCard
}

const SEALED_SHOW_KINDS = new Set([`surface`, `question`, `plan`])

/** EXP-1458: a body envelope (never plaintext: it would leak the card). */
const sealedText = z
  .string()
  .max(sealedBodyMax(MESSAGE_BODY_MAX))
  .refine((value) => isSealedBody(value) && validSealedBody(value), `must be an exp-sealed:v1 envelope`)

/** EXP-1458: a card envelope `{kind, sealed, ...clear}` (validSealedCard
 *  checks the clear part). */
export const sealedCardEnvelope = z
  .object({ kind: z.string().max(32), sealed: z.string().min(1).max(sealedBodyMax(MESSAGE_BODY_MAX)) })
  .passthrough()

/** EXP-1458: MCP show's sealed fields (the device gateway's call). */
export const sealedShowExtrasSchema = z.object({
  sealedCard: sealedCardEnvelope,
  kind: z.enum([`surface`, `question`, `plan`]).optional(),
  askText: sealedText.nullable().optional(),
  listenerCopies: z.array(z.object({ rootId: z.string().uuid(), body: sealedText })).max(20).optional(),
})

/** EXP-1458: a sealed show card `{kind, sealed, ...clear}`: only its clear
 *  part is checked (the server never opens `sealed`). */
export function buildSealedShowCard(card: Record<string, unknown>, kind?: string): ShowCard {
  if (!validSealedCard(card) || !SEALED_SHOW_KINDS.has(card.kind as string)) {
    throw bad(`sealedCard must be {kind: "surface"|"question"|"plan", sealed: "v1:<epoch>:<b64url>"} plus clear fields only`)
  }
  if (kind !== undefined && kind !== card.kind) throw bad(`kind is ${kind}; sealedCard is a ${String(card.kind)} card`)
  sealedCardAttachmentIds(card as unknown as MessageCard)
  return card as unknown as ShowCard
}

/** EXP-1458: a sealed card names an epoch its tree holds. */
async function assertSealedCardForTree(tx: Tx, rootId: string, card: MessageCard): Promise<void> {
  if (!isSealedCard(card)) return
  const value = parseSealedValue((card as unknown as { sealed: string }).sealed)
  if (value.kind !== `sealed`) return
  const [root] = await tx.select({ sealEpoch: sessions.sealEpoch }).from(sessions).where(eq(sessions.id, rootId)).limit(1)
  if (root?.sealEpoch == null) throw bad(`This chat has no key for sealed messages yet`)
  if (value.epoch > root.sealEpoch) throw bad(`The chat's key is at epoch ${root.sealEpoch}`)
}

/** EXP-1458: a sealed card's pictures = attachments the caller uploaded to
 *  the team (or already on this row, a replace), claimed by the card's row
 *  and listed in clear on `messages.attachment_ids` (like send.ts
 *  `claimAttachments`, without embed lines: the body is the card). */
export async function claimCardAttachments(
  tx: Tx,
  args: { ids: readonly string[]; messageId: string; teamId: string; uploaderId: string }
): Promise<void> {
  const ids = [...new Set(args.ids)]
  if (ids.length === 0) {
    await tx.update(messages).set({ attachmentIds: null }).where(eq(messages.id, args.messageId))
    return
  }
  const claimed = await tx
    .update(attachments)
    .set({ messageId: args.messageId })
    .where(
      and(
        inArray(attachments.id, ids),
        eq(attachments.teamId, args.teamId),
        isNull(attachments.issueId),
        isNull(attachments.draftId),
        or(
          eq(attachments.messageId, args.messageId),
          and(isNull(attachments.messageId), eq(attachments.uploaderId, args.uploaderId))
        )
      )
    )
    .returning({ id: attachments.id })
  if (claimed.length !== ids.length) throw bad(`Attach files you uploaded to this team (each file once)`)
  await tx.update(messages).set({ attachmentIds: ids }).where(eq(messages.id, args.messageId))
}

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
  pictures?: CardPictureInput[]
  story?: boolean
  description?: string
}): Extract<MessageCard, { kind: `surface` }> {
  const surface = validateCardSurface(input.surface)
  if (!surface) throw bad(`surface is required: an Exponential UI core tree {root, data?}`)
  const title = input.title?.trim() || null
  if (title && title.length > SURFACE_CARD_TITLE_MAX) {
    throw bad(`title is ${title.length} characters; the cap is ${SURFACE_CARD_TITLE_MAX}`)
  }
  // S-3 stories.json: a surface may carry a result story's screenshots;
  // a plain surface is a story only when asked (or once a picture lands).
  const pictures = cardPictures(input.pictures)
  // The cap lives here, not in the show schema (its own byte budget).
  const description = input.description?.trim() || null
  if (description && description.length > PLAN_CARD_DESCRIPTION_MAX) {
    throw bad(`description is ${description.length} characters; the cap is ${PLAN_CARD_DESCRIPTION_MAX}`)
  }
  return {
    kind: `surface`,
    surface,
    title,
    ...(description ? { description } : {}),
    ...(pictures.length ? { pictures } : {}),
    ...(input.story !== undefined ? { story: input.story } : {}),
  }
}

/** EXP-1458: MCP suggest's `{sealed}`: a suggestions/replies card
 *  envelope on the caller's own session, pre-consumed like its plaintext
 *  twin; the server never acts on its items (suggestions.ts refuses). */
export async function postSealedSuggestionCard(args: {
  userId: string
  callerSessionId: string
  card: Record<string, unknown>
}): Promise<{ messageId: string }> {
  const card = args.card
  if (!validSealedCard(card) || (card.kind !== `suggestions` && card.kind !== `replies`)) {
    throw bad(`sealed must be {kind: "suggestions"|"replies", sealed: "v1:<epoch>:<b64url>"} plus clear fields only`)
  }
  const session = await loadSessionForUser(args.userId, args.callerSessionId)
  if (!(await isSessionPrincipal(args.userId, session))) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Suggest only on your own session` })
  }
  const messageId = await db.transaction(async (tx) => {
    await assertSealedCardForTree(tx, session.rootId, card as unknown as MessageCard)
    await generateTxId(tx)
    return insertMessage(tx, session, {
      authorKind: `agent`,
      authorSessionId: session.id,
      card: card as unknown as MessageCard,
      consumedAt: new Date(),
    })
  })
  return { messageId }
}

/** Posts a surface card as an AGENT message in the caller's own session
 *  (answers nothing, wakes nothing; `author_session_id` = the session, so
 *  S-3's `replace` finds it). → the message id. */
export async function postSurfaceCard(args: {
  sessionId: string
  card: Extract<MessageCard, { kind: `surface` }>
  /** EXP-1458: the uploader of a sealed card's pictures (default: the
   *  session's starter). */
  userId?: string
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
    await assertSealedCardForTree(tx, session.rootId, args.card)
    await generateTxId(tx)
    const messageId = await insertMessage(tx, session, { authorKind: `agent`, authorSessionId: session.id, card: args.card })
    if (isSealedCard(args.card)) {
      await claimCardAttachments(tx, {
        ids: sealedCardAttachmentIds(args.card),
        messageId,
        teamId: session.teamId,
        uploaderId: args.userId ?? session.userId,
      })
    }
    return messageId
  })
  return { messageId }
}

type ShowCard = Extract<MessageCard, { kind: `surface` | `plan` | `question` }>

const isAsk = (card: { kind: string }) => card.kind === `plan` || card.kind === `question`

/**
 * S-3 `exponential_show {replace: true}`: the session's latest own card of
 * the same family (a plain surface vs an ask/plan), whatever the turn or
 * title (#EXP-1446), is edited in place and moved to the bottom: the same
 * row, so every client's card updates instead of a new one landing. A card a person already
 * answered is final (refused); none of the family = a new card. → the
 * edited row, or null to insert.
 */
export async function replaceTarget(
  tx: Tx,
  sessionId: string,
  card: ShowCard
): Promise<{ id: string; card: MessageCard } | null> {
  const [row] = await tx
    .select({ id: messages.id, card: messages.card, answeredAt: messages.answeredAt })
    .from(messages)
    .where(
      and(
        inSession(sessionId),
        eq(messages.authorSessionId, sessionId),
        isAsk(card)
          ? sql`${messages.card}->>'kind' IN ('question', 'plan')`
          : sql`${messages.card}->>'kind' = 'surface'`
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
  return { id: row.id, card: row.card }
}

/** S-3 (F37): a replaced card keeps the pictures already uploaded to the
 *  row (same label = same picture): their attachment, size and a plan's
 *  `upload:<label>` embeds resolve on the new card too. */
export function carryPictures(before: MessageCard, card: ShowCard): ShowCard {
  if (!hasCardPictures(before)) return card
  // EXP-1458: a sealed card carries on its CLEAR pictures only: a label
  // still to upload takes the id the replaced sealed card's clear entry got.
  if (isSealedCard(card)) {
    if (!isSealedCard(before)) return card
    let next = card as unknown as MessageCard
    for (const picture of before.pictures ?? []) {
      if (picture.label && picture.attachmentId) next = fileSealedPicture(next, picture.label, picture.attachmentId)
    }
    return next as unknown as ShowCard
  }
  if (isSealedCard(before)) return card
  let next = card
  for (const picture of before.pictures ?? []) {
    if (!picture.attachmentId) continue
    next = resolveUpload(next as AskCardLike, {
      label: picture.label,
      attachmentId: picture.attachmentId,
      width: picture.width ?? null,
      height: picture.height ?? null,
    }) as ShowCard
  }
  return next
}

/** S-3: THE show path (`exponential_show`): a plain surface, a question or
 *  a plan card on the caller's own session; `replace` edits the latest
 *  card in place (`replaceTarget`), else a new card lands (an ask/plan the
 *  way `postAskCard` posts it). */
export async function postShowCard(args: {
  userId: string
  sessionId: string
  card: ShowCard
  replace: boolean
  /** EXP-1458: set = `card` is sealed (`buildSealedShowCard`). */
  sealed?: SealedShowExtras
}): Promise<{ messageId: string; replaced: boolean; uploads: CardUpload[]; next?: string }> {
  const sealedCard = isSealedCard(args.card)
  if (!sealedCard && args.sealed) throw bad(`askText and listenerCopies go with a sealed card`)
  // #EXP-1439: in a member's Assistant conversation a surface is THE live
  // status card (`statusCardTarget`, edited in place across turns until the
  // member writes) and an ask notifies its owner.
  const [owner] = await db
    .select({ id: sessions.id, rootId: sessions.rootId, agentId: sessions.agentId })
    .from(sessions)
    .where(eq(sessions.id, args.sessionId))
    .limit(1)
  const assistant = owner ? await isAssistantConversation(db, owner) : false
  // An explicit replace edits the latest card (`replaceTarget`), never only
  // the status card since the member last wrote (#EXP-1446).
  const status = assistant && args.card.kind === `surface` && !args.replace
  const replaced = args.replace || status
    ? await db.transaction(async (tx) => {
        const [session] = await tx
          .select({ id: sessions.id, rootId: sessions.rootId, teamId: sessions.teamId, status: sessions.status })
          .from(sessions)
          .where(eq(sessions.id, args.sessionId))
          .limit(1)
          .for(`update`)
        if (!session) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
        if (session.status === `ended`) {
          throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This session has ended` })
        }
        const target = status ? await statusCardTarget(tx, session.id) : await replaceTarget(tx, session.id, args.card)
        if (!target) return null
        await assertSealedCardForTree(tx, session.rootId, args.card)
        await generateTxId(tx)
        const now = new Date()
        const card = carryPictures(target.card, args.card)
        // Edited AND moved to the bottom (#EXP-1446): every client orders a
        // chat by created_at (ties by id) and re-sorts on a synced update.
        // The DB clock, so it lands after every row its `now()` stamped.
        await tx
          .update(messages)
          .set({ card, createdAt: sql`clock_timestamp()`, updatedAt: now })
          .where(eq(messages.id, target.id))
        if (sealedCard) {
          await claimCardAttachments(tx, {
            ids: sealedCardAttachmentIds(card),
            messageId: target.id,
            teamId: session.teamId,
            uploaderId: args.userId,
          })
        }
        if (isAsk(args.card) && sealedCard) {
          // EXP-1458: the mirrors' text follows from the device's sealed
          // copies (in-tree = askText, other trees = their listener copy);
          // the server reads neither the old nor the new card.
          await updateSealedAskMirrors(tx, session, target.id, args.sealed ?? {}, now)
        } else if (isAsk(args.card) && !isSealedCard(target.card)) {
          // The parent's / watchers' ask mirrors keep pointing at the row;
          // their text follows (a neutral private line stays as it is).
          const before = askText(session, target.card as Extract<MessageCard, { kind: `question` | `plan` }>)
          await tx
            .update(messages)
            .set({ body: askText(session, card as Extract<MessageCard, { kind: `question` | `plan` }>), updatedAt: now })
            .where(
              and(
                eq(messages.authorSessionId, session.id),
                sql`${messages.card}->>'kind' = 'ask'`,
                sql`${messages.card}->>'messageId' = ${target.id}`,
                eq(messages.body, before)
              )
            )
        }
        return { id: target.id, card }
      })
    : null
  if (replaced) {
    return {
      messageId: replaced.id,
      replaced: true,
      // Only the labels still waiting: a carried picture is already filed.
      uploads: mintCardUploads({
        userId: args.userId,
        sessionId: args.sessionId,
        messageId: replaced.id,
        labels: (replaced.card.pictures ?? []).filter((picture) => !picture.attachmentId).map((picture) => picture.label),
      }),
      ...(isAsk(args.card) ? { next: PLAN_CARD_COPY.proposeNext } : {}),
    }
  }
  if (isAsk(args.card)) {
    const posted = await postAskCard({
      userId: args.userId,
      sessionId: args.sessionId,
      card: args.card as Extract<MessageCard, { kind: `plan` | `question` }>,
      ...(args.sealed ? { sealed: args.sealed } : {}),
    })
    if (assistant) {
      // EXP-1458: a sealed ask notifies with the title alone.
      void notifyAssistantDecision(
        args.sessionId,
        sealedCard
          ? ``
          : args.card.kind === `question`
            ? args.card.question
            : (args.card as Extract<MessageCard, { kind: `plan` }>).plan
      )
    }
    return { ...posted, replaced: false }
  }
  const posted = await postSurfaceCard({
    sessionId: args.sessionId,
    card: args.card as Extract<MessageCard, { kind: `surface` }>,
    userId: args.userId,
  })
  return {
    ...posted,
    replaced: false,
    uploads: mintCardUploads({
      userId: args.userId,
      sessionId: args.sessionId,
      messageId: posted.messageId,
      labels: (args.card.pictures ?? []).filter((picture) => !picture.attachmentId).map((picture) => picture.label),
    }),
  }
}

/** EXP-1458: a replaced SEALED ask's mirrors take the device's new copies:
 *  listeners in the asking tree `askText`, listeners in another tree that
 *  tree's `listenerCopies` entry; a mirror with no new copy keeps its text. */
async function updateSealedAskMirrors(
  tx: Tx,
  session: { id: string; rootId: string },
  messageId: string,
  sealed: SealedShowExtras,
  now: Date
): Promise<void> {
  const mirrorOf = and(
    eq(messages.authorSessionId, session.id),
    sql`${messages.card}->>'kind' = 'ask'`,
    sql`${messages.card}->>'messageId' = ${messageId}`
  )
  const askText = sealed.askText?.trim()
  if (askText) {
    await tx
      .update(messages)
      .set({ body: askText, updatedAt: now })
      .where(and(mirrorOf, eq(messages.rootId, session.rootId)))
  }
  for (const copy of sealed.listenerCopies ?? []) {
    await tx
      .update(messages)
      .set({ body: copy.body, updatedAt: now })
      .where(and(mirrorOf, eq(messages.rootId, copy.rootId), sql`${messages.rootId} <> ${session.rootId}`))
  }
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
        inSession(sessionId),
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
  /** EXP-1458: a sealed card's extras (`SealedShowExtras`). */
  sealed?: SealedShowExtras
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
    await assertSealedCardForTree(tx, session.rootId, args.card)
    await generateTxId(tx)
    if (args.card.kind === `plan`) await supersedeWaitingPlans(tx, session.id)
    const messageId = await postOwnAskCard(tx, session, args.card)
    if (isSealedCard(args.card)) {
      await claimCardAttachments(tx, {
        ids: sealedCardAttachmentIds(args.card),
        messageId,
        teamId: session.teamId,
        uploaderId: args.userId,
      })
    }
    // NEW-1: the card reaches the session's parent and watchers (one row,
    // two views); a parent may answer what it knows. EXP-1458: a sealed
    // card's ask text is the device's (in-tree + per-listener copies).
    await busAsk(
      tx,
      session,
      messageId,
      args.card,
      isSealedCard(args.card)
        ? { askText: args.sealed?.askText ?? null, listenerCopies: args.sealed?.listenerCopies ?? [] }
        : undefined
    )
    return messageId
  })
  return {
    messageId,
    // EXP-1458: a sealed card's clear `{label}` entries still to upload.
    uploads: mintCardUploads({
      userId: args.userId,
      sessionId: args.sessionId,
      messageId,
      labels: (args.card.pictures ?? []).filter((picture) => !picture.attachmentId).map((picture) => picture.label),
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
/** S-3: a card that takes uploaded pictures (an ask, a plan, a surface). */
const hasCardPictures = (card: MessageCard): card is Extract<MessageCard, { kind: `plan` | `question` | `surface` }> =>
  card.kind === `plan` || card.kind === `question` || card.kind === `surface`

export async function assertCardPictureOpen(messageId: string, sessionId: string, label: string) {
  const [row] = await db
    .select({ card: messages.card, sessionId: messageSessionId() })
    .from(messages)
    .where(eq(messages.id, messageId))
    .limit(1)
  const card = (row?.card ?? null) as MessageCard | null
  if (!row || row.sessionId !== sessionId || !card || !hasCardPictures(card)) {
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
  let published: { label: string; card: MessageCard; rootId: string }
  try {
    published = await db.transaction(async (tx) => {
      await generateTxId(tx)
      const [row] = await tx
        .select({ card: messages.card, sessionId: messageSessionId(), rootId: messages.rootId })
        .from(messages)
        .where(eq(messages.id, input.messageId))
        .limit(1)
        .for(`update`)
      const card = (row?.card ?? null) as MessageCard | null
      if (!row || row.sessionId !== input.sessionId || !card || !hasCardPictures(card)) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Card not found` })
      }
      const picture = (card.pictures ?? []).find((entry) => entry.label === input.label)
      if (!picture) throw new TRPCError({ code: `NOT_FOUND`, message: `This card has no picture "${input.label}"` })
      if (picture.attachmentId) throw alreadyUploaded()
      await tx.insert(attachments).values({
        ...sessionAttachmentValues(prepared, { teamId: input.teamId }, input.uploaderId),
        messageId: input.messageId,
      })
      // EXP-1458: a sealed card's upload lands on its clear entry only and
      // joins the row's clear attachment list (`claimCardAttachments`).
      const sealed = isSealedCard(card)
      const next = sealed
        ? fileSealedPicture(card, input.label, prepared.attachmentId)
        : (resolveUpload(card as AskCardLike, {
            label: input.label,
            attachmentId: prepared.attachmentId,
            width: prepared.width,
            height: prepared.height,
          }) as MessageCard)
      await tx
        .update(messages)
        .set({
          card: next,
          ...(sealed ? { attachmentIds: sql`array_append(coalesce(${messages.attachmentIds}, '{}'), ${prepared.attachmentId}::uuid)` } : {}),
          updatedAt: new Date(),
        })
        .where(eq(messages.id, input.messageId))
      return { label: input.label, card: next, rootId: row.rootId }
    })
  } catch (error) {
    await rollbackSessionImage(prepared.storageKey)
    throw error
  }
  // S-3: a story picture is a PR picture: the thread's live PR bodies
  // re-render (lib/pull-requests.ts, lazily: it imports this module's peers).
  // A card outside the stories (`story: false`, an engine ask) is no PR picture.
  if (isStoryCard(published.card)) void import(`@/lib/pull-requests`)
    .then(({ syncPullRequestBodies }) => syncPullRequestBodies(published.rootId))
    .catch((error: unknown) => console.error(`[stories] PR body sync failed`, error))
  return { label: published.label, card: published.card }
}
