-- Give every pre-collapse row a stored icon before the legacy column goes:
-- clients keep a cosmetic fallback, but a populated icon means no project
-- ever changes glyph when `type` disappears.
UPDATE "projects" SET "icon" = CASE "type"
  WHEN 'feedback' THEN 'megaphone'
  WHEN 'tasks' THEN 'square-kanban'
  ELSE 'code'
END WHERE "icon" IS NULL;--> statement-breakpoint
