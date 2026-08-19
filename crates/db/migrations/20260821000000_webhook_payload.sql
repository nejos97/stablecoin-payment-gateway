-- Body of the most recent delivery attempt, so the dashboard can show what
-- was actually sent. NULL until a first attempt is made (and on legacy rows).
ALTER TABLE webhook_deliveries ADD COLUMN "lastPayload" JSONB;
