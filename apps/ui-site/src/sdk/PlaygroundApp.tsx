/* The playground (client-only, loaded by pages/Playground.tsx after
   hydration): an editor that takes A2UI messages (a JSON array/object or
   JSONL), a flat component list or a nested node; the live render beside it
   (debounced) with the reducer's issues and the actions it fires; a theme
   switch (the built-ins or a pasted theme JSON) and light/dark; a shareable
   URL (the state deflated into the hash); the system prompt the catalog
   generates for a model, with an optional extension catalog. */
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react"
import { BUILTIN_THEMES, builtinTheme, catalogPrompt, estimateTokens, tryLoadTheme, validateExtension } from "@exponential-at/ui"
import type { ExtensionDef, ReduceIssue, ResolvedTheme } from "@exponential-at/ui"
import type { SurfaceActionEvent } from "@exponential-at/ui-react"
import extensionFixture from "@exponential-at/ui/fixtures/catalog-extension.json"
import { DocsCode } from "@exp/site-shell"
import { parseInput, surfaceIds, type Message } from "./a2ui"
import { loadRuntime, type SdkRuntime } from "./Island"
import { EXAMPLES } from "./playground-examples"
import { decodeShare, encodeShare, SHARE_KEY, shareTokenFromHash } from "./share"

type Mode = `light` | `dark`
type Tab = `surface` | `theme` | `extension`

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
const EXAMPLE_EXTENSION = JSON.stringify(extensionFixture.extension, null, 2)

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value)
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms)
    return () => clearTimeout(t)
  }, [value, ms])
  return v
}

/** Tab inserts two spaces instead of leaving the editor. */
function onEditorKey(e: KeyboardEvent<HTMLTextAreaElement>, set: (v: string) => void) {
  if (e.key !== `Tab` || e.shiftKey || e.metaKey || e.ctrlKey) return
  e.preventDefault()
  const el = e.currentTarget
  const { selectionStart: a, selectionEnd: b, value } = el
  const next = `${value.slice(0, a)}  ${value.slice(b)}`
  set(next)
  requestAnimationFrame(() => el.setSelectionRange(a + 2, a + 2))
}

export default function PlaygroundApp() {
  const [runtime, setRuntime] = useState<SdkRuntime | null>(null)
  const [source, setSource] = useState(EXAMPLES[0]!.source)
  const [exampleId, setExampleId] = useState(EXAMPLES[0]!.id)
  const [theme, setTheme] = useState(`exponential`)
  const [mode, setMode] = useState<Mode>(`dark`)
  const [themeJson, setThemeJson] = useState(``)
  const [extensionJson, setExtensionJson] = useState(``)
  const [tab, setTab] = useState<Tab>(`surface`)
  const [issues, setIssues] = useState<Record<string, ReduceIssue[]>>({})
  const [actions, setActions] = useState<SurfaceActionEvent[]>([])
  const [lite, setLite] = useState(false)
  const [terse, setTerse] = useState(false)
  const [shareNote, setShareNote] = useState(``)
  const restored = useRef(false)

  useEffect(() => {
    void loadRuntime().then(setRuntime)
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
      setMode(s.mode)
      setThemeJson(s.themeJson ?? ``)
      setExtensionJson(s.extensionJson ?? ``)
      setShareNote(`Restored from the link.`)
    })
  }, [])

  const debounced = useDebounced(source, DEBOUNCE_MS)
  const parsed = useMemo(() => parseInput(debounced), [debounced])
  const ids = useMemo(() => surfaceIds(parsed.messages), [parsed.messages])
  const bySurface = useMemo(() => new Map(ids.map((id) => [id, parsed.messages.filter((m) => (Object.values(m).find((v) => v && typeof v === `object` && `surfaceId` in v) as { surfaceId?: string } | undefined)?.surfaceId === id)])), [ids, parsed.messages])

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
  const ground = (typeof activeTheme === `string` ? builtinTheme(activeTheme) : activeTheme).modes[mode].color.background

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

  const prompt = useMemo(() => catalogPrompt({ extensions: extension.defs, lite, terse }), [extension.defs, lite, terse])

  const onIssues = useCallback((id: string, list: ReduceIssue[]) => setIssues((cur) => (cur[id] === list ? cur : { ...cur, [id]: list })), [])
  const onAction = useCallback((e: SurfaceActionEvent) => setActions((cur) => [e, ...cur].slice(0, 6)), [])

  const pickExample = (id: string) => {
    const ex = EXAMPLES.find((e) => e.id === id)
    if (!ex) return
    setExampleId(id)
    setSource(ex.source)
    setActions([])
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
    <div className="sdk-playground" data-ready={runtime ? `true` : undefined}>
      <div className="sdk-studio-bar">
        <label className="sdk-inline-label">
          Example
          <select className="sdk-select" value={exampleId} onChange={(e) => pickExample(e.target.value)} aria-label="Starter example">
            {exampleId === `` && <option value="">(your own)</option>}
            {EXAMPLES.map((e) => (
              <option key={e.id} value={e.id}>
                {e.label}
              </option>
            ))}
          </select>
        </label>
        <label className="sdk-inline-label">
          Theme
          <select className="sdk-select" value={theme} onChange={(e) => setTheme(e.target.value)} aria-label="Theme">
            {BUILTIN_THEMES.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
            <option value={CUSTOM}>Paste a theme…</option>
          </select>
        </label>
        <div className="sdk-seg" role="group" aria-label="Mode">
          {([`light`, `dark`] as const).map((m) => (
            <button key={m} type="button" aria-pressed={mode === m} onClick={() => setMode(m)}>
              {m === `light` ? `Light` : `Dark`}
            </button>
          ))}
        </div>
        <span className="sdk-grow" />
        {shareNote && <span className="sdk-note">{shareNote}</span>}
        <button type="button" className="btn btn-primary btn-sm" onClick={() => void share()}>
          Copy link
        </button>
      </div>

      <div className="sdk-playground-grid">
        <div className="sdk-editor-col">
          <div className="sdk-tabs" role="tablist">
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
            <>
              <textarea
                className="sdk-editor"
                value={source}
                onChange={(e) => {
                  setSource(e.target.value)
                  setExampleId(``)
                }}
                onKeyDown={(e) => onEditorKey(e, setSource)}
                spellCheck={false}
                aria-label="A2UI JSON or JSONL"
              />
              <p className="sdk-note">
                Read as: <strong>{parsed.form === `empty` ? `nothing yet` : { messages: `A2UI messages`, jsonl: `JSONL messages`, flat: `a flat component list`, nested: `a nested node` }[parsed.form]}</strong>
                {parsed.messages.length > 0 && ` · ${parsed.messages.length} message${parsed.messages.length > 1 ? `s` : ``} · ${ids.length} surface${ids.length === 1 ? `` : `s`}`}
              </p>
            </>
          )}
          {tab === `theme` && (
            <>
              <textarea
                className="sdk-editor"
                value={themeJson}
                onChange={(e) => setThemeJson(e.target.value)}
                onKeyDown={(e) => onEditorKey(e, setThemeJson)}
                spellCheck={false}
                aria-label="Theme JSON"
                placeholder={`{\n  "$schema": "https://ui.exponential.at/schemas/theme/v1.json",\n  "id": "brand",\n  "name": "Brand",\n  "extends": "neutral",\n  "modes": { "dark": { "color": { "primary": "#2563eb" } } },\n  "recipes": { "Button": { "root": [{ "style": { "borderRadius": "$radius.full" } }] } }\n}`}
              />
              <p className="sdk-note">
                A theme file (<code>extends</code> a built-in, override what changes). Build one in the <a href="/themes/builder/">theme builder</a>.
                {custom.theme && ` Loaded: ${custom.theme.name}.`}
              </p>
            </>
          )}
          {tab === `extension` && (
            <>
              <textarea
                className="sdk-editor"
                value={extensionJson}
                onChange={(e) => setExtensionJson(e.target.value)}
                onKeyDown={(e) => onEditorKey(e, setExtensionJson)}
                spellCheck={false}
                aria-label="Extension catalog JSON"
                placeholder="An extension catalog: {id, name, extends, components, macros?}"
              />
              <p className="sdk-note">
                Validated with <code>validateExtension</code>; its components join the system prompt below. Macro components expand and render; a NATIVE extension component has no
                painter here, so it renders as the <code>Unknown</code> placeholder (a host registers a painter with <code>registerExtension</code>).{` `}
                <button type="button" className="sdk-link" onClick={() => setExtensionJson(EXAMPLE_EXTENSION)}>
                  Load the example extension
                </button>
                {extension.defs.length > 0 && ` · valid: ${Object.keys(extension.defs[0]!.components).join(`, `)}`}
              </p>
            </>
          )}
        </div>

        <div className="sdk-render-col">
          <div className={`sdk-stage is-${mode} is-playground`} style={{ background: ground }}>
            {!runtime ? (
              <div className="sdk-placeholder" aria-busy="true">
                <span>Loading the renderer…</span>
              </div>
            ) : ids.length === 0 ? (
              <div className="sdk-placeholder">
                <span>Nothing to render yet.</span>
              </div>
            ) : (
              ids.map((id) => (
                <div key={id} className="sdk-playground-surface">
                  {ids.length > 1 && <div className="sdk-surface-label">surface {id}</div>}
                  <runtime.LiveSurface
                    surfaceId={id}
                    domId={`pg-${id.replace(/[^a-zA-Z0-9_-]/g, ``)}`}
                    messages={bySurface.get(id) as Message[]}
                    theme={activeTheme}
                    mode={mode}
                    icons={runtime.icons}
                    extensions={extension.defs}
                    onIssues={(list) => onIssues(id, list)}
                    onAction={onAction}
                  />
                </div>
              ))
            )}
          </div>
          <div className={`sdk-issues${errorCount ? ` has-issues` : ``}`} aria-live="polite">
            <div className="sdk-issues-head">{errorCount ? `${errorCount} issue${errorCount > 1 ? `s` : ``}` : `No issues`}</div>
            {errorCount > 0 && (
              <ul>
                {parsed.errors.map((e) => (
                  <li key={`p-${e}`}>
                    <span className="sdk-issue-kind">parse</span> {e}
                  </li>
                ))}
                {allIssues.map((i, n) => (
                  <li key={`r-${n}`}>
                    <span className="sdk-issue-kind">reducer</span> <code>{i.id}</code> {i.message}
                  </li>
                ))}
                {custom.errors.map((e) => (
                  <li key={`t-${e}`}>
                    <span className="sdk-issue-kind">theme</span> {e}
                  </li>
                ))}
                {extension.errors.map((e) => (
                  <li key={`x-${e}`}>
                    <span className="sdk-issue-kind">extension</span> {e}
                  </li>
                ))}
              </ul>
            )}
          </div>
          {actions.length > 0 && (
            <div className="sdk-actions-log">
              <div className="sdk-issues-head">Actions sent to the host</div>
              <ul>
                {actions.map((a) => (
                  <li key={`${a.timestamp}-${a.componentId}`}>
                    <code>{a.name}</code> from <code>{a.componentId}</code>
                    {Object.keys(a.context ?? {}).length > 0 && <span className="sdk-dim"> {JSON.stringify(a.context)}</span>}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      </div>

      <section className="sdk-prompt">
        <div className="sdk-studio-bar">
          <h2>System prompt</h2>
          <label className="sdk-check">
            <input type="checkbox" checked={lite} onChange={(e) => setLite(e.target.checked)} /> lite
          </label>
          <label className="sdk-check">
            <input type="checkbox" checked={terse} onChange={(e) => setTerse(e.target.checked)} /> terse
          </label>
          <span className="sdk-grow" />
          <span className="sdk-note">
            ~{estimateTokens(prompt).toLocaleString(`en-US`)} tokens · {prompt.length.toLocaleString(`en-US`)} characters
          </span>
        </div>
        <p className="sdk-note">
          What <code>catalogPrompt({`{ extensions, lite, terse }`})</code> generates for the current catalog{extension.defs.length ? ` plus the extension` : ``}: the rules and every component a model may use.
        </p>
        <div className="sdk-prompt-code">
          <DocsCode language="text">{prompt}</DocsCode>
        </div>
      </section>
    </div>
  )
}
