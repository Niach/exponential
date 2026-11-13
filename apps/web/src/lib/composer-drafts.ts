// #FEED-219 (fixture `composer-draft.json`): the web's writer for composer
// drafts. Every composer's field is mirrored into a synced `composer_drafts`
// row keyed by its target, so a tab switch, a navigation, a reload or another
// device shows the text again (before, the session and New chat composers
// held it in component state only, and the comment composers in an
// in-memory tab map: an unmount or a reload lost it).
//
// Writes are DEBOUNCED per key (`COMPOSER_DRAFT_DEBOUNCE_MS` after the last
// keystroke) and FLUSHED at once when a composer goes away, after a send
// settles and when the page hides. Until a write has been echoed back by the
// shape, its text stays here as the key's LOCAL copy (the fixture's
// `local`), so a composer that remounts in between never shows an older
// synced row. A failed write is silent: the local copy stays and the next
// write retries. No React here — `hooks/use-composer-draft.ts` wraps it.

import {
  COMPOSER_DRAFT_DEBOUNCE_MS,
  composerDraftWrite,
} from "@exp/domain-contract/composer-draft"

export interface ComposerDraftTransport {
  upsert(input: { teamId: string; key: string; text: string }): Promise<{ txId: number }>
  delete(input: { key: string }): Promise<{ txId: number }>
  /** Resolves once the shape has delivered the write's transaction. */
  awaitTxId(txId: number): Promise<unknown>
}

interface Entry {
  teamId: string
  /** The newest text for the key, written or not. */
  text: string
  timer: ReturnType<typeof setTimeout> | null
  /** A write is in flight; `dirty` = another one is due after it. */
  writing: boolean
  dirty: boolean
}

const entries = new Map<string, Entry>()
let transport: ComposerDraftTransport | null = null

/** Installed once by the app (`hooks/use-composer-draft.ts`); tests install
 *  a fake. */
export function setComposerDraftTransport(next: ComposerDraftTransport | null): void {
  transport = next
}

/** The key's own write not yet echoed back (null = nothing pending). */
export function localComposerDraft(key: string): string | null {
  return entries.get(key)?.text ?? null
}

/** Record the field's text; it is written `debounceMs` later, or at once. */
export function writeComposerDraft(
  key: string,
  teamId: string,
  text: string,
  options: { now?: boolean } = {}
): void {
  let entry = entries.get(key)
  if (!entry) {
    entry = { teamId, text, timer: null, writing: false, dirty: false }
    entries.set(key, entry)
  }
  entry.teamId = teamId
  entry.text = text
  entry.dirty = true
  if (entry.timer) clearTimeout(entry.timer)
  entry.timer = null
  if (options.now) {
    void flushComposerDraft(key)
    return
  }
  entry.timer = setTimeout(() => {
    entry.timer = null
    void flushComposerDraft(key)
  }, COMPOSER_DRAFT_DEBOUNCE_MS)
}

/** Write the key's pending text now (no-op when nothing is due). */
export async function flushComposerDraft(key: string): Promise<void> {
  const entry = entries.get(key)
  if (!entry) return
  if (entry.timer) {
    clearTimeout(entry.timer)
    entry.timer = null
  }
  if (entry.writing || !entry.dirty || !transport) return
  const text = entry.text
  entry.dirty = false
  entry.writing = true
  try {
    const write = composerDraftWrite(text)
    const { txId } =
      write.op === `upsert`
        ? await transport.upsert({ teamId: entry.teamId, key, text: write.text })
        : await transport.delete({ key })
    await transport.awaitTxId(txId)
  } catch {
    // Silent: the local copy stays, the next write retries.
    entry.dirty = true
  } finally {
    entry.writing = false
  }
  if (entries.get(key) !== entry) return
  if (entry.dirty && entry.text !== text) {
    // Typed while the write was in flight: write the newer text.
    if (!entry.timer) void flushComposerDraft(key)
    return
  }
  if (!entry.dirty && !entry.timer) entries.delete(key)
}

/** Write every pending draft now (the page is hiding). */
export function flushAllComposerDrafts(): void {
  for (const key of [...entries.keys()]) void flushComposerDraft(key)
}

/** Test seam. */
export function resetComposerDrafts(): void {
  for (const entry of entries.values()) if (entry.timer) clearTimeout(entry.timer)
  entries.clear()
}
