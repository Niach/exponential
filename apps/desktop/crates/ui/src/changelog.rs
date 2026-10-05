//! The in-app changelog (EXP-723) — the desktop half of EXP-164's "What's
//! new".
//!
//! The web app keeps the whole history in `apps/web/src/lib/changelog.ts` and
//! surfaces the HEAD entry as a dismissable card in its sidebar footer. The
//! desktop shows the same card in the rail footer, so it needs the same head
//! entry — mirrored here as a const rather than fetched, because the app must
//! be able to say what is new in the build the user is actually running (an
//! older desktop pinned to an older release must not advertise a newer web
//! entry).
//!
//! **The mirror is a contract**: [`LATEST`] is a verbatim copy of
//! `CHANGELOG[0]` — id, date, title, summary and body — and
//! `apps/web/src/lib/changelog-desktop-mirror.test.ts` reads this file and
//! fails the web suite when the ids or titles drift. Prepending a web entry
//! means updating this const in the same commit.
//!
//! Dismissal state is the per-install `settings.json` key `changelogSeenId`
//! (the desktop analog of the web's per-device `changelog-seen.ts`): the card
//! renders while the stored id differs from [`LATEST`]'s, and both the ✕ and
//! opening the dialog store it.

use gpui::{
    div, px, size, App, AppContext as _, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Window,
};
use gpui_component::{text::TextView, v_flex, ActiveTheme as _};

use crate::coding_flow;
use crate::native_dialog::{self, DialogContent, DialogSpec};

/// One changelog entry — the `ChangelogEntry` interface of
/// `apps/web/src/lib/changelog.ts`, field for field.
pub(crate) struct ChangelogEntry {
    /// Stable slug and dismissal key. Never reused.
    pub id: &'static str,
    /// ISO date, display only.
    pub date: &'static str,
    pub title: &'static str,
    /// One-line card preview (rendered truncated).
    pub summary: &'static str,
    /// GFM markdown, rendered read-only in the dialog.
    pub body: &'static str,
}

/// The head entry of the web `CHANGELOG` — see the module docs: this is a
/// verbatim mirror, gated by a web-side test.
pub(crate) const LATEST: ChangelogEntry = ChangelogEntry {
    id: "2026-10-05-release-train",
    date: "2026-10-05",
    title: "Agent first, and run pull requests you can read in the app",
    summary: "Phones open on the Agent page, tabs swipe sideways, a chat or action run's pull request opens its changes in the app, Actions and Drafts are top-level entries again, and Exponential's MCP server draws issue, run and inbox views inside MCP clients.",
    body: r#"- **Phones open on the Agent page**: web, iOS and Android land on the Agent page with the tab bar kept, and the chat button is lit while you are there.
- **Swipe between tabs**: Inbox, My Issues and Drafts page sideways like the issue tabs, and so do Actions and Suggestions and the action page on the phone web.
- **Run pull requests**: a pull request from a chat or action run opens the run's Changes tab on web, desktop, iOS and Android instead of GitHub. Teammates get a read-only view of the diff.
- **Navigation**: the More menu is gone on web and desktop. Actions and Drafts (while you have any) are top-level entries, actions wear a rocket, and the desktop footer has a Computer menu with Terminal, Files and Source Control.
- **Desktop sidebar**: the sidebar stays expanded everywhere and the app opens on Agent. Inbox, a Reviews detail and the Agent page's recent runs open a second list inside the content card; the Reviews list there includes pull requests from agent runs, and the file tree only shows when the diff keeps enough room.
- **Issue and run pages**: the properties stay in view under the header on every tab, the transcript fades into the composer, an arrow jumps to the newest message, the task list shows its progress, a plain × discards a draft, and Merge is a circle in the phone's bottom bar.
- **MCP Apps**: in MCP clients that support MCP Apps, Exponential's server draws the issue list with issue detail, runs, the inbox and devices as interactive views. New tools: `exponential_issues_show` and `exponential_devices_account_login`. An OpenClaw plugin ships in the repository.
- **Safer by default**: signing a machine's agent account in through MCP needs a full-access key, and a run's pull request diff is readable only when its repository belongs to the run's team."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_32: ChangelogEntry = ChangelogEntry {
    id: "2026-10-04-release-train",
    date: "2026-10-04",
    title: "Fixes on top of the one-path wave",
    summary: "Reporter replies reach the team, Reply to reporter switches off after each send, the GitHub connection stays signed in, phone lists span every team, and the issue title collapses into the header as you scroll.",
    body: r#"- **Reporter replies**: a widget reporter's answer reaches every team member when nobody is assigned to or following the issue, and the Reply to reporter toggle switches off after each send on web and desktop, so a follow-up note stays inside the team. A comment an agent sent to the reporter reads "to reporter · via MCP", and comments from people who left the team read "Former member" on every client.
- **GitHub connection**: refreshing the connection no longer signs you out when two screens refresh at once, the GitHub user token stays on the server (API keys cannot read it), Continue with GitHub only works when the instance enables it, and the connect page never leaves the site after the flow.
- **Pull requests**: an issue-less run's pull request keeps its base branch across a close and reopen, old links to the separate review page open the issue's Changes face, and opening an issue with an open pull request no longer asks GitHub for the diff on every visit.
- **Phones**: Reviews, runs, actions and devices list every team you belong to, grouped by team; the Actions tab replaces the More menu; the issue title collapses into the header as you scroll and content slides under a blurred edge on iOS, Android, web and desktop.
- **Runs**: a run shows a screenshot while it works, filed under its Results; queued merges report as queued; the Run face collapses to a status row when there is nothing live.
- **Joining a team**: the invite flow offers the device setup step when you own no device yet, on web, iOS and Android.
- **Emails**: report confirmation and resolution emails show the title as plain text, and the resolution subject no longer repeats it.
- **Android**: reporter comments an earlier version failed to show appear after updating.
- **Older apps**: apps from before this release keep working. App Store iOS and current Android builds do not show reporter comments until they update; older desktops' email settings save again."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS: ChangelogEntry = ChangelogEntry {
    id: "2026-10-04-review-on-the-issue",
    date: "2026-10-04",
    title: "Review the pull request on the issue",
    summary: "A pull request's diff is the issue's Changes face now, and Results reads as a numbered guide whose text is also the pull request's description on GitHub.",
    body: r#"- **One page**: a Reviews row, the issue's pull request row and the Related work rows open the issue on its Changes face, with the file tree beside it, Merge PR in the properties and GitHub in the header. The separate review screen is gone on web, desktop, iOS and Android.
- **Close PR**: closing a pull request without merging moved into the issue's actions menu.
- **Results as a guide**: the summary leads, then one numbered section per change with the files it touched and its screenshots. A file opens the Changes face on that file. An open pull request without a report shows its GitHub description.
- **Pull request description**: the agent's report is the pull request's description, so the two never disagree; screenshots stay in Exponential.
- **Phones**: Merge PR is the white button on the bottom bar again, on every face."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_1: ChangelogEntry = ChangelogEntry {
    id: "2026-10-04-run-status",
    date: "2026-10-04",
    title: "Run status at a glance",
    summary: "Every live run says what it is doing wherever it is named: Claude's spark spins while it works, amber waits on you, green has an open pull request, blue is done.",
    body: r#"- **Working**: while the agent works, its mark is Claude's own spark, spinning in the sidebar's Running rows, the run lists, the tabs and the Run face. Other agents pulse their mark.
- **Waiting on you**: an amber badge, also when the run already has a pull request open and asks again.
- **In review**: a green badge once the agent finished and its pull request is open.
- **Done**: a blue badge once the agent finished with no open pull request, or after the merge.
- **Follow-ups**: a follow-up on a run in review shows it working again and returns to green when the turn ends; the issue's status does not change."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_2: ChangelogEntry = ChangelogEntry {
    id: "2026-10-04-one-github-flow",
    date: "2026-10-04",
    title: "One way to connect GitHub",
    summary: "Connect GitHub, install the app, pick a repository: one guided page that every Start coding checklist, board form and settings section opens, listing the repositories you can push to.",
    body: r#"- **One flow**: Connect GitHub links your GitHub account to your Exponential account (any login works), installs the Exponential app on the accounts you choose, and lists the repositories you can push to. The team claim step, the install round-trip and the stale-access banners are gone.
- **Start coding**: the checklist's Connect GitHub fix opens that page over the issue; coming back ticks the row off.
- **Settings → Repositories**: your GitHub connection with its accounts and a Configure link per account, Install on another account, and Add repository from the live list or by name.
- **Self-hosting**: the GitHub App now needs its OAuth client id and secret, and its callback URL is the login callback; INSTALL.md walks through it."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_3: ChangelogEntry = ChangelogEntry {
    id: "2026-10-03-one-path-for-feedback",
    date: "2026-10-03",
    title: "One path for feedback",
    summary: "Every widget submission is an issue, you reply to the reporter from the comment composer, and their answers come back as comments and Inbox rows. The helpdesk and its Support inbox are gone.",
    body: r#"- **Widget submissions are issues**: whatever a visitor sends through the widget lands as an issue on the widget's board, with their screenshot, page context and contact attached. The widget has no modes any more.
- **Reply to the reporter**: the comment composer on such an issue carries a Reply to reporter toggle. Switch it on and your comment is emailed to the reporter, marked "to reporter" in the thread.
- **Reporter answers**: the reporter answers from the emailed link. Their reply shows up as a comment on the issue, marked "reporter", and reaches you as an Inbox row and a notification. A reply on a done issue reopens it.
- **Helpdesk removed**: the helpdesk, its Support inbox and the Support entries in the sidebar are gone. Old conversations were moved to issues on a Support board so nothing is lost."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_4: ChangelogEntry = ChangelogEntry {
    id: "2026-10-03-new-issue-page",
    date: "2026-10-03",
    title: "New issue is a page",
    summary: "New issue opens as the issue detail in draft mode on web, desktop, iOS and Android, saving a draft as you type until you press Create.",
    body: r#"- **New issue page**: the create dialog is gone. New issue opens a blank issue page with the same title, properties, description and files you know from the detail, on every platform.
- **Drafts as you type**: what you write is kept as a draft a moment after you stop typing, when you pick a property and when you leave the page. Come back from Drafts and continue where you stopped.
- **Create**: the Create button in the header files the issue and lands you on it. Discard draft in the menu throws it away.
- **Files on drafts**: paste or attach files before the issue exists; they move onto the issue when you create it."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_5: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-one-run",
    date: "2026-10-01",
    title: "One kind of run",
    summary: "Workflows are gone: a run plans and fans out its own work. Batches, stacked pull requests and the Related work badge keep working.",
    body: r#"- **Runs**: workflows are removed. A run splits its own work with subagents and starts further runs for the follow-ups it files, each on its own pull request.
- **Batches**: starting several issues as one batch run with one combined pull request works as before.
- **Dependent work**: a pull request can still be based on another one. Merge the first one first: its merge moves the ones built on it to the default branch.
- **Reviews**: one flat list of open pull requests. The Workflows group is gone.
- **Blocked issues**: starting a blocked issue asks Cancel, Start anyway or Stacked PR. A stacked start builds the whole line bottom-up: the lowest issue without a pull request starts first, and each run starts the next one on top of its own.
- **Header**: the Related work badge stays (blockers, batch, stack). A batch run's title also lists its issues.
- **Merging**: Merge on a stacked pull request asks Merge stack or Merge this pull request, and lands the stack bottom-up.
- **MCP**: the `exponential_workflows_*` tools and `stackOnIssueId` are gone. `exponential_pr_open` takes `base`, and `exponential_pr_merge` merges `issueIds` in the given order or a stack with `mergeStack`.
- **Older apps**: older apps keep working and show an empty Workflows page."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_6: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-release-train",
    date: "2026-10-01",
    title: "Triggers, accounts and connections, tightened",
    summary: "Fixes on top of today's changes: trigger switches that no longer undo each other, older apps that keep editing their automations, and steadier account picks, retries and desktop connections.",
    body: r#"- **Triggers**: two quick changes to an action's triggers no longer undo each other on web, desktop, iOS and Android, and an action that came out of the move with more than ten triggers stays editable.
- **Older apps**: an app from before the triggers release still pauses, edits, creates and deletes automations, and its Set as default on an account answers instead of failing.
- **Accounts**: a run started without an account skips a last used login that is signed out. A desktop app or CLI that had a default account keeps it as its last used login when it updates.
- **API errors**: the automatic retry stops after three failed attempts, even when each attempt got a few words out first.
- **Desktop connection**: slow requests no longer make the app rebuild its connection, apps do not all reconnect in the same second, and after wake the new connection is in place before sync restarts.
- **Phone tabs**: on Android the keyboard closes when you change the tab, and on iOS Stop and Resume stay correct after swiping between Run and Changes, the issue page runs to the screen edge again and Properties opens on the first tap.
- **Desktop**: the list beside an open issue or run is wider and its edge can be dragged, the run dialog of an action fits its content, and the compaction bar is smaller.
- **Stacks and batches**: one quiet badge in the header opens Related work with what blocks the issue, what shares its pull request and its stack. Closed rows are dimmed on desktop too."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_7: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-action-triggers",
    date: "2026-10-01",
    title: "Actions have triggers",
    summary: "Automations are now triggers on the action they run: one Actions list, and each action is a page with its prompt, its triggers and its runs.",
    body: r#"- **Triggers**: a schedule or an event that starts an action now lives on that action. Your automations became triggers on their action and keep firing on the same device, with the same account, model and effort.
- **Action page**: opening an action shows its prompt, its triggers and its runs, as sections of one page on desktop and as tabs on a phone. The Automations page and tab are gone.
- **Actions list**: a clock marks an action with a schedule, a bolt one that an event starts. The mark is dimmed while that trigger is paused.
- **Runs**: an action lists every run of it, the triggered ones marked. It replaces Recent automated runs.
- **Tidy up**: a team that had scheduled Tidy up now owns it as an action with the same prompt, free to edit.
- **MCP**: `exponential_actions_update` takes `triggers`. The `exponential_automations_*` tools are gone.
- **Older apps**: apps from before this release keep firing their triggers and still pause, edit and delete them as automations."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_8: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-last-used-account",
    date: "2026-10-01",
    title: "Last used account",
    summary: "There is no default account any more: every start picks the account you last used on that device.",
    body: r#"- **Last used**: the composer on every client starts on the account you last started or switched a run on, per device, and a run that names no account (an automation without a pinned account, a CLI or MCP start) runs on it too.
- **Removed**: the Default account row in Device settings and Settings > Agents, and Set as default in a device's account menu.
- **Unattended runs**: automations, workflows, agent-started runs and automatic account rotation never change it."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_9: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-runs-recover",
    date: "2026-10-01",
    title: "Runs recover from API errors and dead connections",
    summary: "A run whose turn died on an API error retries by itself, a child run that switches accounts is never resumed twice, and the desktop app drops a dead server connection instead of showing cached data for minutes.",
    body: r#"- **API errors**: when a turn stops on an API outage, a timeout or a dropped connection, the run sends its own "continue" after 30 seconds, then 2 and 10 minutes, and says in the transcript that the retry was automatic. Three failures in a row stop the retries, and Stop cancels a pending one.
- **Accounts**: a new run counts the runs already live on each account, so two runs started close together go to different accounts instead of sharing one usage window. A run you start on a named account stays on it. The rate-limit banner says the limit is per account on web, desktop, iOS and Android.
- **Child runs**: a child run that its machine moves to another account tells its parent that it resumes itself and under which new id, instead of "ended without a report". Resuming a run that is already live again under another id is refused and names that run.
- **Desktop connection**: the desktop app and the CLI detect a dead server connection within seconds, rebuild their connection after repeated failures, on wake and on Retry, and log the cause of a failed request."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_10: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-phone-face-pager",
    date: "2026-09-30",
    title: "Phone tabs follow your finger",
    summary: "Issue, Run, Changes and Results on the phone drag like native tabs on web, iOS and Android, and the Changes tab shows the diff counts.",
    body: r#"- **Dragging**: the Issue, Run, Changes and Results faces of the phone's Work screen move with your finger and settle on the neighbour, as a native pager on iOS and Android and as a slide on web. Tapping a tab animates the same way. Before, the face switched only after the swipe ended.
- **Changes counts**: the Changes tab reads the diff's added and removed line counts, as the desktop app does, once the run's diff or the pull request's files are known.
- **Styleguide**: the face strip has its own entry, Work face tabs, so every client draws the same control."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_11: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-api-errors",
    date: "2026-09-30",
    title: "API errors are not rate limits",
    summary: "An API outage or dropped connection inside a run shows as a warning line in the transcript instead of a rate limit and never switches the account; the banners under the transcript sit in the reading column.",
    body: r#"- **API errors**: when the agent's API times out, drops the connection or rejects a request, the run shows the error as a warning line where it happened. It no longer reads as a rate limit, no longer marks the run blocked and no longer resumes it on another account.
- **Banners**: the rate-limit, paused, connection-lost and compaction banners under the transcript sit in the same centered column as the transcript rows on web, desktop, iPad and Android tablets."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_12: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-release-train",
    date: "2026-09-30",
    title: "Release train 2026-09-30",
    summary: "Runs report on Results and notifications open them, tall screenshots and markdown attachments preview in-app, one stacking toast everywhere, yolo mode, a Tidy up action, editable pull request descriptions, and honest session tools for agents.",
    body: r#"- **Results**: a run's close-out report sits beside its screenshots on the issue's Results, visible to every member, and a notification an agent sends can open it directly. Runs publish screenshots of their visible changes by default. A tall screenshot shows a framed tile and opens a scrolling viewer; Android pinches to zoom.
- **Markdown attachments**: .md files open in an in-app preview on the desktop app, iPhone and Android, with Download beside it.
- **Toasts**: one stacking toast on every platform, bottom-right on a pointer and top-centre on touch, tap to expand, swipe to dismiss. Alerts that only informed became toasts; destructive actions still confirm.
- **Yolo mode**: an owner switch under Settings › General. A pull request an agent opens merges at once, a follow-up tree merges root first once every run in it is done, and Reviews leaves the navigation unless a merge failed.
- **Tidy up**: a built-in action for non-destructive board cleanup (duplicates, labels, relations), the one builtin an automation can run, with matching icons for stacks, batches and workflows.
- **Pull requests**: agents update a pull request's title or description with exponential_pr_update, and Reviews shows the description with Edit while it is open. A merge that GitHub answered with a server error after landing reads as merged, and an issue moved across boards mid-run still resolves.
- **Session tools**: a failed remote start names its reason, a message reports whether the agent consumed it, queued messages reach the agent together, and a run you stop keeps its uncommitted work as a WIP commit on its branch.
- **Fixes**: a signed-out default agent login no longer blocks a signed-in named profile, MCP credentials reach only the run that picked them, an invite to a still referenced placeholder binds and merges on accept, a sub-issue composed with an attachment leaves no draft behind, the desktop app's Files and Source Control no longer flash "No repository linked" on cold start, stacked issue chips draw as outlines, and the web onboarding wizard shows who is signed in with a Sign out."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_13: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-phone-work-tabs",
    date: "2026-09-30",
    title: "Issue, Run, Changes and Results are tabs on the phone",
    summary: "The phone's Work screen shows its faces as swipeable tabs in the header, with Merge beside them while the pull request is open, on web, iOS and Android.",
    body: r#"- **Tabs**: Issue, Run, Changes and Results sit in one segmented strip inside the header, under the title, the same control as the Inbox strip, listing only the faces the issue has. Tap one or swipe the body left or right to move between them. The bottom-right switcher circle is gone.
- **Runs**: with several runs on an issue the strip reads Runs, and the run menu opens from it.
- **Merge beside the tabs**: while the pull request is open, the Merge button sits next to the tabs on every face, so a run can be merged without leaving it. It left the properties card and the Changes bar.
- **Bars**: the Issue face keeps Properties, Comment and Start coding; Changes keeps the file list; the Run face offers Start coding once a run has ended for good; Results has no bar."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_14: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-sign-in-methods",
    date: "2026-09-30",
    title: "Sign-in methods and a changeable email",
    summary: "Settings › Account lists every way into your account: change your email with a code, link or unlink Google, Apple and OIDC logins, and manage passkeys, on web, desktop, iOS and Android.",
    body: r#"- **One primary email**: it starts as the address you registered with (the Google or Apple address for those sign-ups) and Change under Settings › Account swaps it for any other after a code sent to the new address. Sign-in codes, notifications and new mentions use it from then on; earlier @mentions keep the address they were written with.
- **Sign-in methods**: Google, Apple and every OIDC provider your instance offers appear with Link or Unlink, a password row shows while one is set, and passkeys sit right below (moved back from Security, which keeps the API keys). Every method is optional, but the last way in can never be removed.
- **Natives**: the same list on the desktop app, iPhone and Android. Linking runs through the browser as sign-in does and returns to the app; passkeys are still added from the web.
- **Admins**: an email change never grants admin. INITIAL_ADMIN_EMAILS keeps promoting only the address an account registered and verified with."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_15: ChangelogEntry = ChangelogEntry {
    id: "2026-09-29-stack-merge-asks",
    date: "2026-09-29",
    title: "Merging a stacked pull request asks first",
    summary: "Merge on a pull request that belongs to a stack asks whether to merge the whole stack or this pull request only, on every client.",
    body: r#"- **Stacked pull requests**: the Merge button on the Changes face, the issue header and the run view asks first when the pull request is part of a stack with other open members. Merge stack lands every open pull request bottom-up; Merge this pull request lands it and the open ones below it, while the ones above stay open on the base branch. A pull request whose stack has nothing else open merges as before.
- **Agents**: the result of exponential_pr_merge on a stack member names the pull requests it landed below and the ones it left open, and never lists an already merged one."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_16: ChangelogEntry = ChangelogEntry {
    id: "2026-09-29-account-sign-out",
    date: "2026-09-29",
    title: "Sign agent accounts out",
    summary: "Every agent login under a device has a Sign out entry in its menu, and the machine's own login can be removed too.",
    body: r#"- **Devices**: every agent login listed under a machine offers Sign out in its "..." menu, on the web, in the desktop app and on iOS and Android. The machine's own login (the one the CLI in your terminal uses) can now be removed as well: it is signed out there and hidden until it signs in again. Removing never touches the account itself. Update the desktop app or CLI on the machine to pick this up."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_17: ChangelogEntry = ChangelogEntry {
    id: "2026-09-29-release-train",
    date: "2026-09-29",
    title: "Release train 2026-09-29",
    summary: "Start coding is always on an issue with a \"Ready to code?\" checklist that fixes what is missing, sub-issues and relations get their own bands, sign-up asks for your name, the CLI adds devices and sessions commands, and a new machine installs with one command.",
    body: r#"- **Start coding**: every issue shows Start coding on every client. When something is missing (GitHub, the board's repository, an online machine) it opens a "Ready to code?" checklist with one fix per step, including picking the repository right there; the bulk bar, the Agent composer and Getting started use the same checklist.
- **Issues**: a sub-issue names its parent above the title, sub-issues sit in their own band with a progress ring and a +, and every other relation folds into its own band. The stack, batch and runs badge shows on every face of a piece of work.
- **Sign-up**: signing up with an email code asks for your name instead of guessing it from the address.
- **Devices**: Add device shows an install command carrying a one-time token, so a new machine signs in, installs the daemon and turns on auto-update with one command. The CLI adds `exponential devices` and `exponential sessions` (list, show, log, message, kill) and starts runs on another machine with `--device`.
- **Desktop app and CLI**: fetches and pushes that stall stop and clean up after themselves instead of hanging, account rotation probes each agent once per beat and keeps an account you picked, and workflow node tooltips always say what the node is.
- **Self-hosting**: this release removes server support for iOS below 0.14.45, Android below 0.14.46 and desktop or CLI below 0.14.54. Set `CLIENT_MIN_VERSION_*` to those versions before upgrading (see `selfhost/.env.example`)."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_18: ChangelogEntry = ChangelogEntry {
    id: "2026-09-28-release-train",
    date: "2026-09-28",
    title: "Release train 2026-09-28",
    summary: "Plan-mode claude runs use their tools again, a run on an empty repository starts instead of hanging, the Recent runs panel has a back row, and phones wait for their issues instead of flashing an empty board.",
    body: r#"- **Agent runs**: a claude run in plan mode no longer falls into the auto-mode classifier that refused every tool call, because the desktop app and the CLI pin auto mode off for every run they launch (update the machine to pick it up). A run on an empty repository creates its first commit and starts instead of hanging.
- **Agent page**: the Recent runs panel on the web and in the desktop app wears the back row every side panel does, and the history button hides while the panel is up.
- **Phones**: after a fresh sign-in or a full resync a board shows a spinner until its issues arrive, instead of flashing "No issues yet" and the getting-started checklist, and the iPhone waits for its boards the same way."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_19: ChangelogEntry = ChangelogEntry {
    id: "2026-09-25-work-header-badge",
    date: "2026-09-25",
    title: "The work header badge",
    summary: "The stack, batch and runs badge in the work header stands as tall as the controls beside it, the runs badge wears the workflow glyph, and the web shows it too.",
    body: r#"- **One height**: the badge that says what a piece of work is part of (a stacked pull request, a batch, the runs around this one) now stands at the face toggle's height beside Stop, Resume and Merge, on the web and in the desktop app, instead of a small chip next to tall controls.
- **Runs badge**: on the Run face of a run that started or was started by other runs, the badge wears the workflow glyph instead of the robot, and the web shows it where only the desktop app did.
- **Styleguide**: the badge and its popover have their own entry, with the glyph per relation and the rung it stands at."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_20: ChangelogEntry = ChangelogEntry {
    id: "2026-09-25-issue-context-menu",
    date: "2026-09-25",
    title: "One issue context menu",
    summary: "The issue context menu opens from every list, the sidebar and issue chips, no longer closes on its own, takes the estimate, and every web menu is tighter.",
    body: r#"- **Everywhere**: a right-click (or a long-press on a phone) opens the issue menu on a board row, the sidebar list beside an issue, a reviews row and any issue chip, with the same items in the same order.
- **Stays open**: the menu no longer fires the item under the cursor when the right button is released a few pixels into it, which is what made it open and close in one go.
- **Estimate**: the menu sets the estimate on the team's scale, beside status, assignee, priority, labels and due date.
- **Menu density**: menus on the web at desktop widths use tighter rows, matching the desktop app's rhythm at the web's text size; phones keep their touch-sized rows. The values are shared design tokens, shown in the styleguide with the issue context menu as its own entry."#,
};

/// The head before that, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_21: ChangelogEntry = ChangelogEntry {
    id: "2026-09-24-release-train",
    date: "2026-09-24",
    title: "Release train 2026-09-24",
    summary: "The usage popover shows what fills a run's context, owners add a team prompt to every run, machines update their agent CLIs remotely, and fixes for invited members and the Linear import.",
    body: r#"- **Context window**: the usage popover shows a segmented bar of what fills a run's context, with a legend per segment (base, tools, playbook, team prompt, project instructions, task) on every client, and the account switch says why a switch is refused right where the control is.
- **Team prompt**: owners write a prompt in Settings, General that every run on every member's machine carries in its system prompt, on the web and in the desktop app.
- **Machines**: the machine settings update the Claude and Codex CLIs remotely and show each login's version, on the web and in the desktop app.
- **Sidebar**: one look for the sidebar sections on the web and the desktop app, with the What's new card floating above the footer.
- **Workflows**: the node strip reuses the work face toggle, and the MCP workflows update takes the runner machine so an agent can bind it.
- **Invited members**: an invite can no longer seat another team's invited member or move their address, an existing member who opens a colleague's invite link is not merged into it, invited members count once against the seats, signing in with Google, Apple or an identity provider claims the invite, and the invite page welcomes the person instead of calling the link used.
- **Linear import**: cancelling holds against a finishing discovery, attachments stream with a size cap and a timeout, bundle attachments are checked against private addresses, duplicates keep their target, and finished imports drop their snapshot after a week."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_22: ChangelogEntry = ChangelogEntry {
    id: "2026-09-23-linear-import",
    date: "2026-09-23",
    title: "Import from Linear",
    summary: "Bring a Linear workspace into a team from Settings, with its issues, comments, images, labels, statuses, relations and estimates, keeping identifiers and dates.",
    body: r#"- **Import from Linear**: a new owner-only Settings section connects to Linear with a personal API key, previews what it found, lets you map teams to boards, statuses onto yours, labels and members, dry-runs the plan and then imports. Issues keep their identifiers, timestamps, priorities, assignees and comments; inline images are rehosted; activity history is optional.
- **Relations come along**: sub-issues, blocking, duplicate and related links are recreated between imported issues.
- **Members before they sign in**: an email invite puts the person on the team at once, with their name, so issues and comments can be theirs before they ever sign in; when they do, everything carries over. The Linear import invites a workspace's members straight from its member mapping, and the Members settings show who has not joined yet, with a way to resend the invite.
- **Archived issues**: an option puts a source's archived issues on a separate board per team, archived once the import is through, or leaves them out.
- **Estimates**: a team setting picks an estimate scale (exponential, fibonacci, linear or t-shirt sizes), off by default. With one on, issues take an estimate from their properties row on every client, over the API and the agent tools; a Linear import brings its estimates and switches the matching scale on.
- **Settings**: a new Issues section holds the estimate scale and the pull request automation, with Labels and Statuses as its sub-pages, on the web and in the desktop app alike.
- **Resumable and repeatable**: an import that stops resumes where it left off, and running it again adds nothing twice.
- **Bundles**: the same importer accepts a provider-neutral bundle over the API, so other trackers can follow without a new code path."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_23: ChangelogEntry = ChangelogEntry {
    id: "2026-09-23-release-train",
    date: "2026-09-23",
    title: "Release train 2026-09-23",
    summary: "Automations pick an account, the issue list shows its blocking arrows as a rail, and a round of fixes for the desktop app and the CLI.",
    body: r#"- **Automations**: the editor picks an account on the bound machine, with its brand mark and email, instead of an agent alone, on every client. A machine switch re-seeds the pick from the new machine.
- **Blocking arrows**: on wide screens the issue list draws its blocks relations as a rail at the right edge, with a dot per node at rest, the arrows on hover or keyboard focus, and the mini graph on click. Phones keep the badge.
- **Tree lines**: nested issues draw a tighter corner and one unbroken vertical where a later sibling follows, on every client.
- **Desktop and CLI**: a workflow node keeps its identity across an account switch or a resume, the remote sign-in dialog takes the code from the browser again, and the Agent entry no longer duplicates the Running section's dot.
- **MCP**: merging a plain pull request based on your default branch no longer reports it as a stacked merge."#,
};

#[allow(dead_code)]
const PREVIOUS_24: ChangelogEntry = ChangelogEntry {
    id: "2026-09-22-release-train",
    date: "2026-09-22",
    title: "Release train 2026-09-22",
    summary: "One account picker with live limits, a calmer issue activity, MCP servers per machine, and workflows that are always reviewed by an agent.",
    body: r#"- **Accounts**: the composer has one account picker across Claude and Codex with each login's limits inline, a default account per machine, and every login is named by its email, signed out ones included.
- **Activity**: an issue's activity folds quick back and forth edits into their net change, and Show all brings every event back.
- **Runs**: tool calls to Exponential show the issues, runs and pull requests they touched as chips with a preview, resumed runs sit behind one run menu, the GitHub button lives on Changes, and phones show the changed line counts.
- **Search**: open work ranks above done work, the same way on every client and over MCP.
- **MCP servers per machine**: a machine can carry its own MCP servers, picked up from your Claude and Codex config in the desktop app or the CLI, and they ride every run there. Entries that carry a secret are never imported.
- **MCP tools**: agents can list an issue's attachments, upload large files through a signed link, and compact their own session.
- **Workflows**: every node is reviewed by an agent before it lands, the review gate setting is gone, and a pull request merged outside the workflow lands where its code actually is.
- **Fixes**: the link field in the create issue dialog takes focus again, uploaded files no longer show zero bytes, markdown attachments preview in the app, and the comment edit button has its icon back."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_25: ChangelogEntry = ChangelogEntry {
    id: "2026-09-19-release-train",
    date: "2026-09-19",
    title: "Release train 2026-09-19",
    summary: "Workflows ship on every client, the phone tab bar keeps its Agents and New issue buttons, and a round of fixes from the release review.",
    body: r#"- **Workflows**: plan a set of backlog issues as one dependency graph, let a device run them in parallel with a merge train and a review gate, and follow it from the web, the desktop app, iOS and Android.
- **Phones**: the tab bar shows the Agents and New issue buttons on every tab, the Agent page keeps past runs behind a history button, and the board switcher lists teams and boards only.
- **Review fixes**: a workflow dependent can no longer be merged into your default branch behind the final pull request, a node's approval is bound to the reviewer run and the commit it reviewed, the metrics beat is recorded again, a skipped node's issue no longer moves to done with the final pull request, and a stacked start refuses a blocking cycle that runs through a closed issue.
- **Cleanup**: batch runs are named from their stored issue list on every client (older runs were backfilled), the staleness sweep only deletes runs whose machine is gone, and retired external-agent run records are dropped when the desktop app or CLI loads them."#,
};


/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_26: ChangelogEntry = ChangelogEntry {
    id: "2026-09-18-release-train",
    date: "2026-09-18",
    title: "Release train 2026-09-18",
    summary: "Device icons, the run's task list, one picker everywhere, and a round of fixes from the release review.",
    body: r#"- **Device icons**: every machine can carry its own icon, and the board and action picker grows to 96 icons.
- **The run's task list**: the strip under a run leads with the agent's own task list, a background lane keeps its tab while it runs, and calls to Exponential group as "Read 3 issues" instead of a generic tool run.
- **One picker**: every picker on the web opens the same searchable menu, and the keyboard works in every one of them again, including the status, priority, effort, model and device menus and the phone's issue search.
- **Runs**: several runs on one issue fold behind a caret on the Runs tab, an empty task list clears instead of sticking, and a turn that changed no file no longer shows an empty edits card.
- **Phones**: the Work screen's menu sits on the Issue face only, Stop and Resume are plain bar buttons, the face switcher counts the run's changed files and tucks into the composer, and a batch run's issues open in a sheet.
- **Devices**: machines fold to one row each, usage windows say when they reset, and a login can be removed from a machine.
- **Under the hood**: the transcript no longer re-parses every patch per frame on the desktop, iOS and Android, a stray invisible character can no longer leak from the desktop editor into a description, and typing with an IME no longer sends a message mid-composition."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_27: ChangelogEntry = ChangelogEntry {
    id: "2026-09-18-device-icons",
    date: "2026-09-18",
    title: "Device icons and 36 more board icons",
    summary: "Give each machine its own icon, from a desktop, server or laptop to the Apple, Windows and Linux marks, and pick from 96 icons for boards and actions.",
    body: r#"- **Device icons**: the device settings carry an icon picker next to the name, on web, desktop, iOS and Android. Choose a monitor, a server, a laptop or the Apple, Windows or Linux mark; the pick shows on the devices list and in every device picker, for you and for teammates who see a shared server.
- **Defaults stay**: a machine you never touched keeps what it had, the monitor for a desktop app and the server for a CLI daemon.
- **More icons**: the board and action icon picker grows from 60 to 96 icons, with music, camera, gift, store, truck, map, mountain, flame, crown, brain, fingerprint, wallet, smartphone and more."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_28: ChangelogEntry = ChangelogEntry {
    id: "2026-09-18-component-foundation",
    date: "2026-09-18",
    title: "One picker, one search field, one date picker",
    summary: "Every picker on the web now looks and behaves the same, search fields carry a glyph and a clear button, and the styleguide sorts into Views, Components and Style.",
    body: r#"- **One picker**: assignee, board, labels, actions, branches, MCP servers, automation filters and the batch issue picker share a single searchable picker. A picked row shows a trailing check; in a multi-select every row carries the same filled or empty circle the iOS and Android sheets draw, and "Unassign" or "None" is a real row instead of a hidden placeholder value.
- **Search fields**: the Reviews file filter, the emoji search and the issue search sheet are one field with the search glyph inside it and a clear button that appears once you type, the same field iOS and Android already had.
- **Due dates**: the three date pickers on the issue page, the editor chips and the phone properties tray are one calendar popover, and a date never shifts by a day west of Greenwich anymore.
- **Autocomplete**: the @, #, : and / menus under the composers share one keyboard model: arrows wrap, Enter or Tab picks, Cmd or Ctrl plus Enter still sends.
- **Styleguide**: a top bar switches between Views (the screenshots), Components (the live controls, grouped by kind) and Style (colours, shape, type, motion and the icon registry). Each component names which app surfaces still draw it by hand.
- **Desktop**: the IDE draws the same set: one search field, one searchable picker, one fold header, one text button and one count capsule. The @, # and : menus in the editors and the composer, and the / menu over the run composer, are one menu now, and it opens above the caret when the window runs out of room below. A run that is working ripples its green dot like the web does; the loading bars, the bulk-select boxes and the checklist boxes are the same shapes as on the web."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_29: ChangelogEntry = ChangelogEntry {
    id: "2026-09-17-release-train",
    date: "2026-09-17",
    title: "Release train 2026-09-17",
    summary: "Every diff reads the same, tabs keep what you typed, issue search is one engine on every client, and the Devices page holds your logins and usage.",
    body: r#"- **One diff design**: a review, a run's changes and the transcript draw the same file card on all four clients, a run's edits collect into one card in the conversation, and the file list beside a diff is a folder tree with counts and a filter.
- **Tabs keep their state**: on the web and the desktop app a half-typed comment, an open reply box and the scroll position come back when you return to a work tab, and a chat run carries the title the agent gave it.
- **One issue search**: every picker, the # autocomplete and the search sheet rank issues the same way on web, desktop, iOS and Android, with the top row preselected and Enter or Tab to pick. The desktop search sheet finds issues only now; files stay in the Files rail.
- **Accounts and usage**: the context ring opens one usage view that names the account a run spends, the Devices page lists each machine's logins with their usage in place, and idle runs no longer pin a stale rate-limit reading.
- **Runs that are still there stay**: the crash sweep no longer deletes a run whose transcript still exists on its device, and the issue page keeps a run's diff after the merge.
- **Merge and conflicts**: a merge refused by a real conflict turns the Merge PR button into Fix conflicts wherever you clicked it, and the Merge PR pill sits on the small phone rung too.
- **Desktop app**: Back returns where you came from, the sidebar list no longer re-derives every row per frame, the terminal copies and pastes, and long chips and diff headers no longer overlap.
- **Web**: every page has a title, and the composers share one attachment thumbnail and one floating-bar chrome."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_30: ChangelogEntry = ChangelogEntry {
    id: "2026-09-tab-shell-polish",
    date: "2026-09-17",
    title: "Tabs keep their state, chats get their names",
    summary: "Switching work tabs no longer drops a half-typed comment, a live run's tab no longer cuts its label off, and a chat run is named after what the agent called it.",
    body: r#"- **Tabs keep what you typed**: on the web and the desktop app, a comment or reply you started, the reply box you opened and how far you had scrolled come back when you return to the tab. Closing the tab forgets them.
- **Room on the right**: a live run's tab has no close button, so its label now gets the same padding on both sides instead of touching the edge, on the web and the desktop app alike.
- **Chats named by the agent**: a chat run shows the title Claude Code or codex gave the conversation instead of "Chat", in every session list, tab and review row on the web, the desktop app, iOS and Android. Renaming it in the agent renames it here."#,
};

/// The entry before that.
#[allow(dead_code)]
const PREVIOUS_31: ChangelogEntry = ChangelogEntry {
    id: "2026-09-diff-ui-one-design",
    date: "2026-09-17",
    title: "One diff design, everywhere",
    summary: "Every diff in Exponential is now built from the same file card, the files a run edits appear in the transcript itself, and the file list beside a diff is a tree.",
    body: r#"- **One file card**: a review, a run's changes and a transcript all draw the same card now, with the same header, the same counts and the same way to open the lines it hides. Cards start open, so a diff reads top to bottom without a click.
- **Edits in the transcript**: the files a run touches in a row are collected into one "N files edited" card in the conversation, with the file it is writing right now open and the rest a tap away. No jumping to another screen to see what changed.
- **A file tree**: the list beside a diff is a tree of folders now, with the counts per folder, a filter, and long folder chains folded into one line. On a review it sits in the sidebar, where every other page keeps its context.
- **Reviews polish**: a review's header says which issue it is, how big the diff is and what the pull request's state is, with reject, Merge PR and the GitHub link beside it. Batch rows in the review queue no longer lose their first column. On phones the floating bottom bar is now the same on web, iOS and Android: one geometry, no drop shadow, and Merge PR is one white pill centred between the circles, on a review and on a run's changes alike."#,
};

/// Whether the rail's "What's new" card renders, given the stored
/// `changelogSeenId`. Pure so the rule is testable without a gpui App: a
/// user who has never dismissed anything sees the card, and a user whose
/// stored id is an OLDER entry sees it again (that is the point of the id
/// being the dismissal key rather than a boolean).
pub(crate) fn whats_new_visible(seen: Option<&str>) -> bool {
    seen != Some(LATEST.id)
}

/// Persist [`LATEST`]'s id as seen — the ✕ on the card, and opening the
/// dialog. Idempotent: a no-op write still costs one settings save, so the
/// callers gate on [`whats_new_visible`] where it matters.
pub(crate) fn mark_seen(cx: &mut App) {
    let hub = coding_flow::CodingHub::global(cx);
    let mut settings = hub.read(cx).settings.clone();
    if settings.changelog_seen_id.as_deref() == Some(LATEST.id) {
        return;
    }
    settings.changelog_seen_id = Some(LATEST.id.to_string());
    if let Err(err) = coding_flow::CodingHub::save_settings(&hub, settings, cx) {
        log::warn!("[ui] persisting the changelog dismissal failed: {err}");
    }
}

/// Open the "What's new" dialog: date, title and the entry's GFM body. Also
/// marks the entry seen — reading it IS dismissing it, exactly like the web
/// card's "open the sheet" path.
pub(crate) fn open_whats_new(window: &mut Window, cx: &mut App) {
    mark_seen(cx);
    // Roughly the web sheet's `sm:max-w-lg`; the body is a handful of bullets
    // and the view scrolls when a longer entry lands.
    let spec = DialogSpec::new("What's new", size(px(520.), px(440.)))
        .resizable(size(px(360.), px(240.)));
    native_dialog::open_dialog_window(window, cx, spec, move |_window, cx| {
        DialogContent::new(cx.new(|_| WhatsNewView))
    });
}

/// The dialog body — a pure read-only render of [`LATEST`].
struct WhatsNewView;

impl Render for WhatsNewView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(LATEST.date),
            )
            .child(
                div()
                    .text_base()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(LATEST.title),
            )
            .child(
                div().pt_2().text_sm().child(
                    // Same glass code-block treatment as every other markdown
                    // surface in the app.
                    TextView::markdown("whats-new-body", SharedString::from(LATEST.body))
                        .style(crate::surface::markdown_style())
                        .selectable(true),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_shows_until_the_head_entry_itself_is_seen() {
        assert!(whats_new_visible(None), "a fresh install sees the card");
        assert!(
            whats_new_visible(Some("2026-09-merge-agent-run-prs")),
            "an OLDER dismissal re-surfaces the card — the id is the key"
        );
        assert!(!whats_new_visible(Some(LATEST.id)));
    }

    /// The mirror's shape, so a bad copy/paste fails here rather than in the
    /// web suite that reads this file.
    #[test]
    fn the_mirrored_entry_is_filled_in() {
        assert!(LATEST.id.starts_with("2026-"));
        assert_eq!(LATEST.date.len(), 10, "ISO date, display only");
        assert!(!LATEST.title.is_empty());
        assert!(!LATEST.summary.is_empty());
        assert!(LATEST.body.starts_with("- **"), "GFM bullets");
        // The authoring convention the web file documents.
        assert!(!LATEST.summary.contains('—'), "no em dashes in changelog copy");
        assert!(!LATEST.body.contains('—'), "no em dashes in changelog copy");
    }
}
