CREATE TABLE "workflow_events" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"workflow_id" uuid NOT NULL,
	"team_id" uuid NOT NULL,
	"node_id" uuid,
	"session_id" uuid,
	"at" timestamp with time zone DEFAULT now() NOT NULL,
	"kind" varchar(32) NOT NULL,
	"message" varchar(500) DEFAULT '' NOT NULL
);
--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "workflow_id" uuid;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "workflow_node_id" uuid;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "workflow_role" varchar(16);--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD COLUMN "pending_question" jsonb;--> statement-breakpoint
ALTER TABLE "workflow_events" ADD CONSTRAINT "workflow_events_workflow_id_workflows_id_fk" FOREIGN KEY ("workflow_id") REFERENCES "public"."workflows"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workflow_events" ADD CONSTRAINT "workflow_events_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workflow_events" ADD CONSTRAINT "workflow_events_node_id_workflow_nodes_id_fk" FOREIGN KEY ("node_id") REFERENCES "public"."workflow_nodes"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "workflow_events" ADD CONSTRAINT "workflow_events_session_id_coding_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."coding_sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_workflow_events_workflow_at" ON "workflow_events" USING btree ("workflow_id","at");--> statement-breakpoint
CREATE INDEX "idx_workflow_events_team" ON "workflow_events" USING btree ("team_id");--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD CONSTRAINT "coding_sessions_workflow_id_workflows_id_fk" FOREIGN KEY ("workflow_id") REFERENCES "public"."workflows"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "coding_sessions" ADD CONSTRAINT "coding_sessions_workflow_node_id_workflow_nodes_id_fk" FOREIGN KEY ("workflow_node_id") REFERENCES "public"."workflow_nodes"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_coding_sessions_workflow" ON "coding_sessions" USING btree ("workflow_id") WHERE workflow_id IS NOT NULL;