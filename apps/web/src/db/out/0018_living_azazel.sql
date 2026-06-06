CREATE TYPE "public"."issue_event_type" AS ENUM('status_changed', 'assignee_changed', 'label_added', 'label_removed', 'pr_opened', 'pr_merged', 'plan_ready', 'agent_error', 'agent_started', 'agent_question', 'agent_answer');--> statement-breakpoint
CREATE TYPE "public"."pr_state" AS ENUM('open', 'closed', 'merged', 'draft');--> statement-breakpoint
CREATE TYPE "public"."run_mode" AS ENUM('background', 'interactive');--> statement-breakpoint
CREATE TYPE "public"."subscriber_source" AS ENUM('creator', 'assignee', 'commenter', 'manual', 'mention');--> statement-breakpoint
ALTER TABLE "agent_runs" ALTER COLUMN "run_mode" SET DATA TYPE "public"."run_mode" USING "run_mode"::"public"."run_mode";--> statement-breakpoint
ALTER TABLE "issue_events" ALTER COLUMN "type" SET DATA TYPE "public"."issue_event_type" USING "type"::"public"."issue_event_type";--> statement-breakpoint
ALTER TABLE "issue_subscribers" ALTER COLUMN "source" SET DATA TYPE "public"."subscriber_source" USING "source"::"public"."subscriber_source";--> statement-breakpoint
ALTER TABLE "issues" ALTER COLUMN "pr_state" SET DATA TYPE "public"."pr_state" USING "pr_state"::"public"."pr_state";