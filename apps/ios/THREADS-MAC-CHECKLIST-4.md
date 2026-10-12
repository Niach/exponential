
# iOS lane: merge of origin/exp/batch-ddafdb85 (StartIntent) into EXP-1284 (Linux, uncompiled)

## Files
- `Exponential/UI/Agent/AgentPageView.swift`: CONFLICT RESOLVED in the working tree (markers gone;
  the index still says `UU`, the coordinator stages it). Kept both sides:
  - onDisappear = the batch's B5 rule (the start watch is NOT stopped, no `abandonStarting`) plus
    ours `threads?.stop()`.
  - Landing = the batch's `composer.landedSession` (only an unchanged draft, only while `visible`;
    off-screen it waits for `onAppear`) routed through our NEW `land(_:)`: a composer with a
    `threadId` → `pushRoute(.thread(…, child: .conversation))` (pops back to that thread page, no
    duplicate); otherwise `sessionTarget` → `RunThreadScreen`. `onAppear`'s deferred landing goes
    through `land(_:)` too.
- `Exponential/UI/Thread/ThreadScreen.swift`: the reply box's RESUME now carries ONE `startId`
  per press (`StartIntent.press(target: StartIntent.resumeTarget(sessionId:deviceId:account: nil))`,
  `@State resumeIntent`). Both Retry toasts (the resume refusal and the 30 s hold) call
  `sendReply(retry: true)` → `StartIntent.retry` reuses the same id. The intent clears when the
  held reply is delivered. `onSend` is now a closure (`{ sendReply() }`).

## threadId audit on the StartIntent surfaces
- Composer (`AgentComposerModel.dispatch`, all 4 `startSession` calls): `threadId: threadId,
  startId: startId`. OK. `threadId` comes from `AgentComposerSeed.threadId`.
- Thread "Start a session here" and the reply's `.start` → `openComposer` → the seed carries
  `threadId` (+ teamId, text). OK.
- Reply `.resume` → `resumeSession(…, startId:)`. A resume never carries `threadId` (the server
  refuses it; a resume stays in its predecessor's thread). OK.
- WorkScreen Resume / account switch (`RunContinuation`, batch code): no thread field, same rule.

## Verify
```bash
cd apps/ios && tuist generate --no-open && tuist test ExpCore && tuist test ExpUI
xcodebuild -scheme Exponential -destination 'platform=iOS Simulator,name=iPhone 17' build
```
- `SteerStartInputEncodingTests`: the EXP-1284 `threadId` test still passes next to the batch's
  `startId` fields (memberwise order is `…, threadId, startId`).
- By hand:
  - Start a session here from a thread → Start → back on that thread's Conversation (composer
    popped).
  - Leave the Agent page before the session lands → on return it lands once.
  - Reply to my ended session with its machine online → resume. Kill the network → the error toast
    with Retry. Retry is the SAME startId (server log / relay frame).
