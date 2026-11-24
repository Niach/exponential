// #NEW-134: what of an issue is embedded, and the hash that says whether it
// changed. Pure (no db, no model): unit-tested in input.test.ts.
//
// The model reads at most 512 tokens (about 1 700 to 2 300 characters of
// German or English prose), so the text is budgeted BEFORE the tokenizer cuts
// it, in the order the cut should spare:
//
//   1. the title, whole (it names the issue);
//   2. the description, its first `descriptionChars`;
//   3. the comments, OLDEST first (the first replies usually pin down the
//      report: repro steps, the actual error), each cut to `commentChars`,
//      until `totalChars` is full.
//
// The description is capped below the model's limit on purpose: with a long
// description the first comments still reach the model instead of being cut
// off entirely. Nothing is repeated to weight it; e5 was trained on plain
// "title + body" passages and the title already leads.
//
// Markdown is flattened where its syntax would only burn tokens: an image or
// link keeps its text and loses its URL (attachment URLs are a UUID each),
// whitespace runs collapse. Mentions and `#IDENT` references stay: they are
// words a person searches for.
import { createHash } from "node:crypto"
import embeddingModel from "../../../embedder/model.json"

export const ISSUE_EMBEDDING_MODEL = embeddingModel.id
export const ISSUE_EMBEDDING_DIMENSIONS = embeddingModel.dimensions

export const ISSUE_EMBEDDING_INPUT = {
  titleChars: 300,
  descriptionChars: 1200,
  commentChars: 240,
  totalChars: 2400,
} as const

export interface IssueEmbeddingSource {
  title: string
  description: string | null
  /** Live comments only (a trashed one never reaches the model), oldest first. */
  comments: readonly string[]
}

/** Markdown → the words the model should read. */
export function plainForEmbedding(markdown: string): string {
  return (
    markdown
      // ![alt](url) → alt, [text](url) → text
      .replace(/!?\[([^\]]*)\]\([^)\s]*(?:\s+"[^"]*")?\)/g, `$1`)
      // a bare attachment URL left over
      .replace(/\/api\/attachments\/[0-9a-f-]{36}/gi, ` `)
      .replace(/\s+/g, ` `)
      .trim()
  )
}

/** Cuts at a code-point boundary (never through a surrogate pair). */
function cut(text: string, chars: number): string {
  if (text.length <= chars) return text
  const sliced = text.slice(0, chars)
  const last = sliced.charCodeAt(sliced.length - 1)
  return last >= 0xd800 && last <= 0xdbff ? sliced.slice(0, -1) : sliced
}

/** The passage text for one issue, WITHOUT the `passage: ` prefix. */
export function buildIssueEmbeddingInput(source: IssueEmbeddingSource): string {
  const budget = ISSUE_EMBEDDING_INPUT
  const parts: string[] = [cut(plainForEmbedding(source.title), budget.titleChars)]
  const description = cut(plainForEmbedding(source.description ?? ``), budget.descriptionChars)
  if (description) parts.push(description)
  let used = parts.reduce((sum, part) => sum + part.length + 1, 0)
  for (const body of source.comments) {
    const comment = cut(plainForEmbedding(body), budget.commentChars)
    if (!comment) continue
    if (used + comment.length > budget.totalChars) break
    parts.push(comment)
    used += comment.length + 1
  }
  return parts.join(`\n`)
}

/** sha256 over the model id and the exact input: equal = nothing to embed. */
export function issueEmbeddingHash(input: string, model: string = ISSUE_EMBEDDING_MODEL): string {
  return createHash(`sha256`).update(`${model}\n${input}`).digest(`hex`)
}

/** pgvector's text form of a vector, for a `::vector` cast. */
export function vectorLiteral(vector: ArrayLike<number>): string {
  return `[${Array.from(vector, (value) => (Number.isFinite(value) ? value : 0)).join(`,`)}]`
}
