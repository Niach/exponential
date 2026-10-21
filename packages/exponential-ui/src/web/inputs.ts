// VAPP-87 (FEED-128: hoisted from the React renderer's inputs.ts +
// natives/inputs.tsx): HOST-OWNED inputs, framework-free. Every edit bumps a
// revision and, after `DEBOUNCE_MS`, sends ONE `change` with the latest
// value; `commit` goes out at once on blur / Enter. A value pushed by the
// host (the data model, a new `value` prop) resets the local value only when
// the field is not focused AND no revision is outstanding (the host
// acknowledged everything it was sent). That is the VAPP-4 typing rule: a
// stale echo can never clobber a key in flight. A renderer wraps
// `HostOwnedController` in its reactivity (React: `useHostOwnedValue`).

export const DEBOUNCE_MS = 150

export type HostOwnedSend<T> = (value: T, revision: number, kind: `change` | `commit`) => void | Promise<void>

export interface HostOwnedControllerOptions<T> {
  /** The value the host holds when the field mounts. */
  initial: T
  /** Sends an edit; a returned promise acknowledges the revision on settle. */
  send: HostOwnedSend<T>
  /** Optional local write-through (the bound data model path). */
  writeLocal?: (value: T) => void
  debounceMs?: number
  /** Called after every change of `value`, `pending` or `focused`. */
  onChange?: () => void
}

export class HostOwnedController<T> {
  /** The field's value (the last edit, or the last adopted host push). */
  value: T
  /** The newest revision sent or scheduled. */
  revision = 0
  /** True while a revision the host has not acknowledged exists. */
  pending = false
  focused = false
  private acked = 0
  private timer: ReturnType<typeof setTimeout> | null = null
  private live = true
  private send: HostOwnedSend<T>
  private writeLocal?: (value: T) => void
  private debounceMs: number
  private readonly onChange?: () => void

  constructor(options: HostOwnedControllerOptions<T>) {
    this.value = options.initial
    this.send = options.send
    this.writeLocal = options.writeLocal
    this.debounceMs = options.debounceMs ?? DEBOUNCE_MS
    this.onChange = options.onChange
  }

  /** The latest callbacks (a re-render may hand new closures). */
  configure(options: Pick<HostOwnedControllerOptions<T>, `send` | `writeLocal` | `debounceMs`>): void {
    this.send = options.send
    this.writeLocal = options.writeLocal
    this.debounceMs = options.debounceMs ?? DEBOUNCE_MS
  }

  /** A local edit: bump the revision, write through, debounce a `change`. */
  edit(next: T): void {
    this.value = next
    this.revision += 1
    this.pending = true
    this.writeLocal?.(next)
    this.clearTimer()
    this.timer = setTimeout(() => {
      this.timer = null
      this.dispatch(`change`)
    }, this.debounceMs)
    this.notify()
  }

  /** Flush the debounce and send a `commit`. */
  commit(): void {
    this.clearTimer()
    this.dispatch(`commit`)
  }

  focus(): void {
    this.focused = true
    this.notify()
  }

  /** Blur commits. */
  blur(): void {
    this.focused = false
    this.notify()
    this.commit()
  }

  /** The host's push: adopted only when idle and fully acknowledged.
   *  Returns whether it was adopted. */
  push(external: T): boolean {
    if (this.focused) return false
    if (this.revision > this.acked) return false
    if (Object.is(external, this.value)) return false
    this.value = external
    this.notify()
    return true
  }

  /** (Re)attach after `dispose` (a remount). */
  attach(): void {
    this.live = true
  }

  /** Unmount: the pending debounce is dropped, acknowledgements no longer
   *  notify. */
  dispose(): void {
    this.live = false
    this.clearTimer()
  }

  private clearTimer(): void {
    if (this.timer) {
      clearTimeout(this.timer)
      this.timer = null
    }
  }

  private notify(): void {
    if (this.live) this.onChange?.()
  }

  private ack(rev: number): void {
    if (rev > this.acked) this.acked = rev
    if (this.live && this.acked >= this.revision && this.pending) {
      this.pending = false
      this.notify()
    }
  }

  private dispatch(kind: `change` | `commit`): void {
    const rev = this.revision
    let result: void | Promise<void>
    try {
      result = this.send(this.value, rev, kind)
    } catch {
      result = undefined
    }
    if (result && typeof (result as Promise<void>).then === `function`) {
      ;(result as Promise<void>).then(
        () => this.ack(rev),
        () => this.ack(rev)
      )
    } else this.ack(rev)
  }
}

const localeDigits = new Map<string, Map<string, string>>()

/** The locale's own digit glyphs → ASCII digits (empty for latn locales). */
function digitMap(locale: string): Map<string, string> {
  let hit = localeDigits.get(locale)
  if (!hit) {
    hit = new Map()
    const fmt = new Intl.NumberFormat(locale, { useGrouping: false })
    for (let d = 0; d <= 9; d++) {
      const glyph = fmt.format(d)
      if (glyph !== String(d)) hit.set(glyph, String(d))
    }
    localeDigits.set(locale, hit)
  }
  return hit
}

/** A typed number in the surface locale (its group and decimal separators
 *  AND its digits; ASCII digits are always accepted too) → the number, or
 *  null. */
export function parseLocaleNumber(text: string, locale: string): number | null {
  const parts = new Intl.NumberFormat(locale).formatToParts(12345.6)
  const group = parts.find((p) => p.type === `group`)?.value ?? `,`
  const decimal = parts.find((p) => p.type === `decimal`)?.value ?? `.`
  const digits = digitMap(locale)
  const ascii = digits.size ? Array.from(text, (ch) => digits.get(ch) ?? ch).join(``) : text
  const clean = ascii.trim().split(group).join(``).replace(/[\s‎‏؜]/g, ``).replace(decimal, `.`).replace(/[−–]/g, `-`)
  if (clean === `` || clean === `-`) return null
  const n = Number(clean)
  return Number.isFinite(n) ? n : null
}
