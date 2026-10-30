// FEED-128 (React's natives/code.tsx; round 1 contract §3 CodeBlock,
// a11y.json CodeBlock): the code block on the catalog's ONE tokenizer
// (`tokenizeCode`, catalog/code.json), painted per token kind through the
// `CodeBlock/token` recipe (`when: {kind}`). An optional header (title +
// the copy button: `builtinIcons` copy/copied, `copied` announced), a
// line-number gutter, emphasised `highlight` lines (the `selected` state),
// `wrap`, and `maxLines` before the block scrolls. EXP-1324: `variant`
// (destructive) and `size` (sm) are recipe props; `ansi` output copies
// without its escapes.

import { Index, Show, createMemo, createSignal, onCleanup, type Component, type JSX } from "solid-js"
import { codeText, tokenizeCode } from "@exponential_app/eui"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { arr, bool, BuiltinIcon, css, num, parts, str } from "./shared"

export function CodeBlockNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const code = () => str(props.props.code)
  const language = () => str(props.props.language, `plain`)
  const lines = createMemo(() => tokenizeCode(code(), language()))
  const highlight = createMemo(() => new Set(arr<unknown>(props.props.highlight).map((v) => num(v))))
  const numbers = () => bool(props.props.lineNumbers)
  const copyable = () => props.props.copyable !== false
  const title = () => str(props.props.title)
  // Round 2 (§7 CodeBlock): an untitled block names its language in the
  // header (`ts`; none for `plain`); the region's name stays `codeBlock`.
  const heading = () => title() || (language() === `plain` ? `` : language())
  const maxLines = () => (props.props.maxLines === undefined ? 0 : num(props.props.maxLines))
  const [copied, setCopied] = createSignal(false)
  let timer: ReturnType<typeof setTimeout> | null = null
  onCleanup(() => {
    if (timer) clearTimeout(timer)
  })
  const copy = async () => {
    // Acknowledge only a copy that happened: no clipboard (an insecure
    // origin) or a refusal leaves the button as it was and says nothing.
    const clipboard = typeof navigator === `undefined` ? undefined : navigator.clipboard
    if (!clipboard?.writeText) return
    try {
      await clipboard.writeText(codeText(code(), language()))
    } catch {
      return
    }
    setCopied(true)
    ctx.announce(ctx.t(`copied`))
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => setCopied(false), 1500)
  }
  const gutterWidth = () => `${String(lines().length).length + 1}ch`
  const bodyClass = () => `${part(`body`).class as string} xui-codeblock-scroll`
  const codeClass = () => `${part(`code`).class as string} xui-codeblock-text`
  return (
    <div {...props.rootProps} role="region" aria-label={title() || ctx.t(`codeBlock`)} data-language={language()}>
      <Show when={heading() || copyable()}>
        <div {...part(`header`)}>
          <Show when={heading()} fallback={<span class="xui-codeblock-spacer" />}>
            <span {...part(`title`)}>{heading()}</span>
          </Show>
          <Show when={copyable()}>
            <button type="button" {...part(`copy`, copied() && `checked`)} aria-label={copied() ? ctx.t(`copied`) : ctx.t(`copy`)} onClick={() => void copy()}>
              <BuiltinIcon slot={copied() ? `CodeBlock.copied` : `CodeBlock.copy`} />
            </button>
          </Show>
        </div>
      </Show>
      <div {...part(`body`)} class={bodyClass()} data-wrap={bool(props.props.wrap) ? `true` : undefined} style={maxLines() > 0 ? css({ "--xui-code-lines": maxLines() }) : undefined} data-max-lines={maxLines() > 0 ? maxLines() : undefined} tabindex={0}>
        <pre class="xui-codeblock-pre" id={`${props.domId}.code`}>
          <code>
            <Index each={lines()}>
              {(tokens, i) => (
                <span {...part.at(`line`, i, highlight().has(i + 1) && `selected`)} data-line={i + 1}>
                  <Show when={numbers()}>
                    <span {...part(`gutter`)} style={{ "min-width": gutterWidth() }} aria-hidden="true">
                      <span {...part.at(`lineNumber`, i)}>{i + 1}</span>
                    </span>
                  </Show>
                  <span {...part.at(`code`, i)} class={codeClass()}>
                    {tokens().length === 0 ? `​` : null}
                    <Index each={tokens()}>{(t) => <span {...part.with(`token`, { kind: t().kind })}>{t().text}</span>}</Index>
                  </span>
                  {`\n`}
                </span>
              )}
            </Index>
          </code>
        </pre>
      </div>
    </div>
  )
}

export const NATIVES_CODE: Record<string, Component<NativeProps>> = {
  CodeBlock: CodeBlockNative,
}
