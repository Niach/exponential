# scripts

One-shot operational scripts. Not part of the test suite.

## record-google-verification.ts

Records the end-to-end Google Calendar OAuth flow as a webm video, for
submission to Google's OAuth verification reviewers.

```sh
EXP_EMAIL=danny@straehhuber.com \
EXP_PASSWORD=… \
bun run scripts/record-google-verification.ts
```

The script uses a persistent Chromium profile at
`~/.cache/exponential-recording-profile/` so Google's bot detection
treats the browser as trusted across runs. First run will require a
manual Google sign-in; subsequent runs skip it.

Output: `recordings/verification-<timestamp>.webm`

### What's automated

1. Sign-in to Exponential (uses `EXP_EMAIL` / `EXP_PASSWORD`)
2. Navigate to `/account/integrations`
3. Disconnect any existing Google session (clean recording)
4. Click "Connect Google Calendar"
5. Wait for the Google OAuth flow to round-trip back to the app

### What you do interactively

- The Google OAuth consent screen (pick account, click Continue,
  grant `calendar.events`) — script auto-resumes when you land back
  in the app
- Create an issue with a due date in the recorded browser, then
  press Enter in the terminal
- Switch to the Google Calendar tab the script opens, navigate to
  the right week so the synced event is visible, press Enter to stop

### Tips for the verification recording

- Pause briefly on the Google consent screen so the reviewer can
  read the requested scope
- The OAuth Client ID is visible in the consent screen URL — that
  satisfies Google's "OAuth ID must be visible" requirement
- Total length should be under 90 seconds; trim/upload as Unlisted
  on YouTube and paste the URL into the verification form
