-- EXP-690: skip permissions is no longer a setting (every launch bypasses; plan
-- mode is the only starting-mode choice). Strip the retired per-agent
-- `skipPermissions` key from stored launch defaults. No launch_defaults_updated_at
-- bump: clients already ignore the key, and a moved stamp would make every synced
-- device re-apply an identical copy.
UPDATE devices
SET launch_defaults = jsonb_set(
  launch_defaults, '{agents}',
  (SELECT jsonb_object_agg(a.key,
      CASE WHEN jsonb_typeof(a.value) = 'object' THEN a.value - 'skipPermissions' ELSE a.value END)
   FROM jsonb_each(launch_defaults->'agents') AS a))
WHERE jsonb_typeof(launch_defaults->'agents') = 'object'
  AND EXISTS (SELECT 1 FROM jsonb_each(launch_defaults->'agents') AS a
              WHERE jsonb_typeof(a.value) = 'object' AND a.value ? 'skipPermissions');
