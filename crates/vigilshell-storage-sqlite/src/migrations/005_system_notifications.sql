ALTER TABLE preferences RENAME COLUMN task_completion_notification_mode TO system_notification_mode;
ALTER TABLE preferences ADD COLUMN task_completion_notification_enabled INTEGER NOT NULL DEFAULT 1 CHECK(task_completion_notification_enabled IN (0,1));
ALTER TABLE preferences ADD COLUMN execution_issue_notification_enabled INTEGER NOT NULL DEFAULT 1 CHECK(execution_issue_notification_enabled IN (0,1));
UPDATE preferences SET system_notification_mode='unfocused', attention_required_notification_enabled=1;
