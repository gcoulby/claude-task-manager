-- When a transcript file shrinks it was rewritten, not appended to. The rows
-- read from the old version are kept under a retired source instead of
-- being deleted, so the archive never loses history.
ALTER TABLE sources ADD COLUMN replaced_at_ms INTEGER;
