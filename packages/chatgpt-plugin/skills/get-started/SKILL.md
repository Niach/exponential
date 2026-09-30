---
name: get-started
description: >
  First run after installing the Exponential plugin: confirm the connection,
  find the user's teams and boards, agree on the board the conversation is
  about, and show what is open there.
---

# Get started with Exponential

Run this once after the plugin is installed, or whenever the user asks how
to set Exponential up.

## 1. Confirm the connection

Call `exponential_teams_list`.

- If the call needs authentication, the host opens Exponential's sign-in and
  a consent screen. On that screen the user picks what this connection may
  touch: everything, specific teams, or specific boards. Tell the user that
  the grant can be narrowed or widened later by reconnecting the plugin.
- If the result is empty, the account is a member of no team, or the grant
  excludes every team. Say so and point the user to the web app
  (https://app.exponential.at) to create or join a team, then try again.
  Never create a team on the user's behalf unless they ask for it.

## 2. Find the boards

Call `exponential_boards_list` for the team (or for all teams when there is
more than one). Show each board with its identifier prefix (issues on a board
with prefix `WEB` are named `WEB-12`), and whether it is backed by a GitHub
repository, because only those boards support pull requests and coding
sessions.

## 3. Agree on a default board

If the user has one board, use it. Otherwise ask which board this
conversation is about and remember the choice for the rest of the
conversation. Every later create or list call passes that board's id.

## 4. Show what is open

Call `exponential_issues_list` for the chosen board. It returns open work
only, unless asked for closed issues too. Group the result by status, name
each issue by identifier and title, and mention priority and assignee when
set. Offer the next steps the user is most likely to want: file an issue,
read one in detail, or comment.

## What to keep in mind

- Identifiers like `DEMO-3` work wherever a tool takes an issue id.
- Descriptions and comments are plain Markdown. `@email` mentions a member,
  `#DEMO-3` references another issue.
- Creating, commenting and labelling are additive. Updating, deleting,
  merging and closing overwrite or end something: confirm with the user
  before calling those, and never run them in bulk from one request.
