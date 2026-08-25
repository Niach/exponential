-- EXP-540: the pre-EXP-498 `merged` park state is never written anew (a PR
-- merge ENDS the session since EXP-498). Move the last parked rows off it so
-- the server special cases, the contract entry and the client fallbacks can
-- go. `ended_at` is only stamped where it is missing — a row that already
-- carries one keeps its real end time. The PG enum keeps the orphan `merged`
-- label (dropping a value needs a type recreate for nothing).
UPDATE coding_sessions
SET status = 'ended',
    ended_at = COALESCE(ended_at, updated_at, now()),
    updated_at = now()
WHERE status = 'merged';