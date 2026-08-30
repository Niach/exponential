-- EXP-685: retire the builtin "Todo" status. Todo and Backlog meant the same
-- thing, so every Todo issue moves to its team's Backlog row and the Todo
-- builtin rows go; the `unstarted` category stays (empty until a team adds a
-- custom status, which now anchors to `backlog`). The PG enum keeps the
-- orphan `todo` label (dropping a value needs a type recreate for nothing —
-- the EXP-540 `merged` precedent); the seed trigger + boot heal pass in
-- custom/0001_triggers.sql no longer produce the row. Migrations are silent:
-- no status_changed events, no updated_at bump (the 0052 guard pattern), and
-- populate_issue_status_id is held off so an issue on a CUSTOM unstarted row
-- keeps that row while only its anchor enum flips.
DO $$
BEGIN
	IF EXISTS (
		SELECT 1 FROM pg_trigger
		WHERE tgname = 'update_updated_at' AND tgrelid = 'issues'::regclass
	) THEN
		EXECUTE 'ALTER TABLE issues DISABLE TRIGGER update_updated_at';
	END IF;
	IF EXISTS (
		SELECT 1 FROM pg_trigger
		WHERE tgname = 'populate_issue_status_id' AND tgrelid = 'issues'::regclass
	) THEN
		EXECUTE 'ALTER TABLE issues DISABLE TRIGGER populate_issue_status_id';
	END IF;

	-- 1. Issues on the builtin Todo row (or todo-anchored with no row at all)
	--    land on the team's Backlog row.
	UPDATE issues i SET status = 'backlog', status_id = b.id
	FROM issue_statuses b
	WHERE b.team_id = i.team_id AND b.builtin_key = 'backlog'
	  AND (
	    i.status_id IN (SELECT id FROM issue_statuses WHERE builtin_key = 'todo')
	    OR (i.status = 'todo' AND i.status_id IS NULL)
	  );

	-- 2. Issues on a CUSTOM unstarted row keep the row; only the anchor moves.
	UPDATE issues SET status = 'backlog' WHERE status = 'todo';

	-- 3a. An automation whose status filter listed ONLY Todo rows would be
	--     widened by the scrub below to fire on EVERY status change (an empty
	--     filter means pass-all). Disable it instead; the owner re-points it.
	UPDATE automations a
	SET enabled = false
	WHERE jsonb_typeof(a.trigger->'filters'->'toStatusIds') = 'array'
	  AND jsonb_array_length(a.trigger->'filters'->'toStatusIds') > 0
	  AND NOT EXISTS (
	    SELECT 1 FROM jsonb_array_elements(a.trigger->'filters'->'toStatusIds') AS x
	    WHERE (x #>> '{}') NOT IN (
	      SELECT id::text FROM issue_statuses WHERE builtin_key = 'todo'
	    )
	  );

	-- 3. Automation status filters referencing a Todo row id (jsonb, no FK).
	UPDATE automations a
	SET trigger = jsonb_set(
		a.trigger,
		'{filters,toStatusIds}',
		COALESCE((
			SELECT jsonb_agg(x)
			FROM jsonb_array_elements(a.trigger->'filters'->'toStatusIds') AS x
			WHERE (x #>> '{}') NOT IN (
				SELECT id::text FROM issue_statuses WHERE builtin_key = 'todo'
			)
		), '[]'::jsonb)
	)
	WHERE jsonb_typeof(a.trigger->'filters'->'toStatusIds') = 'array'
	  AND EXISTS (
	    SELECT 1 FROM jsonb_array_elements(a.trigger->'filters'->'toStatusIds') AS x
	    WHERE (x #>> '{}') IN (SELECT id::text FROM issue_statuses WHERE builtin_key = 'todo')
	  );

	-- 4. Drop the rows. teams.pr_opened_status_id / pr_merged_status_id are
	--    ON DELETE SET NULL (NULL = the builtin default target).
	DELETE FROM issue_statuses WHERE builtin_key = 'todo';

	IF EXISTS (
		SELECT 1 FROM pg_trigger
		WHERE tgname = 'populate_issue_status_id' AND tgrelid = 'issues'::regclass
	) THEN
		EXECUTE 'ALTER TABLE issues ENABLE TRIGGER populate_issue_status_id';
	END IF;
	IF EXISTS (
		SELECT 1 FROM pg_trigger
		WHERE tgname = 'update_updated_at' AND tgrelid = 'issues'::regclass
	) THEN
		EXECUTE 'ALTER TABLE issues ENABLE TRIGGER update_updated_at';
	END IF;
END $$;
