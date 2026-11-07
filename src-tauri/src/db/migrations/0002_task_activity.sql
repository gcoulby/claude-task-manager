-- When the task's transcript file last changed, so staleness can be judged
-- at read time against the current clock.
ALTER TABLE tasks ADD COLUMN activity_ms INTEGER NOT NULL DEFAULT 0;
