-- Token counts from each assistant entry's message.usage. A response split
-- across several entries repeats its input counts and grows its output
-- count, so totals take the maximum per message_id, never the sum.
ALTER TABLE entries ADD COLUMN input_tokens INTEGER;
ALTER TABLE entries ADD COLUMN output_tokens INTEGER;
ALTER TABLE entries ADD COLUMN cache_read_tokens INTEGER;
ALTER TABLE entries ADD COLUMN cache_write_tokens INTEGER;
CREATE INDEX entries_message ON entries (message_id) WHERE message_id IS NOT NULL;
