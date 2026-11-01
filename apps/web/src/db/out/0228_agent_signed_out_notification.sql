ALTER TYPE "public"."notification_type" ADD VALUE 'agent_signed_out';--> statement-breakpoint
ALTER TABLE "devices" ADD COLUMN "agent_sign_out_notified" jsonb DEFAULT '[]'::jsonb NOT NULL;--> statement-breakpoint
ALTER TABLE "notifications" ADD COLUMN "device_id" uuid;--> statement-breakpoint
ALTER TABLE "notifications" ADD CONSTRAINT "notifications_device_id_devices_id_fk" FOREIGN KEY ("device_id") REFERENCES "public"."devices"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_notifications_device" ON "notifications" USING btree ("device_id") WHERE device_id IS NOT NULL;