/* The scene tree of a compositor built for agents: outputs, workspaces,
   toplevels, surfaces, and inside a painted (A2UI) window the components
   themselves, each addressable by path. A native client is pixels below its
   surface plus whatever tree its toolkit offers once the launcher switches
   it on (AT-SPI, the DevTools protocol). A highlight walks the path to one
   button to show addressing. */
import type { ReactNode } from "react"

type Node = {
  label: ReactNode
  note?: string
  /** Index on the highlighted path (root = 0), the walk's order. */
  path?: number
  /** The path's end: the target badge. */
  target?: boolean
  /** Where reach ends for a native client. */
  stop?: boolean
  kind?: `output` | `space` | `window` | `surface` | `bridge` | `component` | `layer`
  children?: Node[]
}

const TREE: Node = {
  label: `compositor`,
  note: `one seat per agent, one per person`,
  path: 0,
  kind: `output`,
  children: [
    {
      label: `output DP-1`,
      note: `2560×1440 · you`,
      kind: `output`,
      children: [
        {
          label: `workspace 2`,
          kind: `space`,
          children: [{ label: `toplevel "Dota 2"`, note: `fullscreen · input: your seat only · capture: watch only`, kind: `window` }],
        },
      ],
    },
    {
      label: `output DP-2`,
      note: `1920×1080 · agents`,
      path: 1,
      kind: `output`,
      children: [
        {
          label: `workspace 3`,
          path: 2,
          kind: `space`,
          children: [
            {
              label: `toplevel firefox`,
              note: `xdg_toplevel · seat: agent 1`,
              kind: `window`,
              children: [
                { label: `wl_surface`, note: `buffer 1904×1024`, kind: `surface`, stop: true },
                {
                  label: `AT-SPI`,
                  note: `41 nodes · switched on at launch`,
                  kind: `bridge`,
                  children: [
                    { label: `document "Payouts"`, kind: `bridge` },
                    { label: `button "Export CSV"`, note: `action: press`, kind: `bridge` },
                  ],
                },
              ],
            },
            {
              label: `toplevel slack`,
              note: `electron · seat: agent 1`,
              kind: `window`,
              children: [
                { label: `wl_surface`, note: `buffer 1280×800`, kind: `surface`, stop: true },
                { label: `DOM via DevTools protocol`, note: `per-app grant · localhost`, kind: `bridge`, children: [{ label: `textbox "Message #finance"`, kind: `bridge` }] },
              ],
            },
            {
              label: `toplevel "Expenses"`,
              note: `a2ui · seat: you`,
              path: 3,
              kind: `window`,
              children: [
                {
                  label: `Column`,
                  path: 4,
                  kind: `component`,
                  children: [
                    { label: `Heading "Log an expense"`, kind: `component` },
                    { label: `Field amount`, note: `= 42.00`, kind: `component` },
                    { label: `Field category`, note: `= Travel`, kind: `component` },
                    { label: `Dropzone receipt`, kind: `component` },
                    { label: `Button "Save"`, note: `action: save`, path: 5, target: true, kind: `component` },
                  ],
                },
              ],
            },
            { label: `toplevel files`, note: `a2ui · seat: agent 2`, kind: `window` },
          ],
        },
      ],
    },
    {
      label: `layer panel`,
      note: `a2ui · top edge`,
      kind: `layer`,
      children: [
        { label: `Workspaces`, kind: `component` },
        { label: `Clock`, kind: `component` },
        {
          label: `Tray`,
          kind: `component`,
          children: [
            { label: `Agents`, note: `2 working`, kind: `component` },
            { label: `Mic`, note: `push-to-talk F13 · wake word`, kind: `component` },
          ],
        },
      ],
    },
  ],
}

function Branch({ node }: { node: Node }) {
  const cls = [`ct-node`, node.kind ? `ct-node--${node.kind}` : ``, node.path !== undefined ? `ct-path ct-path--${node.path}` : ``, node.target ? `is-target` : ``, node.stop ? `is-stop` : ``]
    .filter(Boolean)
    .join(` `)
  return (
    <li className={cls}>
      <div className="ct-row">
        <i className="ct-dot" />
        <span className="ct-label">{node.label}</span>
        {node.note ? <em className="ct-note">{node.note}</em> : null}
        {node.target ? <b className="ct-badge ct-badge--target">target · a2ui://expenses/save</b> : null}
        {node.stop ? <b className="ct-badge ct-badge--stop">pixels</b> : null}
      </div>
      {node.children?.length ? (
        <ul>
          {node.children.map((child, i) => (
            <Branch key={i} node={child} />
          ))}
        </ul>
      ) : null}
    </li>
  )
}

export function CompositorTree() {
  return (
    <div className="ct" role="img" aria-label="The compositor's tree: two outputs, their workspaces and windows; a native Firefox window is pixels plus an AT-SPI tree and an Electron window pixels plus its DOM over the DevTools protocol, while the painted Expenses window continues into its components down to a Save button that an agent addresses by path; a panel layer with workspaces, a clock and a tray">
      <ul className="ct-root">
        <Branch node={TREE} />
      </ul>
      <div className="ct-legend">
        <span>
          <i className="ct-dot ct-dot--component" /> component: id, role, value, action
        </span>
        <span>
          <i className="ct-dot ct-dot--surface" /> native surface: pixels
        </span>
        <span>
          <i className="ct-dot ct-dot--bridge" /> the toolkit's own tree, switched on by the launcher
        </span>
        <span>
          <i className="ct-dot ct-dot--walk" /> an agent's address, root to leaf
        </span>
      </div>
    </div>
  )
}
