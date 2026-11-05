/* The genUI playground (an island, `client:load` through Playground.astro):
   an editor that takes A2UI messages (a JSON array/object or JSONL), a flat
   component list or a nested node; the live render beside it (debounced)
   with the reducer's issues and the actions it fires; a theme switch (the
   built-ins or a pasted theme JSON) and light/dark (following the site's
   theme until picked); a shareable URL (share.ts, the state deflated into
   the hash); the system prompt the catalog generates for a model, with an
   optional extension catalog.
   The whole app prerenders with the first example, so the first paint is in
   the HTML. The renderer + icons load after hydration; the prompt module
   and the example extension load lazily, so the ~4.4k-word catalog prompt
   never sits in the HTML. */
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent } from "react"
import { BUILTIN_THEMES, builtinTheme, tryLoadTheme, validateExtension } from "@exponential_app/eui"
import type { ExtensionDef, ReduceIssue, ResolvedTheme } from "@exponential_app/eui"
import type { SurfaceActionEvent } from "@exponential_app/eui-react"
import { messagesFor, parseInput, surfaceIds } from "./a2ui"
import { EXAMPLES } from "./playground-examples"
import type { SdkRuntime } from "./runtime"
import { decodeShare, encodeShare, SHARE_KEY, shareTokenFromHash } from "./share"

type Mode = `light` | `dark`
type Tab = `surface` | `theme` | `extension`
type PromptModule = typeof import("@exponential_app/eui/prompt")

/** The share-hash payload (format kept identical to the old /sdk/genui/ site). */
interface ShareState {
  v: 1
  source: string
  theme: string
  mode: Mode
  themeJson?: string
  extensionJson?: string
}

const CUSTOM = `custom`
const DEBOUNCE_MS = 250

let runtime: Promise<SdkRuntime> | null = null
const loadRuntime = () => (runtime ??= Promise.all([import(`./runtime`), import(`./icons`)]).then(([r, i]) => ({ LiveSurface: r.LiveSurface, icons: i.siteIcons })))

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value)
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms)
    return () => clearTimeout(t)
  }, [value, ms])
  return v
}

/** The site's theme (Starlight's data-theme); dark in the prerender.
 *  `onSwitch` runs when the visitor flips it (not on the first read). */
function useSiteMode(onSwitch: () => void): Mode {
  const [mode, setMode] = useState<Mode>(`dark`)
  const switched = useRef(onSwitch)
  switched.current = onSwitch
  useEffect(() => {
    const read = () => setMode(document.documentElement.dataset.theme === `light` ? `light` : `dark`)
    read()
    const mo = new MutationObserver(() => {
      read()
      switched.current()
    })
    mo.observe(document.documentElement, { attributes: true, attributeFilter: [`data-theme`] })
    return () => mo.disconnect()
  }, [])
  return mode
}

/** Tab inserts two spaces instead of leaving the editor. */
function onEditorKey(e: KeyboardEvent<HTMLTextAreaElement>, set: (v: string) => void) {
  if (e.key !== `Tab` || e.shiftKey || e.metaKey || e.ctrlKey) return
  e.preventDefault()
  const el = e.currentTarget
  const { selectionStart: a, selectionEnd: b, value } = el
  set(`${value.slice(0, a)}  ${value.slice(b)}`)
  requestAnimationFrame(() => el.setSelectionRange(a + 2, a + 2))
}

function PromptCode({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  const onCopy = () => {
    void navigator.clipboard?.writeText(text).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1400)
    })
  }
  return (
    <div className="eui-code">
      <div className="eui-code-head">
        <span>text</span>
        <button type="button" className="eui-code-copy" onClick={onCopy}>
          {copied ? `Copied` : `Copy`}
        </button>
      </div>
      <pre>
        <code>{text}</code>
      </pre>
    </div>
  )
}

export default function PlaygroundApp() {
  const [rt, setRt] = useState<SdkRuntime | null>(null)
  const [source, setSource] = useState(EXAMPLES[0]!.source)
  const [exampleId, setExampleId] = useState(EXAMPLES[0]!.id)
  const [theme, setTheme] = useState(`exponential`)
  // null = follow the site's theme; a click or a shared link picks one, until
  // the site's theme switches again (Head.astro then re-syncs every surface).
  const [pickedMode, setPickedMode] = useState<Mode | null>(null)
  const siteMode = useSiteMode(() => setPickedMode(null))
  const mode = pickedMode ?? siteMode
  const [themeJson, setThemeJson] = useState(``)
  const [extensionJson, setExtensionJson] = useState(``)
  const [tab, setTab] = useState<Tab>(`surface`)
  const [issues, setIssues] = useState<Record<string, ReduceIssue[]>>({})
  const [actions, setActions] = useState<SurfaceActionEvent[]>([])
  const [lite, setLite] = useState(false)
  const [terse, setTerse] = useState(false)
  // VAPP-108: the sectioned prompt: the index (one line per component) instead of every block.
  const [index, setIndex] = useState(false)
  const [promptMod, setPromptMod] = useState<PromptModule | null>(null)
  const [shareNote, setShareNote] = useState(``)
  const restored = useRef(false)
  const promptRef = useRef<HTMLElement>(null)

  useEffect(() => {
    void loadRuntime().then(setRt)
    // Restore a shared state from the hash, once.
    const token = shareTokenFromHash(location.hash)
    if (!token || restored.current) return
    restored.current = true
    void decodeShare<ShareState>(token).then((s) => {
      if (!s || s.v !== 1) {
        setShareNote(`That link could not be read.`)
        return
      }
      setSource(s.source)
      setExampleId(``)
      setTheme(s.theme)
      setPickedMode(s.mode === `light` ? `light` : `dark`)
      setThemeJson(s.themeJson ?? ``)
      setExtensionJson(s.extensionJson ?? ``)
      setShareNote(`Restored from the link.`)
    })
  }, [])

  // The prompt module (the catalog's descriptions) loads once its section nears the viewport.
  useEffect(() => {
    const el = promptRef.current
    if (!el) return
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return
        io.disconnect()
        void import(`@exponential_app/eui/prompt`).then(setPromptMod)
      },
      { rootMargin: `600px` }
    )
    io.observe(el)
    return () => io.disconnect()
  }, [])

  const debounced = useDebounced(source, DEBOUNCE_MS)
  const parsed = useMemo(() => parseInput(debounced), [debounced])
  const ids = useMemo(() => surfaceIds(parsed.messages), [parsed.messages])
  const bySurface = useMemo(() => new Map(ids.map((id) => [id, messagesFor(parsed.messages, id)])), [ids, parsed.messages])

  // A pasted theme: validated against the built-ins; issues listed.
  const custom = useMemo(() => {
    if (theme !== CUSTOM || !themeJson.trim()) return { theme: null as ResolvedTheme | null, errors: [] as string[] }
    try {
      const { theme: t, issues: is } = tryLoadTheme(JSON.parse(themeJson), { themes: BUILTIN_THEMES })
      return { theme: t, errors: is.map((i) => `${i.path}: ${i.message}`) }
    } catch (e) {
      return { theme: null, errors: [(e as Error).message] }
    }
  }, [theme, themeJson])
  const activeTheme = theme === CUSTOM ? (custom.theme ?? `exponential`) : theme
  const resolved = typeof activeTheme === `string` ? (BUILTIN_THEMES.some((t) => t.id === activeTheme) ? builtinTheme(activeTheme) : builtinTheme(`exponential`)) : activeTheme
  // Both grounds ride as variables; `is-auto` (no pick) lets the site's theme choose in CSS, so the prerender paints the visitor's scheme.
  const grounds = { "--eui-ground-light": resolved.modes.light.color.background, "--eui-ground-dark": resolved.modes.dark.color.background } as CSSProperties

  // An optional extension catalog: validated with validateExtension.
  const extension = useMemo(() => {
    if (!extensionJson.trim()) return { defs: [] as ExtensionDef[], errors: [] as string[] }
    try {
      const def = JSON.parse(extensionJson) as ExtensionDef
      const errors = validateExtension(def)
      return { defs: errors.length ? [] : [def], errors }
    } catch (e) {
      return { defs: [], errors: [(e as Error).message] }
    }
  }, [extensionJson])

  const prompt = useMemo(
    () => (promptMod ? promptMod.catalogPrompt({ extensions: extension.defs, lite, terse, ...(index ? { sections: [`rules`, `index`, `functions`, `style`] } : {}) }) : null),
    [promptMod, extension.defs, lite, terse, index]
  )

  const onIssues = useCallback((id: string, list: ReduceIssue[]) => setIssues((cur) => (cur[id] === list ? cur : { ...cur, [id]: list })), [])
  const onAction = useCallback((e: SurfaceActionEvent) => setActions((cur) => [e, ...cur].slice(0, 6)), [])

  const pickExample = (id: string) => {
    const ex = EXAMPLES.find((e) => e.id === id)
    if (!ex) return
    setExampleId(id)
    setSource(ex.source)
    setActions([])
  }

  const loadExampleExtension = () => {
    void import(`@exponential_app/eui/fixtures/catalog-extension.json`).then((m) => setExtensionJson(JSON.stringify((m.default as { extension: unknown }).extension, null, 2)))
  }

  const share = async () => {
    const state: ShareState = { v: 1, source, theme, mode, ...(themeJson.trim() ? { themeJson } : {}), ...(extensionJson.trim() ? { extensionJson } : {}) }
    const token = await encodeShare(state)
    const url = `${location.origin}${location.pathname}#${SHARE_KEY}=${token}`
    history.replaceState(null, ``, url)
    try {
      await navigator.clipboard.writeText(url)
      setShareNote(`Link copied (${url.length.toLocaleString(`en-US`)} characters).`)
    } catch {
      setShareNote(`Link is in the address bar.`)
    }
  }

  const allIssues = ids.flatMap((id) => (issues[id] ?? []).map((i) => ({ surface: id, ...i })))
  const errorCount = parsed.errors.length + allIssues.length + custom.errors.length + extension.errors.length

  return (
    <div className="eui-playground not-content" data-ready={rt ? `true` : undefined}>
      <div className="eui-bar">
        <label className="eui-inline-label">
          Example
          <select className="eui-select" value={exampleId} onChange={(e) => pickExample(e.target.value)} aria-label="Starter example">
            {exampleId === `` && <option value="">(your own)</option>}
            {EXAMPLES.map((e) => (
              <option key={e.id} value={e.id}>
                {e.label}
              </option>
            ))}
          </select>
        </label>
        <label className="eui-inline-label">
          Theme
          <select className="eui-select" value={theme} onChange={(e) => setTheme(e.target.value)} aria-label="Theme">
            {BUILTIN_THEMES.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
            <option value={CUSTOM}>Paste a theme…</option>
          </select>
        </label>
        <div className="eui-seg" role="group" aria-label="Mode">
          {([`light`, `dark`] as const).map((m) => (
            <button key={m} type="button" aria-pressed={mode === m} onClick={() => setPickedMode(m)}>
              {m === `light` ? `Light` : `Dark`}
            </button>
          ))}
        </div>
        <span className="eui-grow" />
        {shareNote && <span className="eui-note">{shareNote}</span>}
        <button type="button" className="eui-btn" onClick={() => void share()}>
          Copy link
        </button>
      </div>

      <div className="eui-playground-grid">
        <div className="eui-editor-col">
          <div className="eui-tabs" role="tablist">
            {(
              [
                [`surface`, `Surface`],
                [`theme`, `Theme JSON`],
                [`extension`, `Extension`],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={tab === id}
                onClick={() => {
                  setTab(id)
                  if (id === `theme` && theme !== CUSTOM) setTheme(CUSTOM)
                }}
              >
                {label}
              </button>
            ))}
          </div>
          {tab === `surface` && (
            <textarea
              className="eui-editor"
              value={source}
              onChange={(e) => {
                setSource(e.target.value)
                setExampleId(``)
              }}
              onKeyDown={(e) => onEditorKey(e, setSource)}
              spellCheck={false}
              aria-label="A2UI JSON or JSONL"
            />
          )}
          {tab === `theme` && (
            <textarea
              className="eui-editor"
              value={themeJson}
              onChange={(e) => setThemeJson(e.target.value)}
              onKeyDown={(e) => onEditorKey(e, setThemeJson)}
              spellCheck={false}
              aria-label="Theme JSON"
              placeholder={`{\n  "$schema": "https://ui.exponential.at/schemas/theme/v1.json",\n  "id": "brand",\n  "name": "Brand",\n  "extends": "neutral",\n  "modes": { "dark": { "color": { "primary": "#2563eb" } } },\n  "recipes": { "Button": { "root": [{ "style": { "borderRadius": "$radius.full" } }] } }\n}`}
            />
          )}
          {tab === `extension` && (
            <>
              <textarea
                className="eui-editor"
                value={extensionJson}
                onChange={(e) => setExtensionJson(e.target.value)}
                onKeyDown={(e) => onEditorKey(e, setExtensionJson)}
                spellCheck={false}
                aria-label="Extension catalog JSON"
                placeholder="An extension catalog: {id, name, extends, components, macros?}"
              />
              <p className="eui-note">
                <button type="button" className="eui-link" onClick={loadExampleExtension}>
                  Load the example extension
                </button>
              </p>
            </>
          )}
        </div>

        <div className="eui-render-col">
          <div className={`eui-stage is-${pickedMode ?? `auto`}`} style={grounds} data-mode={mode}>
            {!rt ? (
              <div className="eui-placeholder" aria-busy="true">
                <span>Loading the renderer…</span>
              </div>
            ) : ids.length === 0 ? (
              <div className="eui-placeholder">
                <span>Nothing to render yet.</span>
              </div>
            ) : (
              ids.map((id) => (
                <div key={id} className="eui-playground-surface" data-surface-id={id}>
                  {ids.length > 1 && <div className="eui-surface-label">surface {id}</div>}
                  <rt.LiveSurface
                    surfaceId={id}
                    domId={`pg-${id.replace(/[^a-zA-Z0-9_-]/g, ``)}`}
                    messages={bySurface.get(id) ?? []}
                    theme={activeTheme}
                    mode={mode}
                    icons={rt.icons}
                    extensions={extension.defs}
                    onIssues={(list) => onIssues(id, list)}
                    onAction={onAction}
                  />
                </div>
              ))
            )}
          </div>
          <div className={`eui-issues${errorCount ? ` has-issues` : ``}`} aria-live="polite">
            <div className="eui-issues-head">{errorCount ? `${errorCount} issue${errorCount > 1 ? `s` : ``}` : `No issues`}</div>
            {errorCount > 0 && (
              <ul>
                {parsed.errors.map((e) => (
                  <li key={`p-${e}`}>
                    <span className="eui-issue-kind">parse</span> {e}
                  </li>
                ))}
                {allIssues.map((i, n) => (
                  <li key={`r-${n}`}>
                    <span className="eui-issue-kind">reducer</span> <code>{i.id}</code> {i.message}
                  </li>
                ))}
                {custom.errors.map((e) => (
                  <li key={`t-${e}`}>
                    <span className="eui-issue-kind">theme</span> {e}
                  </li>
                ))}
                {extension.errors.map((e) => (
                  <li key={`x-${e}`}>
                    <span className="eui-issue-kind">extension</span> {e}
                  </li>
                ))}
              </ul>
            )}
          </div>
          {actions.length > 0 && (
            <div className="eui-actions-log">
              <div className="eui-issues-head">Actions sent to the host</div>
              <ul>
                {actions.map((a) => (
                  <li key={`${a.timestamp}-${a.componentId}`}>
                    <code>{a.name}</code> from <code>{a.componentId}</code>
                    {Object.keys(a.context ?? {}).length > 0 && <span className="eui-dim"> {JSON.stringify(a.context)}</span>}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      </div>

      <section className="eui-pg-prompt" ref={promptRef}>
        <div className="eui-bar">
          <h2>System prompt</h2>
          <label className="eui-check">
            <input type="checkbox" checked={lite} onChange={(e) => setLite(e.target.checked)} /> lite
          </label>
          <label className="eui-check">
            <input type="checkbox" checked={terse} onChange={(e) => setTerse(e.target.checked)} /> terse
          </label>
          <label className="eui-check">
            <input type="checkbox" checked={index} onChange={(e) => setIndex(e.target.checked)} /> index only
          </label>
          <span className="eui-grow" />
          {prompt && promptMod && <span className="eui-note">~{promptMod.estimateTokens(prompt).toLocaleString(`en-US`)} tokens</span>}
        </div>
        {prompt ? (
          <PromptCode text={prompt} />
        ) : (
          <div className="eui-placeholder eui-pg-prompt-placeholder" aria-busy="true">
            <span>Loading the prompt…</span>
          </div>
        )}
      </section>
    </div>
  )
}
