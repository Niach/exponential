import { For, Show, children, createEffect, createMemo, createSignal, onCleanup, splitProps, type ComponentProps, type JSX } from "solid-js"
import { Check, Copy } from "lucide-solid"
import { tokenizeCode } from "@exponential-at/ui"
import { CODE_BLOCK_CLASSES, CODE_TOKEN_CLASS, codeBlockLineVariants, codeBlockVariants, type CodeBlockLineVariantProps, type CodeBlockVariantProps } from "@exponential-at/ui/web/primitives"

import { PaintIcon } from "../icons"
import { css, type StyleInput } from "../style"
import { Button, type ButtonProps } from "./button"
import { cn } from "./cn"

function CodeBlock(props: ComponentProps<`div`> & CodeBlockVariantProps & { size?: `default` | `sm` }) {
  const [local, rest] = splitProps(props, [`class`, `variant`, `size`])
  const variant = () => local.variant ?? `default`
  return <div data-slot="code-block" data-variant={variant()} data-size={local.size ?? `default`} class={cn(codeBlockVariants({ variant: variant() }), local.class)} {...rest} />
}

const slotPart = (slot: string, recipe: `header` | `title` | `actions` | `footer`) =>
  function CodeBlockPart(props: ComponentProps<`div`>) {
    const [local, rest] = splitProps(props, [`class`])
    return <div data-slot={slot} class={cn(CODE_BLOCK_CLASSES[recipe], local.class)} {...rest} />
  }

const CodeBlockHeader = slotPart(`code-block-header`, `header`)
const CodeBlockTitle = slotPart(`code-block-title`, `title`)
const CodeBlockActions = slotPart(`code-block-actions`, `actions`)
const CodeBlockFooter = slotPart(`code-block-footer`, `footer`)

/** The copy action: ui-copy → ui-check for 2s, announced politely. A copy
 *  that did not happen (no clipboard, a refusal) changes nothing. */
function CodeBlockCopyButton(props: ButtonProps & { value: string; labels?: { copy: string; copied: string } }) {
  const [local, rest] = splitProps(props, [`value`, `variant`, `size`, `class`, `onClick`, `labels`])
  const [copied, setCopied] = createSignal(false)
  const labels = () => local.labels ?? { copy: `Copy`, copied: `Copied` }
  createEffect(() => {
    if (!copied()) return
    const timeout = setTimeout(() => setCopied(false), 2000)
    onCleanup(() => clearTimeout(timeout))
  })
  const onClick = (event: MouseEvent & { currentTarget: HTMLButtonElement; target: Element }) => {
    const handler = local.onClick
    if (typeof handler === `function`) handler(event)
    else if (Array.isArray(handler)) handler[0](handler[1], event)
    const clipboard = typeof navigator === `undefined` ? undefined : navigator.clipboard
    if (!clipboard?.writeText) return
    clipboard
      .writeText(local.value)
      .then(() => setCopied(true))
      .catch(() => {})
  }
  return (
    <>
      <Button
        data-slot="code-block-copy-button"
        data-copied={copied() || undefined}
        variant={local.variant ?? `ghost`}
        size={local.size ?? `icon-xs`}
        aria-label={copied() ? labels().copied : labels().copy}
        class={cn(local.class)}
        onClick={onClick}
        {...rest}
      >
        <Show when={copied()} fallback={<PaintIcon icon={Copy} />}>
          <PaintIcon icon={Check} />
        </Show>
      </Button>
      <span aria-live="polite" class={CODE_BLOCK_CLASSES.srOnly}>
        {copied() ? labels().copied : ``}
      </span>
    </>
  )
}

function CodeBlockContent(
  props: Omit<ComponentProps<`pre`>, `style`> & {
    /** React-form (camelCase) or kebab-case keys, merged after the block's own. */
    style?: StyleInput
    wrap?: boolean
    lineNumbers?: boolean
    startLine?: number
    /** A catalog language (`ts`, `json`, `ansi`…): string children are
     *  tokenized and coloured per token kind. */
    language?: string
    /** Lines shown before the block scrolls; 0 = unbounded. */
    maxLines?: number
  }
) {
  const [local, rest] = splitProps(props, [`class`, `children`, `style`, `wrap`, `lineNumbers`, `startLine`, `language`, `maxLines`])
  const resolved = children(() => local.children)
  const source = createMemo(() => {
    const c = resolved()
    return typeof c === `string` ? c : null
  })
  const lines = createMemo(() => {
    const s = source()
    return s !== null && local.language ? tokenizeCode(s.replace(/\n$/, ``), local.language) : null
  })
  const maxLines = () => local.maxLines ?? 16
  const style = () =>
    css({
      counterReset: `line ${(local.startLine ?? 1) - 1}`,
      ...(maxLines() > 0 ? { maxHeight: `calc(${maxLines()}lh + var(--code-block-pad, 1.5rem))` } : null),
      ...local.style,
    })
  const body = (): JSX.Element => {
    const tokenized = lines()
    if (tokenized !== null)
      return (
        <For each={tokenized}>
          {(tokens) => (
            <CodeBlockLine>
              {tokens.length === 0 ? null : (
                <For each={tokens}>
                  {(token) => (
                    <span data-token={token.kind} class={cn(CODE_TOKEN_CLASS[token.kind], CODE_BLOCK_CLASSES.token)}>
                      {token.text}
                    </span>
                  )}
                </For>
              )}
            </CodeBlockLine>
          )}
        </For>
      )
    const s = source()
    if (local.lineNumbers && s !== null) return <For each={s.replace(/\n$/, ``).split(`\n`)}>{(line) => <CodeBlockLine>{line}</CodeBlockLine>}</For>
    return resolved()
  }
  return (
    <pre
      data-slot="code-block-content"
      data-wrap={local.wrap || undefined}
      data-numbered={local.lineNumbers || undefined}
      data-language={local.language}
      dir="ltr"
      tabindex={0}
      style={style()}
      class={cn(CODE_BLOCK_CLASSES.content, local.class)}
      {...rest}
    >
      <code data-slot="code-block-code" class={CODE_BLOCK_CLASSES.code}>
        {body()}
      </code>
    </pre>
  )
}

function CodeBlockLine(props: ComponentProps<`span`> & CodeBlockLineVariantProps) {
  const [local, rest] = splitProps(props, [`class`, `variant`, `children`])
  const resolved = children(() => local.children)
  // An empty line holds a newline so copying a selection keeps blank lines.
  const empty = () => {
    const c = resolved()
    return c === undefined || c === null || c === `` || (Array.isArray(c) && c.length === 0)
  }
  return (
    <span data-slot="code-block-line" data-variant={local.variant ?? `default`} class={cn(codeBlockLineVariants({ variant: local.variant ?? `default` }), local.class)} {...rest}>
      <span data-slot="code-block-line-content" class={CODE_BLOCK_CLASSES.lineContent}>
        {empty() ? `\n` : resolved()}
      </span>
    </span>
  )
}

export { CodeBlock, CodeBlockHeader, CodeBlockTitle, CodeBlockActions, CodeBlockCopyButton, CodeBlockContent, CodeBlockLine, CodeBlockFooter, CODE_TOKEN_CLASS }
