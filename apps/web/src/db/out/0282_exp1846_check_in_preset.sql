-- #EXP-1846 (agent-settings.json `checkIn`): the Check in is the schedule
-- trigger carrying `preset`. Each Assistant's single preset-cron schedule
-- (its old Check in) is tagged with its preset; team agents' schedules stay
-- plain triggers.
UPDATE "agents" AS a
SET "triggers" = jsonb_set(a."triggers", '{0,preset}', to_jsonb(p.preset))
FROM (VALUES
  ('*/15 * * * *', '15m'),
  ('*/30 * * * *', '30m'),
  ('0 * * * *', '1h'),
  ('0 9 * * *', 'daily')
) AS p(cron, preset)
WHERE a."role" = 'assistant'
  AND jsonb_typeof(a."triggers") = 'array'
  AND jsonb_array_length(a."triggers") = 1
  AND a."triggers"->0->>'kind' = 'schedule'
  AND NOT (a."triggers"->0 ? 'preset')
  AND a."triggers"->0->>'cron' = p.cron;
