import type { SnapdomPlugin } from "@zumer/snapdom"

// Email addresses visible on the host page are PII — redact them in snapDOM's
// detached clone so the pixels never exist. Redaction (same-length bullet runs,
// `@` kept as a visual cue) beats CSS blur: it can never be reversed and does
// not depend on filter rasterization support.
const EMAIL_RE = /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g

const BULLET = `•`

function redact(text: string): string {
  return text.replace(/[^@]/g, BULLET)
}

export function maskEmailsInText(text: string): string {
  return text.replace(EMAIL_RE, redact)
}

function maskTextNodes(root: Node): void {
  const document = root.ownerDocument ?? (root as Document)
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const parentTag = node.parentElement?.tagName
    if (parentTag === `SCRIPT` || parentTag === `STYLE`) continue
    const text = node.nodeValue
    if (!text || !text.includes(`@`)) continue
    const masked = maskEmailsInText(text)
    if (masked !== text) node.nodeValue = masked
  }
}

function maskFormValues(root: Node): void {
  if (!(root instanceof Element)) return
  for (const el of root.querySelectorAll(`input, textarea`)) {
    if (el instanceof HTMLInputElement) {
      const isEmailField = (el.type || ``).toLowerCase() === `email`
      // Email fields redact any value containing `@` — a half-typed address
      // the full pattern would miss is still someone's email.
      const masked =
        isEmailField && el.value.includes(`@`)
          ? redact(el.value)
          : maskEmailsInText(el.value)
      if (masked !== el.value) {
        // snapDOM mirrors input values onto both the property and the
        // attribute of the clone; overwrite both so neither leaks.
        el.value = masked
        el.setAttribute(`value`, masked)
      }
    } else if (el instanceof HTMLTextAreaElement) {
      const masked = maskEmailsInText(el.value)
      if (masked !== el.value) {
        el.value = masked
        el.textContent = masked
      }
    }
  }
}

export function maskEmailsInTree(root: Node): void {
  maskTextNodes(root)
  maskFormValues(root)
}

// snapDOM rasterizes a readable (same-origin) iframe to an <img> DURING clone
// construction — before afterClone runs — so the tree walker above never sees
// its text and emails inside it would reach the screenshot unmasked. Masking
// the iframe's live document instead is off the table: it mutates the host
// page and a framework re-render could leave redaction bullets behind. So
// readable iframes are excluded from capture via the `filter` option, which
// snapDOM evaluates before its iframe branch; filterMode `hide` swaps in an
// invisible same-size box. Cross-origin iframes are unreadable and can't
// leak — snapDOM's own skip path keeps handling those.
export function isReadableIframe(el: Element): boolean {
  if (el.tagName !== `IFRAME`) return false
  try {
    return !!(
      (el as HTMLIFrameElement).contentDocument ??
      (el as HTMLIFrameElement).contentWindow?.document
    )
  } catch {
    return false
  }
}

export const piiMaskPlugin: SnapdomPlugin = {
  name: `exp-pii-mask`,
  afterClone(context) {
    if (context.clone) maskEmailsInTree(context.clone)
  },
}
