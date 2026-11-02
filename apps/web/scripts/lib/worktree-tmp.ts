/**
 * EXP-1564: a tmpdir path private to one checkout. Every worktree on a
 * machine shares `$TMPDIR`, so a fixed name there (the screenshot eNet stub's
 * identity, its feed file) is clobbered by whichever worktree ran last. Keying
 * the name by the checkout's root keeps restarts in the SAME worktree on the
 * same file and keeps concurrent worktrees apart.
 */
import { createHash } from "node:crypto"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"

/** The first 12 hex of sha1(resolved root): stable per checkout. */
export function worktreeKey(root: string): string {
  return createHash(`sha1`).update(resolve(root)).digest(`hex`).slice(0, 12)
}

/** `$TMPDIR/<stem>-<key><ext>` for the checkout at `root`. */
export function worktreeTmpPath(root: string, stem: string, ext = ``, dir = tmpdir()): string {
  return join(dir, `${stem}-${worktreeKey(root)}${ext}`)
}
