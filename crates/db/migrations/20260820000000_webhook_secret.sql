-- Optional per-endpoint signing secret. When set, every delivery towards the
-- endpoint carries an X-Webhook-Signature header (HMAC-SHA256 of the body).
-- Write-only through the API: never returned, only exposed as has_secret.
ALTER TABLE "webhook_endpoints" ADD COLUMN "secret" TEXT;
