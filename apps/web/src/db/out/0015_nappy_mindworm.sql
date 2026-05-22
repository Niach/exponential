ALTER TABLE "workspace_agents" ADD COLUMN "whatsapp_own_jid" text;--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD COLUMN "whatsapp_chats" jsonb;--> statement-breakpoint
ALTER TABLE "workspace_agents" ADD COLUMN "whatsapp_notify_jid" text;