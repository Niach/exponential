# @exp/video

Remotion project for Exponential's promo / demo videos. Compositions render MP4s
from **real screenshots of the running web app** (captured with Playwright), wrapped
in animated browser-window mockups on the app's dark zinc + indigo theme.

## Compositions

- **`WebUiDemo`** — ~20s walkthrough (1920×1080, 30 fps): intro → sign in →
  issue board → issue detail → outro.

## Commands

From the repo root:

```bash
bun run studio:video     # open the Remotion Studio editor (live preview)
bun run render:video     # render WebUiDemo → apps/video/out/webui-demo.mp4
```

Or from `apps/video/`:

```bash
bun run studio          # remotion studio
bun run render          # render the default composition
bunx remotion still WebUiDemo out/frame.png --frame=240   # one-frame check
```

Rendered videos land in `out/` (git-ignored).

## Layout

```
src/
├── Root.tsx        # registers the WebUiDemo composition (dims / fps / duration)
├── Video.tsx       # scenes, captions, and the sequence timeline
├── components.tsx  # Background glow, Logo, BrowserWindow mockup, Scene fade wrapper
├── theme.ts        # Inter font + brand color tokens
└── index.css       # minimal reset
public/shots/       # source screenshots of the live web app
```

## Refreshing the screenshots

The images in `public/shots/` are captures of the live web app. To regenerate
them, run the local stack (`bun run backend:up` + `bun dev`), seed a workspace,
and drive Chromium with Playwright. Two gotchas when scripting the capture:

- Over plain `http://localhost`, the browser drops Better Auth's `session_token`
  cookie, so the React login form can't authenticate tRPC. Log in via Playwright's
  `context.request.post('/api/auth/sign-in/email', { headers: { Origin } })` so both
  auth cookies land in the shared cookie jar, then navigate.
- Electric's shape sync uses long-polling, so the page never reaches
  `networkidle`. Wait on visible content (a known issue title) instead, and give
  the first sync a few seconds.

## Notes

- Pinned to Remotion `4.0.484` (exact-pinned so installs stay reproducible).
- Pure inline styles — no Tailwind.
- Not deployed by CI; this app just produces video artifacts on demand.
