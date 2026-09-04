CREATE TYPE "public"."issue_relation_source" AS ENUM('user', 'reference');--> statement-breakpoint
CREATE TYPE "public"."issue_relation_type" AS ENUM('blocks', 'parent', 'duplicate', 'related');--> statement-breakpoint
ALTER TYPE "public"."issue_event_type" ADD VALUE 'relation_added';--> statement-breakpoint
ALTER TYPE "public"."issue_event_type" ADD VALUE 'relation_removed';--> statement-breakpoint
CREATE TABLE "issue_relations" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"issue_id" uuid NOT NULL,
	"related_issue_id" uuid NOT NULL,
	"type" "issue_relation_type" NOT NULL,
	"source" "issue_relation_source" DEFAULT 'user' NOT NULL,
	"team_id" uuid NOT NULL,
	"board_id" uuid NOT NULL,
	"board_deleted_at" timestamp with time zone,
	"board_archived_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "chk_issue_relations_not_self" CHECK ("issue_relations"."issue_id" <> "issue_relations"."related_issue_id")
);
--> statement-breakpoint
ALTER TABLE "issue_relations" ADD CONSTRAINT "issue_relations_issue_id_issues_id_fk" FOREIGN KEY ("issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_relations" ADD CONSTRAINT "issue_relations_related_issue_id_issues_id_fk" FOREIGN KEY ("related_issue_id") REFERENCES "public"."issues"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_relations" ADD CONSTRAINT "issue_relations_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "issue_relations" ADD CONSTRAINT "issue_relations_board_id_boards_id_fk" FOREIGN KEY ("board_id") REFERENCES "public"."boards"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_issue_relations_pair_type" ON "issue_relations" USING btree ("issue_id","related_issue_id","type");--> statement-breakpoint
CREATE INDEX "idx_issue_relations_related" ON "issue_relations" USING btree ("related_issue_id");--> statement-breakpoint
CREATE INDEX "idx_issue_relations_team" ON "issue_relations" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_issue_relations_board" ON "issue_relations" USING btree ("board_id");--> statement-breakpoint
-- EXP-736 backfill: pre-existing duplicate links get their mirror row, so
-- the relations card/shape is not blind to every duplicate marked before this
-- migration. team_id/board_id are selected explicitly because the
-- denormalizing BEFORE INSERT triggers (db/out/custom/0001_triggers.sql) are
-- applied at app boot and may not exist yet at migration time;
-- board_deleted_at/board_archived_at stay NULL until the next board fan-out.
INSERT INTO "issue_relations" ("issue_id", "related_issue_id", "type", "source", "team_id", "board_id")
SELECT "id", "duplicate_of_id", 'duplicate', 'user', "team_id", "board_id"
FROM "issues"
WHERE "duplicate_of_id" IS NOT NULL AND "id" <> "duplicate_of_id"
ON CONFLICT DO NOTHING;