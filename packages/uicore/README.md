# @exp/uicore

The shell contract (#EDE-46): the pure shell reducer the Exponential IDE and
eDE both host (`ide = shell + uicore`, `ede = compositor + uicore`) and the
geometry their documents share. Each `fixtures/*.json` is the spec (its
`$comment`) plus cases; `src/` holds the pure TypeScript reference that
replays them; the `uicore-model` crate replays the same fixtures in Rust, so
the IDE's shell, eDE's documents and the goldens agree.

| fixture | covers |
|---|---|
| `shell.json` | `reduce(state, event) -> state`: the rail, the folded sidebar sections, the desktop/team switcher, windows + the MRU, workspaces, Mint's taskbar popups (one popup at a time), the start menu, the chat toggle, Alt-Tab; the selectors `taskbarButtons`, `taskbarPress`, `taskbarScroll` (Mint's grouped window list: click, scroll, pinned-but-not-running), `workspaceBadges`, `altTabOrder`, `startMenuApps` (Mint's categories + the ranked search), `startMenuCategories` |
| `ide-sidebar.json` | #EXP-1725: what the IDE's sidebar shows, a pure projection of the host's state (the nav's rows and when they hide, what lights, who gets the gears); `sidebar.json` binds it under `/shell/ide` |
| `chat.json` | #EDE-65 lane I: THE agent chat document both hosts draw (`docs/chat.json`): the home face (agent row + composer) and a conversation's rows, bubbles or transcript, with the composer's state, a pure projection of what the host resolved |
| `shell-layout.json` | the numbers the documents use (sidebar 272, rail 56, taskbar 48, panel margin 10 + radius 12, row 28, section label 24, taskbar button 40, applet 32, start menu 760×620, previews tile 184×110), read by `src/layout.ts` |

`parity.md` is the inventory: every region of the IDE's shell against its eDE
region, the genUI components it uses, both sets of numbers and a status
(`same`, `close`, `gap`); it ends with the parity gate's rule (#EDE-45).

Not an SDK module: eDE lives in `apps/ede` (`docs/libraries.md`). Private, no release.

Licence: Apache-2.0
