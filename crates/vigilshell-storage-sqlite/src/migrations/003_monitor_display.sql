ALTER TABLE preferences ADD COLUMN monitor_display TEXT NOT NULL DEFAULT 'bar' CHECK(monitor_display IN ('bar','chart'));
