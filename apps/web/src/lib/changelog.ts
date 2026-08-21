// In-app changelog (EXP-164). Entries power the dismissable "What's new" card
// in the sidebar footer and the detailed changelog sheet it opens.
//
// Authoring convention: every user-facing release prepends ONE entry at the
// HEAD of `CHANGELOG` with a fresh `id`. The card re-surfaces for a user
// whenever the head entry's id differs from the one they last dismissed
// (per-device, see `changelog-seen.ts`). Keep `summary` to a single short
// line (it renders truncated in the card) and `body` to a few GFM bullets.
// Body bullets read `- **Title**: text`, with the colon outside the bold and
// the next word lowercase. No em dashes anywhere in changelog copy.

export interface ChangelogEntry {
  // Stable slug, e.g. `2026-07-whats-new-card`. Never reuse an id: it is
  // the dismissal key.
  id: string
  // ISO date (display only).
  date: string
  title: string
  // One-line card preview.
  summary: string
  // GFM markdown, rendered read-only in the changelog sheet.
  body: string
}

// Newest first.
export const CHANGELOG: ChangelogEntry[] = [
  {
    id: `2026-08-automations-split`,
    date: `2026-08-21`,
    title: `Automations are their own thing`,
    summary: `Automations moved out of the action editor into their own list, so one action can run on several schedules or events.`,
    body: `- **New automation button**: the Automations tab has its own form. Pick an action, a schedule or an event, the machine it runs on, and optionally pin the agent, model and effort.
- **Many per action**: an action can now carry as many automations as you like, each with its own trigger and device. Action cards show how many target them.
- **Cleaner action editor**: creating or editing an action no longer asks about triggers.
- **Suggestions**: cards say whether a suggestion sets up just an action or an action plus its automation, and the create dialog prefills the trigger.`,
  },
  {
    id: `2026-08-mobile-issue-ref-menu`,
    date: `2026-08-21`,
    title: `Issue links on mobile`,
    summary: `Typing # on iOS opens a proper issue menu with status, identifier and title, and Android shows the status too.`,
    body: `- **iOS issue links**: typing \`#\` in a comment or description now opens a vertical menu under your text, the same one as @mentions and emoji, instead of a cramped horizontal strip that never appeared in the comment composer.
- **Status everywhere**: issue suggestions on iOS and Android lead with the issue's status glyph, matching web and desktop, and show up to eight matches.`,
  },
  {
    id: `2026-08-mobile-glass-ui-icon-pickers`,
    date: `2026-08-21`,
    title: `Mobile polish and compact icon pickers`,
    summary: `Android gets glass inputs and buttons, issue lists pin their status headers cleanly, and icons are picked from a compact swatch on every app.`,
    body: `- **Android look**: one glass text field and glass buttons replace the Material outlines across login, onboarding, issues, filters, search, support and settings.
- **Issue lists**: status headers stick while scrolling on iOS and Android without a stray band under the navigation bar.
- **Icon pickers**: board forms, action editors and the widget launcher show the current icon as a swatch that opens the grid, with a real "No icon" state for optional action inputs.
- **Actions**: the same layout on every app, with section counts and a labeled New action button.
- **iOS labels**: editing or deleting a label from Team settings no longer fails.`,
  },
  {
    id: `2026-08-mobile-editor-fixes`,
    date: `2026-08-20`,
    title: `Mobile editor fixes`,
    summary: `iOS keeps your bold and italic marks through heading changes, and Android link editing and the issue bottom bar behave.`,
    body: `- **iOS editor**: changing a heading or tapping Clear formatting no longer wipes bold, italic and code marks in the paragraph, Clear formatting applies only to the selected text, and heading changes cover every paragraph in the selection.
- **Android links**: editing a link with the caret inside it now updates the whole link instead of corrupting it, and clearing the URL field removes the link.
- **Android issue view**: the floating bottom bar returns after dismissing the keyboard with the back gesture.`,
  },
  {
    id: `2026-08-editor-rail-widget-launcher`,
    date: `2026-08-20`,
    title: `A formatting rail, widget launcher styles, server insights`,
    summary: `Format issue text from a rail on every app, style the feedback widget button, and watch server health on a new admin Performance page.`,
    body: `- **Formatting rail**: the issue description and comment editors on web, desktop, iOS and Android grew a formatting rail: headings, bold, lists, links, code and more, applied to your selection without typing markdown.
- **Widget launcher styles**: the feedback widget button is now configurable per widget: floating button or edge tab, corner or edge position, a picked icon, and separate desktop and mobile choices, with a live preview in widget settings.
- **Admin Performance page**: global admins get a live view of request latencies, Electric shape traffic, database table sizes, notification email flow and relay health.
- **Android editor**: pressing Enter at the end of the description now reliably adds a new line, and the review screen's merge conflict notice was rebuilt to match the app.
- **Remote approvals**: permission prompts an agent raises for dangerous commands now reach the remote steering view even in skip-permissions sessions, so you can approve or deny from your phone.`,
  },
  {
    id: `2026-08-desktop-issue-open-crash`,
    date: `2026-08-19`,
    title: `Desktop hotfix: opening an issue`,
    summary: `Desktop 0.14.17 fixes a crash that closed the app whenever an issue was opened.`,
    body: `- **Desktop crash fix**: opening an issue in the desktop app no longer closes it. The comment composer's emoji button (from the previous release) tripped an internal render check on every issue view; 0.14.17 repairs it, and the desktop app updates itself.`,
  },
  {
    id: `2026-08-comment-files-paused-sessions`,
    date: `2026-08-19`,
    title: `Files on comments, paused sessions, getting started`,
    summary: `Attach images and files to comments, see a coding session as paused while its machine is offline, and a getting-started checklist that finishes itself.`,
    body: `- **Files on comments**: the comment composer now uploads images and files on send, on web, desktop and Android. They render under the comment as image previews and file chips, can be added or removed in edit mode, and stay out of the issue Files rail.
- **Paused sessions**: a coding session whose machine goes offline now reads as paused with the device name, on the Agents rows, the dock, the issue and the session view, instead of an endless starting spinner. The view reconnects by itself when the machine comes back.
- **Getting started**: the checklist in the sidebar and the desktop rail mirrors the same ten steps everywhere, adds a Create an action step for owners, and hides itself once everything is done. No dismiss button to hunt for.
- **Batch merge**: the merge button on a batch session now targets only its own branch's pull request, never a teammate's, after the session's own PR closed unmerged.
- **Mobile web**: the team pages on a phone use the same layout as the native apps, and the avatar menu lost its duplicate Account entry.`,
  },
  {
    id: `2026-08-emoji-picker`,
    date: `2026-08-19`,
    title: `Emoji, everywhere you write`,
    summary: `An emoji picker in the description editor and the comment composer on web, desktop, iOS and Android, plus :shortcode autocomplete.`,
    body: `- **Emoji picker**: a smiley button in the description toolbar and next to the attach button in the comment composer opens a searchable picker on all four apps, with your recent picks first.
- **:shortcode autocomplete**: type a colon and two letters, like :tad, and pick from the suggestions; typing :tada: in full turns into the emoji straight away, the same way mentions and issue references complete.
- **Skin tones**: choose a skin tone once in the picker and every hand and person emoji follows it, on that device.
- **Plain unicode**: emoji are stored as ordinary characters in the markdown, so an emoji added on one app renders the same on every other and in email.`,
  },
  {
    id: `2026-08-per-user-repo-sharing`,
    date: `2026-08-19`,
    title: `Your repos, shared with the team`,
    summary: `Every member connects their own GitHub repos and shares them with the team, and stale GitHub accounts finally show a Disconnect button.`,
    body: `- **Per-user repo sharing**: any member can now connect GitHub and add repositories, not just owners. You see and connect the repos your own GitHub account can access; connecting one shares it with the team so everyone can code on it.
- **Shared by**: repository rows now show who shared them. The sharer and team owners manage a repo (remove it, pin its default branch); everyone else codes on it.
- **Disconnect stale accounts**: a linked GitHub account that no reconnect can refresh now shows a visible Disconnect button for owners instead of warning forever.
- **Reconnect self-heals**: reconnecting GitHub as an owner also cleans up orphaned account links left behind by old versions, so the reconnect warning actually goes away.`,
  },
  {
    id: `2026-08-action-automations`,
    date: `2026-08-18`,
    title: `Actions run themselves`,
    summary: `Give an action a schedule or an issue-event trigger and a device of yours runs it, plus a tabbed Actions surface with suggestions.`,
    body: `- **Schedules**: run an action every day, week, or month at a time you pick. The run happens on a device you choose, in its local time, and a run missed while the machine was off fires once when it comes back.
- **Event triggers**: start an action when issues change, on creation, status moves, assignee or priority changes, label adds, or pull request opens and merges, with optional board, label, priority, and status filters. Changes that arrive together batch into one run.
- **Runs stay local**: your desktop app or the exponential daemon watches its own sync and starts the run itself. There is no server-side runner, and automated runs carry an Automated badge everywhere.
- **A tabbed Actions surface**: Actions, Automations, and Suggestions tabs on all four apps. The Automations tab shows every trigger, its device, the next run, and recent automated runs.
- **Suggestions**: eight ready-made action ideas, from a daily standup digest to a label janitor. One tap prefills the action creator.
- **Multi-line inputs**: actions can now declare textarea inputs, and the action editor's description grew into one.`,
  },
  {
    id: `2026-08-motion-pass`,
    date: `2026-08-17`,
    title: `Everything moves`,
    summary: `Panels slide, lists reorder, and tabs travel, on all four apps and in one shared timing.`,
    body: `- **Panels slide**: the coding panel on web and the terminal dock in the desktop IDE now open and close with a slide instead of jumping the page. The IDE's left rail expands and collapses the same way, and dragging either one to resize still tracks your pointer exactly.
- **Lists that reorder**: changing an issue's status or a filter now moves rows to where they landed on iOS and Android, instead of having them appear somewhere else.
- **Navigation with direction**: the bottom tab bar's selection travels between tabs, and the first-run setup steps slide forward and back on both phones.
- **One shared timing**: every app now animates on the same three durations and curves, so the same action feels the same everywhere.
- **Respects your settings**: turning on Reduce Motion (iOS), Remove animations (Android) or the browser's reduced-motion preference now switches these off. Android had no such handling before.`,
  },
  {
    id: `2026-08-git-history-view`,
    date: `2026-08-17`,
    title: `Git history in the desktop IDE`,
    summary: `See your repo's commit graph in Source Control, and push safely when trunk has moved on.`,
    body: `- **Commit graph**: Source Control in the desktop IDE now shows the repo's history as a lane graph, including remote branches, with paging for deep histories.
- **Push through a diverged trunk**: commit and push now walks you through a guided rebase when your trunk is both ahead of and behind the remote, instead of failing with a raw git error.
- **Fixes**: the board settings color picker no longer clips at the dialog edge, and steering a session with image attachments can no longer stall the live connection to your phone.`,
  },
  {
    id: `2026-08-selectable-desktop-text`,
    date: `2026-08-16`,
    title: `Selectable text in the desktop IDE`,
    summary: `Select and copy read-only text anywhere in the desktop app, riding a major rendering engine upgrade.`,
    body: `- **Select and copy**: issue descriptions, comments, and file previews in the desktop IDE are now selectable, with sweeps that run across several comments at once. Copying a selected description yields its markdown source, ready to paste anywhere markdown goes.
- **Rendering engine upgrade**: the desktop app moved to the newest UI framework release, bringing correct orientation for photos straight off a phone, IME composition fixes for CJK input, smoother scrolling, and a fix for a class of text layout glitches at very wide windows.`,
  },
  {
    id: `2026-08-steering-images`,
    date: `2026-08-16`,
    title: `Steering images and archivable boards`,
    summary: `Attach screenshots to a live coding session, archive boards you are done with, and steer the newest agent CLIs reliably again.`,
    body: `- **Images in steering**: the session composer now takes up to four images per message via the new attach button (or paste, on web). The machine running the session hands them to the agent as local files, so it sees exactly what you see, and they render inline in the session feed on every app.
- **Archive boards**: owners can archive a board from its settings on web and desktop. An archived board and its whole history disappear from every app until you unarchive it, and nothing is deleted.
- **Steering fixes**: permission prompts from the newest Claude Code versions are answerable again from your phone, including questions that take a typed reply.
- **Offline banner in the IDE**: the desktop app now says when it can't reach the server and is showing cached data, with a one-tap retry.
- **Leaner web composer**: the Esc button is gone; killing the session remains available from the session header.`,
  },
  {
    id: `2026-08-review-hardening-wave`,
    date: `2026-08-15`,
    title: `Stability wave: 40+ fixes across every app`,
    summary: `A deep code review turned into a wave of correctness, performance, and security fixes on web, desktop, iOS, and Android.`,
    body: `- **Merging wraps up the session**: merging a PR now ends its live coding session on every path (webhook, poller, or the Merge button), so machines stop working on branches that already landed.
- **Faster lists**: big boards render instantly on web (virtualized) and much cheaper on desktop; steering a session no longer replays its whole history frame by frame.
- **Editor fixes**: bracket-heavy lines no longer freeze the desktop editor, nested numbered lists keep their numbering on Android, and image alt text with brackets parses correctly.
- **Sync and auth hardening**: attachment storage recovers after a hiccup instead of sticking until restart, login rate limiting is on in production, and a batch of rare account/session edge cases are closed.`,
  },
  {
    id: `2026-08-agent-bug-reports`,
    date: `2026-08-14`,
    title: `Agent bug reports and richer feedback details`,
    summary: `Coding agents can file Exponential bug reports straight from a session, and feedback metadata now shows on every platform.`,
    body: `- **Report bugs from a session**: agents connected over MCP get a report tool that files bugs about Exponential directly to us, credited to you so you get the resolution email. Issues filed this way carry an Agent origin chip.
- **Feedback details everywhere**: the reporter, page, and device details on widget-filed issues now show in the desktop IDE and as an expandable card on mobile, not just on web.`,
  },
  {
    id: `2026-08-pr-notification-attribution`,
    date: `2026-08-13`,
    title: `Better PR notifications`,
    summary: `PR notifications now credit the person who started the work, plus fixes across desktop and mobile.`,
    body: `- **PR notifications attributed**: when an agent opens or merges a PR, the notification now names the person who asked for it, skips notifying them about their own action, and no longer double-fires.
- **IDE**: deleting an issue now closes its open tabs cleanly instead of leaving a dead "not found" tab.
- **Mobile**: the assignee picker and @-mentions only offer current team members.
- **Device settings**: machine settings edited from web or mobile now reach the machine in about a second instead of on the next heartbeat.`,
  },
  {
    id: `2026-08-device-settings-everywhere`,
    date: `2026-08-12`,
    title: `Device settings, remote worktrees, and resume from anywhere`,
    summary: `Every machine gets a proper settings view on all four apps: rename, share, edit its agent defaults even while it is offline, manage its worktrees remotely, and resume sessions from web and mobile.`,
    body: `- **Device settings view**: the Edit entry on a machine opens a full settings dialog (bottom sheet on mobile) with its name, team sharing, agent defaults, and worktrees in one place.
- **Defaults editable from anywhere**: a machine's per-agent model, effort, and permission defaults now live on the server; edit them from web or your phone even while the machine is offline and it converges when it comes back.
- **Remote worktree cleanup**: see every worktree a machine holds, remove one, or prune everything already merged; commands queued against an offline machine run on its return.
- **Resume from web and mobile**: starting coding on an issue that already has a worktree offers "Resume previous session" outside the desktop app too.
- **Live machine list**: machines now sync in realtime instead of polling, with online status derived from their heartbeats.`,
  },
  {
    id: `2026-08-worktree-autoclean`,
    date: `2026-08-11`,
    title: `Tidy worktrees and a reworked Actions view`,
    summary: `The desktop IDE now cleans up landed worktrees and stale branches automatically, and its Actions surface matches the web agents page.`,
    body: `- **Worktree auto-clean**: landed session worktrees and their branches are pruned automatically once git confirms the work is in the default branch; anything with uncommitted or unpushed work always survives.
- **Actions as cards**: the desktop Actions surface is now the web agents page 1:1, with a card grid, owner edit dialog, and Run on every card.
- **Safer file previews**: binary files no longer freeze the viewer; the placeholder offers opening the file in its default app or revealing it in your file manager.
- **Smarter PR notifications**: pull request activity from your own agent no longer pings you, and webhook events are attributed to the session owner.`,
  },
  {
    id: `2026-08-pi-plan-mode`,
    date: `2026-08-10`,
    title: `Plan mode for pi`,
    summary: `pi sessions can now start in plan mode: the agent investigates read-only, proposes a plan, and waits for your approval before touching code.`,
    body: `- **Plan-first pi runs**: the launcher injects a plan-mode extension that blocks edits until you approve the plan pi presents in its terminal dialog.
- **Approve from anywhere**: the plan shows up as the familiar plan card in the session view, answerable from web, iOS, and Android.
- **Per-agent default**: pi gets its own Plan mode toggle under Settings, Agents on the desktop, on by default like Claude's.
- **Pin a default branch**: repository settings on web and desktop can pin the branch coding sessions branch from and PRs target, overriding GitHub's default everywhere.`,
  },
  {
    id: `2026-08-remote-permission-approval`,
    date: `2026-08-09`,
    title: `Approve agent permissions from anywhere`,
    summary: `Permission and approval prompts are now answerable from web, iOS, and Android instead of pausing the session until you reach the machine.`,
    body: `- **Claude permission prompts**: sessions started without permission bypass surface the "Do you want to proceed?" dialog as an answerable card in the steer view, with the dialog's real options.
- **Codex approvals**: command, edit, and network-access approval modals are answerable remotely the same way.
- **Steer with words instead**: sending a message while a prompt is up dismisses it safely and delivers your text as the "tell the agent what to do differently" feedback.
- **Settings slide into the sidebar**: team settings now open as a slide-in navigation in the sidebar on web and desktop instead of a separate page rail.`,
  },
  {
    id: `2026-08-machine-launch-defaults`,
    date: `2026-08-08`,
    title: `Start coding with your machine's own defaults`,
    summary: `The remote Start coding dialog now pre-fills the model, effort, plan mode, and permission settings you configured on the machine you pick.`,
    body: `- **Machine defaults everywhere**: pick a desktop or server in the Start coding dialog on web, iOS, or Android and it loads that machine's saved per-agent defaults, including plan mode and permission bypass.
- **Per agent**: switching the agent tab re-seeds model, effort, and toggles from the selected machine's settings for that agent.
- **Live**: change a default in the desktop app's agent settings and remote dialogs pick it up right away.`,
  },
  {
    id: `2026-08-one-status-order`,
    date: `2026-08-08`,
    title: `Statuses read the same everywhere`,
    summary: `Issue lists now group statuses in the order your settings page lays them out, on web, desktop, iOS, and Android.`,
    body: `- **One status order**: issue lists group by status in the same order the statuses settings page shows, starting with Backlog and Todo instead of the started statuses.
- **Everywhere at once**: lists, status pickers, and the settings page all read the same top to bottom, on all four apps.`,
  },
  {
    id: `2026-08-device-sharing-remote-login`,
    date: `2026-08-07`,
    title: `Shared machines, remote sign-in, and widget themes`,
    summary: `Share a server machine with your team for remote coding sessions, sign the agent CLI in from anywhere when a session asks for it, and give the feedback widget your own theme and labels.`,
    body: `- **Share a machine with your team**: mark a server device as shared and teammates can start coding sessions on it, each run owned and steered by whoever started it.
- **Remote sign-in**: when the agent CLI on a remote machine needs a login, the sign-in link surfaces in the live session feed so you can authorize without walking over.
- **Widget themes and labels**: the feedback widget takes light, dark, or auto themes with custom colors, and reporters can tag their feedback with labels you pick.
- **Create actions anywhere**: a New action button on the Agents page starts the creator run that writes the action for you.
- **Consistent move confirms**: moving an issue to another board asks the same way from the list menu and the detail sidebar.`,
  },
  {
    id: `2026-08-unified-settings-api-keys`,
    date: `2026-08-07`,
    title: `One settings page, API keys, and a skill for your agents`,
    summary: `Team and account settings merge into one page with a settings button next to your name, personal API keys get a self-service section, and agents can read SKILL.md and set custom statuses over MCP.`,
    body: `- **One settings page**: account, notifications, and API keys now live in a Personal group inside the settings page, on web and desktop, and the web settings button moved to the sidebar bottom next to your name.
- **Personal API keys**: create and revoke expu_ keys under Settings, for MCP clients, scripts, and non-interactive CLI logins via EXP_TOKEN.
- **Custom statuses over MCP**: agents can list a team's statuses and set them precisely with statusId when updating issues.
- **SKILL.md for agents**: exponential.at/SKILL.md gives any coding agent the full picture of the product, the MCP server, the CLI, and the feedback widget.
- **CLI sign-in**: password login is gone; use the device code flow, or an API key via EXP_TOKEN for scripts.`,
  },
  {
    id: `2026-08-board-rename-widget-pictures`,
    date: `2026-08-05`,
    title: `Rename boards, attach pictures to feedback`,
    summary: `Boards can be renamed in place, the feedback widget accepts picture attachments, and machines with an outdated daemon get a one-click update button.`,
    body: `- **Rename boards**: a gear on the sidebar board row opens board settings, so owners can rename a board without leaving the list, on web and desktop.
- **Pictures in feedback**: the feedback widget now takes up to three picture attachments alongside the screenshot, and large submissions no longer fail with an opaque network error.
- **Update your machines**: the machines list shows an Update button for devices running an outdated daemon, and it triggers the self-update remotely.
- **Issue page polish**: the assignee name no longer gets cut off mid-word, long label names ellipsize in the sidebar, and status pickers list statuses in their settings order.`,
  },
  {
    id: `2026-08-issue-link-chips`,
    date: `2026-08-05`,
    title: `Issue links become chips`,
    summary: `Issue references render as live status chips, the issue page gets a cleaner header with a sticky toolbar, and dropped files land right where you drop them.`,
    body: `- **Issue link chips**: #ABC-123 references in descriptions and comments now render as chips showing the linked issue's status, on web and desktop.
- **Cleaner issue page**: the title and formatting toolbar stay pinned while you scroll, and the desktop app folds the old right sidebar into a compact header.
- **Drop files in place**: files dragged into the editor insert at the drop position, and images can be dragged to reorder them.
- **Activity feed**: a "created the issue" row now leads the timeline.
- **Desktop**: the About page's licence notices no longer freeze the app while opening.`,
  },
  {
    id: `2026-08-desktop-windows-tools`,
    date: `2026-08-05`,
    title: `The desktop app finds your tools on Windows`,
    summary: `Windows now picks up a freshly installed git or Claude without a restart, background git calls no longer flash console windows, and sign-in shows the right methods immediately.`,
    body: `- **Windows tool detection**: Check tools now sees a just-installed git or Claude without restarting the app, and the native Claude installer's claude.exe is found reliably.
- **No more console flashes**: background git and probe commands on Windows run hidden instead of flashing terminal windows.
- **Sign-in without the flash**: the desktop login no longer shows a password form briefly before the real sign-in methods load.
- **Search dialog**: the desktop search dialog is dismissable by clicking outside and keeps a sensible height on long result lists.`,
  },
  {
    id: `2026-08-mobile-login-and-batch-bar`,
    date: `2026-08-04`,
    title: `Mobile stays signed in`,
    summary: `The iOS app no longer loses your session on restart, the mobile multi-select bar floats above the bottom menu, and the welcome screen signs you in directly.`,
    body: `- **Staying signed in**: the iOS app could lose its session on every restart. The cause is fixed and the release pipeline now verifies every build before upload.
- **Floating multi-select**: selecting issues on iOS and Android no longer pushes the list down; the action bar floats above the bottom menu.
- **Direct sign-in**: the mobile welcome screen shows Continue with Apple and Google immediately, with self-hosted servers one tap away.
- **Share sheet polish**: sharing into the app while signed out no longer shows two Cancel buttons.`,
  },
  {
    id: `2026-08-exponential-cli-servers`,
    date: `2026-08-03`,
    title: `Run coding agents on your own servers`,
    summary: `The new exponential CLI turns any Linux or macOS machine into a registered agent runner: install with one command, sign in with a device code, and start sessions on it from the web.`,
    body: `- **The exponential CLI**: a headless companion to the desktop app. Install it with one command, sign in with a device code from any browser, and run coding sessions and actions from your terminal with the same three agents.
- **Registered machines**: every client now shows all of your machines, desktops and servers alike, with live online state, a last-seen time when offline, and each machine's version. Rename or remove them from the overflow menu.
- **Add server**: a copyable one-liner installs the CLI on a server and registers it to your account; the daemon picks up remote starts exactly like the desktop app, including live steering and activity.
- **Stays current**: the CLI offers automatic updates on first run, restarts itself on new releases only while idle, and each server row has an Update button that triggers an update remotely.`,
  },
  {
    id: `2026-08-digest-schedule-mobile-polish`,
    date: `2026-08-01`,
    title: `Email digests keep to your schedule`,
    summary: `Notification emails now wait for your chosen digest hour, comment editing is author-only everywhere, and the Agents surface shows just your own sessions.`,
    body: `- **Digest timing fixed**: notification emails no longer go out minutes after a notification arrives; the daily digest waits for your chosen hour and bundles everything still unread.
- **Author-only comments**: editing and deleting a comment is now reserved for its author on every client.
- **Your agents only**: the Agents list, tab dots, and counts show only your own coding sessions; the coding-now badge on issues still covers the whole team.
- **Mobile polish**: switching boards refreshes instantly, action icon inputs use the curated icon picker, comment rows lost their stray padding, and the Android app is a smaller download.`,
  },
  {
    id: `2026-08-mobile-session-recovery`,
    date: `2026-08-01`,
    title: `Mobile apps recover from a signed-out session`,
    summary: `If your session is removed on the server, iOS and Android now return you to the login screen instead of getting stuck on an error.`,
    body: `- **Session recovery**: when the server no longer recognizes your session (for example after an account was removed), the mobile apps now sign that account out locally and land on the login screen, matching the desktop app.
- **Onboarding escape hatch**: the first-run wizard's error cards gained a Sign out button, so a connection stuck on an error is never a dead end.`,
  },
  {
    id: `2026-08-github-connect-polish`,
    date: `2026-08-01`,
    title: `Smoother GitHub connect and steadier steering`,
    summary: `Connecting GitHub from mobile now continues straight to the install page, remote steering question cards stopped jumping, and copy got a polish everywhere.`,
    body: `- **GitHub connect on mobile**: if the GitHub app isn't installed yet, connecting from iOS or Android onboarding and settings now continues straight to the install page instead of stopping on an error.
- **Steadier steering**: question cards from remotely steered coding sessions no longer jump or go missing when the terminal picker overflows the pane.
- **Polish**: crisper avatar initials on mobile and clearer wording across all apps, the widget, and the marketing site.`,
  },
  {
    id: `2026-07-about-licenses`,
    date: `2026-07-31`,
    title: `About screens with open-source acknowledgements`,
    summary: `Every app now has an About surface: version, source and license links, and the full third-party license notices bundled with that build.`,
    body: `- **About everywhere**: the desktop IDE (Settings → About), iOS and Android (Settings → General → About) and the web app (user menu → About) now show what you're running, with links to the source code and the Apache-2.0 license.
- **Third-party licenses**: each build ships and displays the complete license notices for the open-source components it bundles; on the web they're also downloadable at /NOTICES.txt.
- **Version at a glance**: desktop and mobile show their build version right on the About row.`,
  },
  {
    id: `2026-07-codex-pi-steering`,
    date: `2026-07-31`,
    title: `Watch and steer Codex and Pi sessions`,
    summary: `Codex and Pi coding sessions now stream a live activity feed (prompts, tool calls, questions, diffs) and take steering messages from web and mobile, just like Claude.`,
    body: `- **Codex sessions go live**: watching a Codex session from web or mobile now shows the agent's narration, tool calls, your prompts, and worktree diffs as they happen, instead of an empty feed. Codex questions appear as cards in the feed; answer them by replying in chat. Approval prompts are still answered in the desktop terminal.
- **Pi sessions too**: Pi sessions report the same live activity, and steering messages are delivered through Pi's own message queue. Sent mid-run they arrive at the next tool boundary; sent while idle they start a new turn.
- **Needs-input badges everywhere**: Codex and Pi sessions now flip the "Needs input" badge when they finish a turn or wait on a question, so you can tell from any device when the agent is parked.`,
  },
  {
    id: `2026-07-claim-flow-digest-hour`,
    date: `2026-07-31`,
    title: `Pick exactly what GitHub connects, and when email arrives`,
    summary: `The GitHub claim page now links and unlinks installations in one save, email digests arrive at your preferred hour in your timezone, and the desktop IDE got sturdier session and repo management.`,
    body: `- **GitHub claims, refined**: connecting GitHub now shows every installation you control and saves links and unlinks together in one step; the desktop app hands you straight back when you finish in the browser.
- **Digest hour**: the unread-notification email digest now sends at your preferred hour in your own timezone. The desktop app picks up your system timezone automatically; adjust both in notification settings.
- **Desktop IDE**: "Merge and close" ends the session before merging, so Fix conflicts is available immediately if the merge fails; local repositories and worktrees are manageable from Settings, with live coding sessions protected from accidental removal.
- **First-party analytics on the cloud**: app.exponential.at now measures its signup funnel with cookieless first-party counters; self-hosted instances record nothing. Details on the privacy page.`,
  },
  {
    id: `2026-07-desktop-onboarding`,
    date: `2026-07-30`,
    title: `A guided start in the desktop IDE`,
    summary: `The desktop app now walks first-run users from team to board to a working toolchain, with a setup doctor that says exactly what's missing.`,
    body: `- **First-run wizard**: a fresh desktop install now guides you through creating or joining a team, setting up your first board (with optional GitHub repository connect), and getting your coding tools ready, so you never land in an empty window.
- **Setup doctor**: Settings → Tools gained a doctor panel that probes git and every coding agent (Claude, Codex, Pi), with per-tool install hints and inline path overrides. The onboarding tools step uses the same panel, and existing installs see it once after updating.
- **Honest start-coding buttons**: with no agent installed, Start-coding affordances are disabled with an explanation instead of failing later, and the device stays out of remote-start pickers until an agent is available.
- **Clone failures surface**: a failed repository clone now shows its error and a Retry button in Source Control, and retries automatically once git is installed.`,
  },
  {
    id: `2026-07-github-connect-selfheal`,
    date: `2026-07-30`,
    title: `GitHub connections that fix themselves`,
    summary: `Stale GitHub links now heal on reconnect, fresh installs stop losing their repos, and adding a repository got an explicit Add button with visible errors.`,
    body: `- **Stale links heal themselves**: a leftover connection to a GitHub account you don't control (and the eternal "Reconnect GitHub" warning it caused) is now cleaned up automatically the next time you reconnect, and the warning names the affected account. The account list, with its disconnect button, stays visible while the warning shows.
- **Fresh installs keep their repos**: connecting a brand-new GitHub installation could race GitHub's own bookkeeping and come back with zero repositories until a manual reconnect. The connect flow now rides that out.
- **A real Add button**: the Add-repository dialog selects on click and connects via an explicit **Add repository** button; failures show inside the dialog instead of vanishing with it, and the double-scrolling is gone.
- **Suspended installations say so**: all four apps now distinguish a GitHub-side App suspension (fix it on GitHub) from a stale connection (reconnect), instead of nudging the wrong fix or, on desktop, showing a bogus "repository limit reached" upsell.
- **Mobile connect works signed-in-natively**: connecting GitHub from iOS/Android no longer requires a separate web login in the in-app browser, and every connect outcome hands you back to the app.`,
  },
  {
    id: `2026-07-selfhost-default-no-dogfood`,
    date: `2026-07-30`,
    title: `Self-hosted by default, no more special boards`,
    summary: `Fresh instances start truly empty (no seeded teams, boards, or widgets), and self-hosted mode no longer needs a flag.`,
    body: `- **Self-hosted is the default**: the \`SELF_HOSTED\` flag is gone; every plan limit is off unless the deployment explicitly opts into cloud mode. Existing self-host setups keep working without changes.
- **No seeded content**: the boot-time feedback team, board, and widget bootstrap has been removed entirely. What you create is all there is.
- **Protected boards are gone**: every board behaves the same now. Owners can delete any board, and its repository can always be changed.`,
  },
  {
    id: `2026-07-github-claim-ownership`,
    date: `2026-07-30`,
    title: `GitHub connections locked to owners`,
    summary: `Connecting GitHub now requires owning the installation. Collaborator access on someone else's repos can no longer claim their account for your team.`,
    body: `- **Ownership-verified connections**: connecting GitHub to a team now proves you control the installation. Personal installations must belong to the GitHub account you authorize, and organization installations require an active org membership. Being a collaborator on someone else's repository no longer surfaces their account or repositories in your team.
- **A way out of every dead end**: if the account you authorized has no installation of its own, the connect flow now explains exactly what happened and offers a one-click **Install on GitHub** button that takes you to GitHub's account and repository selection.
- **One-time approval for organizations**: the GitHub App now asks for read-only organization-members access (used solely to verify your membership); org installations need to approve the updated permissions once.`,
  },
  {
    id: `2026-07-merge-and-close-fix`,
    date: `2026-07-30`,
    title: `Merges unstuck`,
    summary: `Cloud PR merges could fail with a database error. That's fixed, and finished subagent tabs now tidy up after themselves.`,
    body: `- **"Merge and close" works again**: a skipped schema migration made every PR merge on the cloud fail with a database error (and left the session's terminal open). The migration now applies correctly, and a new guard test keeps migration ordering honest so this class of bug can't sneak back in.
- **Subagent tabs clean up**: completed subagents (like quick explore runs) no longer linger in the desktop session view after they finish.
- **A fresh coat of paint for the homepage movie**: the exponential.at hero video now matches the app's glassy design, pixel for pixel.`,
  },
  {
    id: `2026-07-session-close-on-merge`,
    date: `2026-07-29`,
    title: `Sessions survive the merge`,
    summary: `Merging a PR no longer kills the coding session. Close it when you're done, with the new "Merge and close" button.`,
    body: `- **Merge without losing the session**: merging an agent's PR used to end its coding session immediately. Now the session parks in a new **Merged** state and stays fully alive: keep steering it, ask follow-up questions, or let it finish cleanup.
- **"Merge and close"**: every live session row (web, desktop, iOS, Android) gets an explicit button that merges the PR *and* tears the session down in one step, for when you really are done.
- All other merge buttons are now merge-only and leave the session running.`,
  },
  {
    id: `2026-07-steering-subagents`,
    date: `2026-07-29`,
    title: `Watch subagents work, live`,
    summary: `Agent sessions show every subagent as its own tab on web, desktop, iOS, and Android.`,
    body: `- **Subagent tabs**: when your coding agent fans out into subagents, the live session view gives each one its own tab with a readable name and its tool activity, instead of a flood of anonymous "agent" cards. Works on all four clients.
- **Queued follow-ups show up**: messages you send while the agent is busy now appear in the session feed instead of vanishing until the agent picks them up.
- **A steadier "Needs input" badge**: the desktop no longer flags a session as waiting while a background subagent is still working, and the badge retries until it lands.
- **Live images in the IDE**: images in issue descriptions render the moment their upload finishes, including ones pasted from another device.
- **Actions remember their repository**: running a saved action pre-fills its bound repo in the run dialog on every client.`,
  },
  {
    id: `2026-07-apache-2`,
    date: `2026-07-29`,
    title: `Exponential is now open source`,
    summary: `The whole repo moved to Apache-2.0. Self-hosting is free for everyone, with optional Enterprise Support.`,
    body: `- **Apache-2.0, fully open source**: the Exponential Small Team License is gone. Read it, change it, and self-host it in production for free, at any company size.
- **No more headcount math**: the 10-person limit and the mandatory commercial license are deleted. Nothing changes for cloud plans.
- **Enterprise Support is now an optional add-on**: self-hosters who want an SLA, priority support, deployment help, or custom development can get a support contract at the same published prices.
- One honest caveat remains: self-hosted instances can't push notifications to the store mobile apps (they're built against our Firebase project). Web and desktop are fully featured either way.`,
  },
  {
    id: `2026-07-selfhost-and-billing-portal`,
    date: `2026-07-29`,
    title: `Self-host with one docker compose`,
    summary: `A pinnable self-host image with an agent-followable install guide, a customer billing portal, and a faster desktop IDE.`,
    body: `- **Self-hosting is now a docker pull**: the cloud and self-host builds are the same published image, pinned by version, with a minimal compose file and an INSTALL.md your coding agent can follow end to end. Free for companies under 10 people.
- **Manage billing yourself**: team owners can open a customer portal from team settings → Billing to view invoices and update payment details.
- **Answering agent questions from your phone is reliable now**: plan approvals and question picks from iOS/Android land on the desktop with a visible confirmation, and retry automatically instead of silently vanishing.
- **Desktop IDE polish**: git history on big repos scrolls smoothly, a merged PR pulls master onto every IDE immediately, a parked trunk (detached head, local commits, dirt) now says so instead of going silently stale, and the repositories settings page looks the same on every platform.
- Attach any file straight from the editor toolbar on web and desktop.`,
  },
  {
    id: `2026-07-new-pricing`,
    date: `2026-07-28`,
    title: `New pricing: bring two friends`,
    summary: `The free plan now has 3 seats, and the paid tiers collapsed into one Team plan with everything included.`,
    body: `- **Free is a team now**: every free team gets **3 seats** instead of 1, with the same 250 MB of attachment storage and a feedback widget. Real-time collaboration is the point of Exponential; now you can try it without paying.
- **One paid plan**: Pro and Business merged into a single **Team** plan at **€15/seat/mo**, or **€12/seat/mo billed yearly**. Everything is included: 10 GB attachment storage, unlimited feedback widgets, the helpdesk & support inbox, and priority support. Existing complimentary tiers carry over automatically.
- **Self-hosting stays free under 10 people**, and commercial self-host licensing now has published pricing instead of "email us".
- Enterprise needs (SSO, SLA, DPA, self-host contracts) live behind one Talk-to-us line instead of a fourth plan card.`,
  },
  {
    id: `2026-07-one-way-to-attach`,
    date: `2026-07-28`,
    title: `One way to attach, on every screen`,
    summary: `Mobile stops asking whether a file is an image, and the issue page loses its clutter.`,
    body: `- **One attach button on iOS and Android**: attachments arrived with their own paperclip next to the description's image button, so adding something to an issue meant guessing which of two gestures to use, and picking an image through the paperclip dead-ended in an error. The description toolbar's button now offers Files or Photo library and sorts the pick itself: an image goes into the description, anything else becomes an attachment. The paperclip is gone.
- **New issues take files too**: attaching a file while composing a new issue now holds it as a draft and uploads it the moment the issue is created, exactly like draft images already worked.
- **No more empty Files block**: an issue with nothing attached shows nothing at all instead of a permanent "No files attached" row. Failed uploads still show up so you can retry them.
- **A calmer issue page**: the rule above Activity now spans the full pane on web and desktop, the linked PR row runs the full width instead of hugging its text, and the mobile issue menu is a single \`…\` that holds Share, notifications and Move to board, with words instead of an ambiguous bell.
- **Status settings say less**: the issue-statuses pane dropped the explanatory paragraphs the rows already made obvious, the two cards no longer sit flush against each other, and the PR-automation pickers line up.`,
  },
  {
    id: `2026-07-custom-statuses`,
    date: `2026-07-28`,
    title: `Statuses that match how you work`,
    summary: `Build your own issue statuses per team, and decide what opening or merging a PR does to them.`,
    body: `- **Custom issue statuses**: a team can now add its own statuses next to the seven built-in ones. Each new status picks a category (Backlog, Unstarted, Started, Completed, Cancelled), a name and a color; up to four Started statuses get their own progress-clock icons. Built-ins stay put (they can be reordered inside their category but never renamed or deleted), and every app groups, sorts and filters by your statuses identically. Manage them in team settings on web or in the IDE.
- **Deleting a status always asks where its issues go**: one consistent dialog on web and desktop, with the real count of affected issues (including issues on boards in the trash, which your apps never see).
- **PR automation is yours to configure**: opening a PR moved issues to In Review and merging completed them. Now each team picks which status a PR open or merge sets, or turns either off entirely. The defaults are unchanged, so nothing moves unless you change it.
- **Merge failures you can actually read**: a merge that hits a conflict now says so inline on web, iOS and Android, right where you tapped, with a Fix conflicts button beside it instead of a message hidden behind the navigation bar.
- **Issue chips in the mobile editors**: typing \`#EXP-42\` in a description on iOS or Android now renders as a chip with the issue's title, and the \`#\` suggestion list stays where it belongs and can be dismissed.
- **Consistent icons everywhere**: the settings pages in the web app and the IDE now draw from one shared icon set, so the same section no longer wears two different glyphs.
- **Desktop IDE**: terminal tabs of a coding session show the issue's status, identifier and live title, and reveal a Merge PR button on hover; the tab strip now runs to the right edge and only collapses into "+N" when a tab genuinely does not fit.`,
  },
  {
    id: `2026-07-attach-any-file`,
    date: `2026-07-27`,
    title: `Attach any file`,
    summary: `Issues now take PDFs, videos and other files, plus seamless steering and a rounder IDE.`,
    body: `- **Attach more than screenshots**: issues and comments now take any file up to 50MB. PDFs, videos, audio, logs, archives. A new Files section on the issue collects them, and team owners get a Storage pane in settings that shows what is using space and can sweep images nothing references anymore.
- **Steering without ceremony**: the "Remote steering" banner and its Take over button are gone. A live coding session is now visible and steerable only from the account that started it; teammates see the synced status badge instead. Update your apps. Builds from before this release degrade to a read-only view of their own sessions until updated.
- **Issue chips show their title**: a \`#EXP-42\` mention now renders the issue's title alongside the identifier in every editor and comment, and typing \`=>\` or \`->\` turns into a real arrow.
- **Safari menus fixed**: dropdown submenus (like Delete issue → Confirm delete) were invisible in Safari.
- **Desktop IDE polish**: tab chips show issue identifiers with smarter overflow, your account avatar shows up in the rail, dialogs feel native on every platform, and a conflicted issue offers its Fix conflicts button right on the detail page.`,
  },
  {
    id: `2026-07-instant-sync-on-open`,
    date: `2026-07-27`,
    title: `Current the moment you open it`,
    summary: `Opening the app used to show ten-second-old data. All three apps now sync over one connection instead of fifteen.`,
    body: `- **No more ten-second wait**: opening iOS, Android or the desktop app showed you the state from before you closed it, and only caught up around ten seconds later. Each of the fifteen synced tables was opening its own connection, so every launch fired fifteen simultaneous DNS lookups and TLS handshakes at once. Enough of them failed that the retries were the delay. All three apps now put every table on a single HTTP/2 connection.
- **A VPN or a waking radio costs a second**: a connection that is not usable *yet* used to be treated like a server error, parking sync on a backoff of up to thirty seconds long after the network had come back. Those failures now retry immediately, and Android additionally wakes sync when a VPN finishes connecting.
- **"Syncing…" tells the truth**: on Android the indicator used to disappear on a fifteen-second timer whether or not anything had synced, and pull-to-refresh always spun for a full five seconds. Both now finish as soon as the server confirms you are up to date.
- **Smaller downloads**: sync responses are compressed for clients that support it, which mostly shows up the first time a device populates its cache.`,
  },
  {
    id: `2026-07-subscription-belongs-to-team`,
    date: `2026-07-27`,
    title: `Your plan belongs to the team`,
    summary: `Deleting an account no longer cancels a shared team's paid plan, and you can cancel a subscription in-app.`,
    body: `- **A shared team keeps its plan**: deleting the account of whoever bought the subscription used to cancel it immediately, dropping the team to Free mid-period with no warning. A subscription belongs to the team now: it keeps running and the remaining owners keep managing it. Only a team that is deleted along with its last member takes its plan with it.
- **Cancel (and un-cancel) in the app**: team settings → Billing gets a Cancel plan button that ends the subscription at the end of the paid period, shows the date it lapses, and lets you resume before then.
- **Delete a team, cancel first**: deleting a team with a live subscription is refused with a message pointing at Billing, instead of quietly leaving a subscription behind that nothing could reach.
- **Nothing breaks when someone leaves**: images they uploaded into your issues stay where they are instead of turning into broken embeds, and mentions of their email address are replaced with a neutral handle.`,
  },
  {
    id: `2026-07-one-icon-set`,
    date: `2026-07-26`,
    title: `One icon set, everywhere`,
    summary: `Every icon is now the same on web, iOS, Android and desktop, and actions get an icon of their own.`,
    body: `- **The same icons on every device**: search, inbox, reviews, agents, support and settings used to be drawn from a different icon library on each platform, so the same thing looked different depending on where you opened it. All four apps now render one shared set. Android changes the most.
- **Board icons you can actually pick**: the picker grew from 16 icons to 60, with a search field, and every one of them draws the same glyph on every device. On the desktop app 11 of the old 16 were quietly substituted with the nearest available shape (a bug was a dot, a wrench was a pencil, and "terminal" drew the same icon as "code"). They are all correct now.
- **Actions have icons**: give an action its own glyph so a long list stays scannable. The "Create action" flow asks for one up front, and you can change it later from the action's editor.`,
  },
  {
    id: `2026-07-desktop-settings-and-tabs`,
    date: `2026-07-26`,
    title: `Settings that fit, tabs that remember`,
    summary: `The desktop IDE gets a real settings section, tabs that remember where you opened them from, and a Linux window that stops shrinking.`,
    body: `- **Settings, restructured**: every board gets its own settings page with web parity (name, icon, color, repository, an honest 48-hour move-to-trash), the coding pane splits into Tools and Agents, and a read-only Plan & Billing card shows seats, storage and widget usage at a glance.
- **Pick your terminal shell**: a new setting chooses which shell the embedded terminal spawns; leave it blank for the platform default.
- **Tabs remember where they came from**: clicking a tab re-selects the sidebar entry (and board) it was opened from, issue tabs carry the issue title instead of just the identifier, and chips that no longer fit collapse into a "+N" menu.
- **Calmer dialogs**: the new-issue dialog opens compact and grows with what you type, the caret stays visible while typing or pasting, and the Start-coding dialog's Cancel/Start bar stays pinned instead of scrolling out of reach. Status, priority, assignee, label and due-date pickers are shared with the sidebar now, so the dialog's label picker finally has search.
- **Linux windows behave**: the main window no longer shrinks 24px on every launch on X11, and all four corners are genuinely round instead of hiding square notches behind the arc.
- **Real glass**: the desktop blurs what sits behind it on macOS and KDE Wayland, and the bulk-select bar floats over the list instead of shoving every row down.`,
  },
  {
    id: `2026-07-stable-terminals-native-dialogs`,
    date: `2026-07-26`,
    title: `Sessions that survive a closed lid`,
    summary: `A sleeping laptop no longer kills a running coding session, desktop dialogs are real windows, and the IDE's left rail can show its labels.`,
    body: `- **Your coding session survives sleep**: a laptop that slept (or a network that stalled) for more than 90 seconds used to make the desktop tear down the terminal and kill the agent the moment it woke. The connection dropping is now treated as what it is (a dropped connection), and the session reconnects instead.
- **Dialogs are real windows**: new issue, start coding, search, create board/team, image preview and every confirm now open as native windows, a sheet on macOS and a proper owner-modal window on Windows and Linux, each with working Escape, Enter and close button.
- **A rail that can speak**: the desktop's left rail expands to show labels next to the icons, and remembers the choice. Settings take over the tool column as their own screen instead of a cramped panel.
- **Select text like text**: dragging in an issue description selects from the click that entered the editor, double-click picks a word, triple-click a line, and clicking beside a block puts the caret where you aimed.
- **Readable terminal colors**: terminal output painted in ANSI black was effectively invisible on the dark background; it now sits well above the contrast floor.
- **Sharper badges on iOS**: count badges and the selected agent chip use the stronger indigo, so their white text clears the accessibility contrast floor.`,
  },
  {
    id: `2026-07-glass-polish`,
    date: `2026-07-26`,
    title: `Glass, second coat`,
    summary: `Menus read clearly over busy content, actions get a screen of their own in the desktop IDE, and you can finally type in the space around an image.`,
    body: `- **Legible menus**: dropdowns, context menus and popovers are far less see-through, so a menu opened over a dense issue list stays easy to read. Highlighted rows use the same glass wash as the rest of the app.
- **Actions have a detail screen**: open an action in the desktop IDE to read its full prompt, see its repository and inputs, and edit or run it from one place instead of a cramped list row.
- **Tabs moved into the titlebar**: the desktop IDE reclaims a row of vertical space, and terminal tabs scroll instead of pushing the controls off screen.
- **Type around images**: click or arrow into the space above, below or between images in an issue description and just start typing.
- **Small fixes**: keyboard-shortcut hints in tooltips are readable again, and the mobile editor toolbar no longer grows a scrollbar in Safari.`,
  },
  {
    id: `2026-07-glass-redesign`,
    date: `2026-07-25`,
    title: `A new coat of glass`,
    summary: `Web and desktop now match the mobile apps' glass look: a dark gradient backdrop, frosted panels, rounder corners, indigo accents, and the desktop draws its own window chrome.`,
    body: `- **Glass everywhere**: popovers, dialogs and sheets float as frosted panels over a fixed dark gradient, on web and in the desktop IDE.
- **Rounder, calmer chrome**: corners follow the mobile radius ladder, small buttons and sidebar items are capsules, and the sidebar melts into the backdrop with an indigo-tinted active state.
- **Denser lists, softer lines**: issue rows keep their density but get hairline dividers, a glass hover wash, and blurred sticky group headers.
- **One indigo**: the accent color is now a shared design token across web, desktop, iOS and Android.
- **Desktop window chrome**: the IDE draws its own titlebar with embedded window controls on macOS, Windows and Linux, with rounded corners and a proper shadow on Linux.`,
  },
  {
    id: `2026-07-steering-v2`,
    date: `2026-07-25`,
    title: `Answering the agent, properly`,
    summary: `Questions from a coding session now arrive as a real stepper (one at a time, with descriptions, multi-select and a submit step), and answering locks the card instead of guessing keystrokes.`,
    body: `- **A real question stepper**: a run that asks you several things now shows one question at a time with "2 of 3" progress, per-option explanations, multi-select toggles, and a final review step with a "Submit answers" button. Answered questions collapse behind what you picked.
- **Answers land where you meant them**: an answer is sent as an answer, not as raw keypresses, and the agent confirms it. Tapping an option locks the card instantly, so one tap can never spill over into the next question.
- **Plans always read as plans**: a plan waiting for approval is rendered as formatted text every time, and it stays waiting until you actually answer it. Sending a message mid-plan no longer makes the approval buttons disappear.
- **See what the subagents did**: work an agent hands to a subagent groups under its own expandable row, and permission prompts show up as their own line in the feed.
- **A feed that survives reconnects**: reconnecting no longer blanks the session view; the desktop republishes the full history and the feed only clears when it is genuinely replaced.`,
  },
  {
    id: `2026-07-fix-conflicts-and-widget-fields`,
    date: `2026-07-25`,
    title: `Merge conflicts fixed for you, widgets that ask for less`,
    summary: `A one-click action rebases and merges a conflicted pull request, actions sync live everywhere, and the feedback widget can collect a name and your own custom fields.`,
    body: `- **"Fix merge conflicts"**: a built-in action that takes a conflicted pull request, rebases it onto your default branch, resolves the conflicts, pushes, and merges it, completing every issue on that PR. Start it from the desktop Reviews list or from any client's launch dialog.
- **Merge from the sidebar**: an open pull request can now be merged straight from the issue sidebar on web and desktop. Merging ends the issue's live coding session instead of leaving a terminal running.
- **Actions sync live**: your team's actions now arrive on every client the moment they change, and running one no longer asks each device to trust it first.
- **Widgets can ask for a name**: the feedback widget can collect a reporter's name without demanding an email, define up to eight custom fields of your own, and be driven entirely from your own UI with the new \`submit()\` API.
- **Roomier dialogs**: the action editor, widget settings, launch and issue dialogs use the width of your screen instead of stretching into tall towers.`,
  },
  {
    id: `2026-07-actions-launch-dialog`,
    date: `2026-07-25`,
    title: `One dialog for starting anything`,
    summary: `Issues and actions now start from the same dialog on every client, actions can ask for inputs, and mobile sync catches up the moment you open the app.`,
    body: `- **One launch dialog**: "Start coding" and "Run action" merged into a single dialog with Issues and Actions tabs, sharing the same device, agent, model, effort and toggle options. Actions finally get the full option set instead of running on defaults.
- **Actions can ask for inputs**: an action can declare text, repository and board inputs, and the dialog prompts for them before the run starts. Values are resolved and checked against your team on the server.
- **Actions moved into Agents**: the separate Actions page is gone; your actions now live alongside your desktops on the Agents page, with a built-in "Create action" card that writes a new action for you.
- **Faster mobile sync**: iOS and Android now catch up immediately when you open the app, regain a connection, or receive a push, instead of waiting out a stale poll. Opening an issue from a notification no longer strands you on a blank loading screen.`,
  },
  {
    id: `2026-07-desktop-editor-toolbar`,
    date: `2026-07-25`,
    title: `A formatting toolbar for desktop descriptions`,
    summary: `The desktop description editor gains the same formatting toolbar as the web, and text and images finally lay out properly.`,
    body: `- **Formatting toolbar**: headings, bold, italic, strikethrough, code, links, quotes, bullet, numbered and task lists, clear formatting and insert image, all sitting above the description exactly as on the web. Press a button with nothing selected and the next thing you type comes out formatted.
- **Links stay links**: putting the cursor inside a link no longer turns it back into raw markdown; edit its address from the toolbar instead.
- **Readable descriptions**: text is sized to match the rest of the app, and wrapped paragraphs no longer overlap the ones below them.
- **Image controls on the image**: a "…" menu at the picture's corner and a drag handle on each of its edges, instead of controls stranded at the far side of the column. Dragging back to full width clears the custom size again.`,
  },
  {
    id: `2026-07-desktop-wysiwyg-editor`,
    date: `2026-07-24`,
    title: `A real editor for issue descriptions on desktop`,
    summary: `The desktop app now edits descriptions as formatted text. Headings, lists, tables and images render as you type, instead of raw markdown.`,
    body: `- **WYSIWYG descriptions**: the desktop editor shows your formatting live while you type, with headings, bold and italics, bullet, numbered and task lists, quotes, code blocks, links, tables, and inline images.
- **Images inline**: paste a screenshot straight into a description and it uploads in the background; drag its corner to resize, or right-click to open, download, copy or delete it.
- **@ and # as you type**: mentioning a teammate or linking another issue offers suggestions from the first character, and both render as pills you can click.
- **Your text stays your text**: descriptions written on web, iPhone or Android now survive a desktop open untouched, down to the byte.`,
  },
  {
    id: `2026-07-labels-and-ide-polish`,
    date: `2026-07-24`,
    title: `No more duplicate labels, dialogs that fit your phone`,
    summary: `Duplicate label names are blocked (existing ones merged), dialogs go full-screen on mobile, and the desktop IDE gets link copying and a full-width diff view.`,
    body: `- **One label per name**: creating a label whose name already exists in the team (regardless of casing) is now rejected with a clear message on every client, and pre-existing duplicates were merged automatically.
- **Dialogs on mobile**: every web dialog now opens as a full-screen page on small screens, and the Start-coding dialog got a two-column layout that finally fits without double scrollbars.
- **Desktop IDE polish**: copy a link to any issue from its header, start coding straight from the properties sidebar, and see inline images in descriptions just like on the web.
- **Full-width diffs**: the desktop Source Control view drops the per-file column so commit diffs get the whole pane.`,
  },
  {
    id: `2026-07-actions`,
    date: `2026-07-24`,
    title: `Actions: reusable AI commands for your team`,
    summary: `Define reusable AI prompts (code review, backlog grooming, changelog drafts) and run them on your desktop from any device.`,
    body: `- **Actions**: every team gets a library of reusable AI prompts. Create one from a template or let Claude write it from a one-line description, then run it with one click from web, desktop, iOS, or Android. It executes live on your own computer, and you can watch and steer it like any coding session.
- **Templates included**: a code review that files issues, backlog labeling and prioritization, and changelog drafting.
- **Safe by design**: actions run only on your own machine under your own accounts and sign-ins, and each device asks you to approve an action's exact instructions before the first run (and again whenever they change). The server never stores secrets.
- **A leaner desktop IDE**: boards now live as icons in the left rail, the top bar is gone, and source control went master-only with automatic pulls. Your local copy quietly tracks the default branch, changes arrive via pull requests, and one button discards local changes if things get tangled.
- **Run configs retired**: the old per-board terminal commands are replaced by actions.`,
  },
  {
    id: `2026-07-batch-select-and-review-ui`,
    date: `2026-07-23`,
    title: `Multi-select everywhere, a cleaner review flow`,
    summary: `Pick several issues at once on every client, a reworked review screen on mobile, and a tidier issue list.`,
    body: `- **Multi-select refinements**: selecting several issues at once got faster and more predictable on the web, the desktop IDE, iOS, and Android, and the bulk action bar now matches across all of them.
- **Reworked review screen on mobile**: merging and closing a pull request moved into a bottom action bar on iOS and Android, so the controls sit where your thumb is, and the file diff opens collapsed so big PRs load instantly.
- **Steering that reconnects**: the live agent terminal on iOS and Android now reconnects on its own after a dropped connection instead of stranding you on a dead screen.
- **Issue list alignment**: identifiers line up in My Issues and search results the way they always did on a board.`,
  },
  {
    id: `2026-07-webmcp-tools`,
    date: `2026-07-23`,
    title: `Browser AI agents can now work your boards`,
    summary: `The web app speaks WebMCP: in-browser AI agents can read your boards, file and update issues, comment, and navigate for you.`,
    body: `- **WebMCP support**: the web app now registers page tools via the emerging WebMCP browser standard, so AI agents running in your browser (Chrome's built-in agent, MCP browser extensions) can work with what's on screen.
- **Read and act as you**: agents can look up your boards, list and search issues, read full issue threads, check your inbox, create and update issues, comment, manage labels and subscriptions, and jump to any view, always as your signed-in user and only in teams you're a member of.
- **Nothing new is exposed**: tools reuse the exact same permissions and APIs as clicking the UI yourself.`,
  },
  {
    id: `2026-07-mobile-detail-live-support`,
    date: `2026-07-23`,
    title: `Redesigned issue view on mobile, live support chat`,
    summary: `A reworked issue screen on iOS and Android, start coding from your phone, and live support conversations.`,
    body: `- **Reworked issue detail on mobile**: iOS and Android get a cleaner issue screen with a bottom action bar, tidier property and label pickers, due-date and assignee sheets, and a collapsible activity timeline.
- **Start coding from iOS**: kick off a coding session on a connected desktop straight from the iOS app.
- **Live support chat**: when a reporter has their support thread open, replies appear live and we hold back the email notification while they're watching.
- **Resizable web terminal**: drag to resize the agent panel on the web, and it remembers your height.
- **Desktop IDE tabs**: closing, middle-click, and right-click on editor tabs now feel like a real IDE.
- **More reliable email**: hardened transactional email with automatic bounce and complaint handling.`,
  },
  {
    id: `2026-07-reliability-and-security`,
    date: `2026-07-22`,
    title: `Faster sync, tighter security, keyboard-driven search`,
    summary: `A reliability, performance, and security pass across every client.`,
    body: `- **Snappier real-time sync**: reworked how boards, issues, and notifications sync so large teams stay fast and connections recover cleanly under load.
- **Keyboard-driven search**: global issue search (⌘F on the web) now supports arrow keys to move and Enter to open a result.
- **Clearer billing errors**: checkout and billing-portal buttons now surface a message when a request fails instead of doing nothing.
- **Security hardening**: signing out now fully ends your session on the server, and we closed a case where an image link could carry your session token to another site.
- **Desktop IDE polish**: fixes to issue-title editing and description layout.`,
  },
  {
    id: `2026-07-feedback-widget-origin`,
    date: `2026-07-21`,
    title: `See what came from your feedback widget`,
    summary: `Issues filed through the embeddable feedback widget now carry a clear "Feedback widget" label.`,
    body: `- **Feedback widget label**: issues that arrive through your embeddable widget now show a "Feedback widget" origin pill on the issue, on web, iOS, Android, and the desktop IDE.
- **No more hidden bot users**: feedback used to be filed by a synthetic per-widget user that could show up in member lists; those are gone. Widget-filed issues simply have no sender, and everything (member lists, seat counts, account deletion) treats them cleanly.`,
  },
  {
    id: `2026-07-invites-from-the-web`,
    date: `2026-07-21`,
    title: `Invites now live on the web`,
    summary: `Team invites are created here on the web. Invite links still open and join right inside the mobile apps.`,
    body: `- **Invite from the web**: the iOS and Android apps no longer create invites. Invite teammates under Settings → Members, and the link you share still opens and joins directly in the app.
- **Clearer storage errors on mobile**: when a team is out of attachment storage, image uploads now say so and offer a retry instead of retrying forever.
- **Privacy policy refresh**: it now covers Sign in with Apple, the legal bases and your rights under the GDPR, international transfers, and the 48-hour board trash.`,
  },
  {
    id: `2026-07-agents-need-you`,
    date: `2026-07-21`,
    title: `See when an agent needs you`,
    summary: `Coding sessions waiting on your input now show an attention badge on every client.`,
    body: `- **Needs-input badge**: when a coding agent parks on a plan approval or a question, the session is flagged everywhere. The Agents tab and nav badges on web, iOS, Android, and the IDE light up until you answer.
- **Start-coding polish**: reworked agent and model pickers in the Start-coding dialog on web, the IDE, and the mobile sheets.
- **Smoother steering on iOS**: answering agent questions and plan approvals in the session view got a cleaner flow.
- **Marketing + pricing refresh**: updated agents section and plan pages at exponential.at.`,
  },
  {
    id: `2026-07-widget-domains-mobile-agents`,
    date: `2026-07-20`,
    title: `Widget domain allowlists required, and a nicer mobile Start-coding sheet`,
    summary: `Feedback widgets now always require a domain allowlist, and the mobile agent picker got a visual refresh.`,
    body: `- **Widget keys are locked to your domains**: the "allow any website" mode is gone. Every widget config needs at least one allowed domain, and keys without one stop serving until you add it in Settings → Feedback widget.
- **Mobile Start-coding refresh**: agent icons, per-agent options, and a cleaner repository picker on iOS and Android.
- **IDE fixes**: window sizing polish and coding-flow refinements.`,
  },
  {
    id: `2026-07-multi-agent-resume`,
    date: `2026-07-20`,
    title: `Pick your coding agent, and resume where you left off`,
    summary: `Start coding sessions with Claude, Codex, or pi. Resume an earlier session instead of starting over.`,
    body: `- **Three agents**: the Start-coding dialog now offers Claude, Codex, and pi, each with its own model and effort picks; your defaults are saved per agent. Remote starts from your phone only offer the agents actually installed on the chosen desktop.
- **Resume coding**: reopen a finished or interrupted session and the agent picks up with its previous context instead of starting from scratch.
- **Guarded by default**: Claude runs now default to the guarded auto permission mode instead of skipping permissions; the IDE doctor needs claude ≥ 2.1.215 for it (run \`claude update\` if coding shows as blocked).
- **IDE polish**: long issue descriptions no longer get cut off in the IDE editor.
- **Smoother marketing film**: the exponential.at intro video now autoplays reliably.`,
  },
  {
    id: `2026-07-mobile-steering-review`,
    date: `2026-07-20`,
    title: `Answer your agent from anywhere, and sessions that wait for review`,
    summary: `Tap to answer agent questions from your phone, and coding sessions now show a "ready for review" state once the PR is open.`,
    body: `- **Steer from your phone**: when an agent asks a question, iOS, Android, and web now show it as a tappable card. Pick an option (multi-select included) instead of typing keystrokes.
- **Ready for review**: when the PR opens, a coding session moves to an in-review state across all clients instead of disappearing, so you can see what's waiting on you.
- **Better tabs on mobile**: My Work and Support switched to a cleaner segmented control on iOS and Android, and agent sessions open full-screen on the mobile web app.
- **Editing on phones**: the issue description editor keeps your cursor visible above the keyboard while you type.
- **Fresh marketing pages**: new agents, teamwork, and helpdesk sections plus clearer pricing at exponential.at.`,
  },
  {
    id: `2026-07-create-or-join-mobile-web`,
    date: `2026-07-19`,
    title: `Create or join a team, a merged inbox, and a mobile-friendly web app`,
    summary: `Pick your own first team, My Issues lives in the Inbox now, and the web app works properly on phones.`,
    body: `- **Create or join**: new accounts no longer get an auto-created personal team. On first launch you create a team (you own it) or join one by pasting an invite link; invites can now also be emailed directly.
- **One inbox**: My Issues merged into the Inbox as a tab on web and in the IDE, so notifications and your assigned work live on one page. Support gets an unread badge on every client.
- **Web on your phone**: a bottom tab bar, mobile-sized layouts, and detail pages that use the full screen.
- **Fixes**: desktop steering activity works again, Android keeps the helpdesk entry after partial syncs and refreshes repositories after board creation, and the IDE got a round of polish.`,
  },
  {
    id: `2026-07-teams-boards-helpdesk`,
    date: `2026-07-19`,
    title: `Boards, a simpler helpdesk, and a private-by-default product`,
    summary: `Projects are now boards, the helpdesk moved to one team-level inbox, and public boards are gone.`,
    body: `- **Projects are now boards**: same power, clearer name, on web, mobile and the IDE.
- **Public boards are gone**: nothing in a team is readable from outside anymore. The feedback widget is the one way outsiders reach you, and it's now included on every plan (1 widget on Free, 3 on Pro).
- **One helpdesk per team**: flip a single switch under Settings → Feedback widget (Pro+) and every member shares the Support inbox. Tickets are standalone conversations with an email reply loop, and any ticket can be escalated into an issue on a board with one click.
- **Simpler board creation**: no more board types. A board is a board; connect a repository when you want to code on it.`,
  },
  {
    id: `2026-07-whats-new-card`,
    date: `2026-07-17`,
    title: `A changelog, mobile coding, and support inboxes`,
    summary: `Start coding from your phone, helpdesk widget mode, and this changelog.`,
    body: `- **What's new lives here now**: each release drops a note in this changelog. Dismiss the card and it stays quiet until the next release; reopen it anytime from the user menu.
- **Start coding from mobile**: the iOS and Android apps can now remotely start coding sessions on your desktop, including batch runs.
- **Support inboxes**: the feedback widget gained a helpdesk mode, so support tickets can file into a separate private inbox, away from your feedback board.
- **Steering polish**: stale steering sessions are cleaned up reliably, and the web agent dock and review views got a refresh.`,
  },
]

export function latestChangelogEntry(): ChangelogEntry | null {
  return CHANGELOG[0] ?? null
}
