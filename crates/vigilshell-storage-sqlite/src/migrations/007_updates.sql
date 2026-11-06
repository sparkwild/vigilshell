ALTER TABLE preferences ADD COLUMN auto_check_updates INTEGER NOT NULL DEFAULT 1 CHECK(auto_check_updates IN (0,1));
