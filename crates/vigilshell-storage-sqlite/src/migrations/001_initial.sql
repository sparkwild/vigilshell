-- 连接配置密文基线；不兼容旧连接表，新建数据库直接使用此结构。
CREATE TABLE connection_profiles (id TEXT PRIMARY KEY NOT NULL, group_id TEXT REFERENCES connection_groups(id) ON DELETE SET NULL, group_sort_order INTEGER NOT NULL CHECK(group_sort_order>=0), created_at TEXT, version INTEGER NOT NULL CHECK(version > 0 AND version <= 9007199254740991), revision INTEGER NOT NULL CHECK(revision > 0 AND revision <= 9007199254740991), credential_ref TEXT UNIQUE, deleting INTEGER NOT NULL DEFAULT 0 CHECK(deleting IN (0,1)), proxy_id TEXT REFERENCES proxy_profiles(id), config BLOB NOT NULL CHECK(length(config)>=48), enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1))) STRICT;
CREATE TABLE connection_configuration (id INTEGER PRIMARY KEY CHECK(id=1), verifier BLOB CHECK(verifier IS NULL OR length(verifier)>=48)) STRICT;
INSERT INTO connection_configuration VALUES(1,NULL);
CREATE TABLE credential_jobs (id TEXT PRIMARY KEY NOT NULL, connection_id TEXT NOT NULL, new_ref TEXT, old_ref TEXT, action TEXT NOT NULL CHECK(action IN ('save','delete','clear'))) STRICT;
CREATE TABLE host_trusts (connection_id TEXT PRIMARY KEY NOT NULL REFERENCES connection_profiles(id) ON DELETE CASCADE, host TEXT NOT NULL, port INTEGER NOT NULL CHECK(port BETWEEN 1 AND 65535), public_key TEXT NOT NULL, fingerprint TEXT NOT NULL, algorithm TEXT NOT NULL, confirmed_at TEXT NOT NULL) STRICT;
CREATE TABLE preferences (id INTEGER PRIMARY KEY CHECK(id=1), task_completion_notification_mode TEXT NOT NULL DEFAULT 'unfocused' CHECK(task_completion_notification_mode IN ('off','unfocused','always')), attention_required_notification_enabled INTEGER NOT NULL DEFAULT 1 CHECK(attention_required_notification_enabled IN (0,1)), theme_mode TEXT NOT NULL CHECK(theme_mode IN ('system','light','dark')), locale_mode TEXT NOT NULL CHECK(locale_mode IN ('system','en','zh-CN')), terminal_font_size INTEGER NOT NULL DEFAULT 12, terminal_line_height INTEGER NOT NULL DEFAULT 14, terminal_scrollback INTEGER NOT NULL DEFAULT 10000, terminal_cursor_style TEXT NOT NULL DEFAULT 'block', terminal_cursor_blink INTEGER NOT NULL DEFAULT 1, connection_open_behavior TEXT NOT NULL DEFAULT 'ask', files_show_hidden INTEGER NOT NULL DEFAULT 1, files_show_tree INTEGER NOT NULL DEFAULT 1, files_columns TEXT NOT NULL DEFAULT '["name","size","kind","modified","permissions","owner"]', download_mode TEXT NOT NULL DEFAULT 'ask', download_directory TEXT NOT NULL DEFAULT '', monitor_auto_refresh INTEGER NOT NULL DEFAULT 1, monitor_sample_seconds INTEGER NOT NULL DEFAULT 5, monitor_full_seconds INTEGER NOT NULL DEFAULT 60, terminal_copy_on_select INTEGER NOT NULL DEFAULT 0, terminal_theme_mode TEXT NOT NULL DEFAULT 'inherit', terminal_font_family TEXT NOT NULL DEFAULT 'jetbrains-mono', sidebar_hover_preview INTEGER NOT NULL DEFAULT 1, files_auto_open INTEGER NOT NULL DEFAULT 1, remote_directory_mode TEXT NOT NULL DEFAULT 'last', revision INTEGER NOT NULL CHECK(revision >= 0 AND revision <= 9007199254740991)) STRICT;
CREATE TABLE connection_groups (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL UNIQUE CHECK(length(name)>0), sort_order INTEGER NOT NULL CHECK(sort_order>=0), version INTEGER NOT NULL CHECK(version > 0 AND version <= 9007199254740991)) STRICT;
CREATE TABLE workspace_meta (id INTEGER PRIMARY KEY CHECK(id=1), namespace TEXT NOT NULL, credential_version INTEGER NOT NULL CHECK(credential_version=1), revision INTEGER NOT NULL CHECK(revision >= 0 AND revision <= 9007199254740991)) STRICT;
INSERT INTO preferences(id,theme_mode,locale_mode,remote_directory_mode,revision) VALUES(1,'system','system','initial',0);
INSERT INTO workspace_meta VALUES(1,lower(hex(randomblob(16))),1,0);
CREATE TABLE schema_baseline (id INTEGER PRIMARY KEY CHECK(id=1), generation INTEGER NOT NULL CHECK(generation=8)) STRICT;
INSERT INTO schema_baseline VALUES(1,8);
PRAGMA application_id = 0x56534831;

CREATE TABLE agent_recordings (
  id TEXT PRIMARY KEY NOT NULL,
  connection_id TEXT NOT NULL,
  entry_id TEXT NOT NULL,
  created_at INTEGER NOT NULL CHECK(created_at>=0),
  ended_at INTEGER CHECK(ended_at>=created_at),
  result TEXT,
  incomplete INTEGER NOT NULL CHECK(incomplete IN (0,1)),
  sequence INTEGER NOT NULL CHECK(sequence>=0),
  bytes INTEGER NOT NULL CHECK(bytes>=0),
  deleting INTEGER NOT NULL CHECK(deleting IN (0,1)),
  body TEXT NOT NULL CHECK(json_valid(body))
) STRICT;
CREATE INDEX agent_recordings_filter ON agent_recordings(connection_id,entry_id,created_at);
CREATE TABLE agent_recording_events (
  session_id TEXT NOT NULL REFERENCES agent_recordings(id) ON DELETE CASCADE,
  sequence INTEGER NOT NULL CHECK(sequence>0),
  task_id TEXT,
  kind TEXT NOT NULL CHECK(kind IN ('session','policy','operation','input','output','resize','files','decision','control','gap','transfer','screen','script')),
  timestamp_ms INTEGER NOT NULL CHECK(timestamp_ms>=0),
  position TEXT NOT NULL CHECK(json_valid(position)),
  PRIMARY KEY(session_id,sequence)
) STRICT;
CREATE INDEX agent_recording_steps ON agent_recording_events(session_id,sequence) WHERE kind<>'output';
CREATE INDEX agent_recording_task ON agent_recording_events(session_id,task_id,sequence);

CREATE TABLE group_layout (id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL CHECK(revision>=0 AND revision<=9007199254740991)) STRICT;
INSERT INTO group_layout VALUES(1,0);

CREATE TABLE proxy_profiles (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL UNIQUE CHECK(length(name)>0), kind TEXT NOT NULL CHECK(kind IN ('socks5','http')), host TEXT NOT NULL CHECK(length(host)>0), port INTEGER NOT NULL CHECK(port BETWEEN 1 AND 65535), username TEXT NOT NULL, credential_ref TEXT UNIQUE, version INTEGER NOT NULL CHECK(version>0 AND version<=9007199254740991)) STRICT;

CREATE TABLE ui_state (scope TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL CHECK(revision>=0 AND revision<=9007199254740991), data TEXT NOT NULL CHECK(json_valid(data))) STRICT;

CREATE TABLE credential_vault (id INTEGER PRIMARY KEY CHECK(id=1), key_version INTEGER NOT NULL CHECK(key_version>0), initialized INTEGER NOT NULL CHECK(initialized IN (0,1))) STRICT;
INSERT INTO credential_vault VALUES(1,1,0);
CREATE TABLE encrypted_credentials (reference TEXT PRIMARY KEY NOT NULL, owner_id TEXT NOT NULL, purpose TEXT NOT NULL CHECK(purpose IN ('ssh_password','ssh_private_key','proxy_password')), format_version INTEGER NOT NULL CHECK(format_version>0), key_version INTEGER NOT NULL CHECK(key_version>0), nonce BLOB NOT NULL CHECK(length(nonce)=24), ciphertext BLOB NOT NULL CHECK(length(ciphertext) BETWEEN 17 AND 1048592)) STRICT;

CREATE TABLE agent_configuration (id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL CHECK(revision>=0), body TEXT NOT NULL) STRICT;
INSERT INTO agent_configuration VALUES(1,0,'{"settings":{"revision":0,"enabled":false,"port":51602,"showTray":true,"retentionDays":30,"storageMib":1024},"entries":[],"policies":[]}');
CREATE TABLE agent_tasks (id TEXT PRIMARY KEY, entry_id TEXT NOT NULL, connection_id TEXT NOT NULL, created_at INTEGER NOT NULL, active INTEGER NOT NULL CHECK(active IN (0,1)), request_key TEXT, fingerprint TEXT, body TEXT NOT NULL) STRICT;
CREATE UNIQUE INDEX agent_task_request ON agent_tasks(entry_id,request_key);
CREATE INDEX agent_task_history ON agent_tasks(created_at DESC);

CREATE TABLE agent_receipts (entry_id TEXT NOT NULL, request_key TEXT NOT NULL, task_id TEXT NOT NULL, fingerprint TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY(entry_id,request_key)) STRICT;

CREATE TABLE agent_access_observations (entry_id TEXT NOT NULL, generation INTEGER NOT NULL CHECK(generation>0), endpoint TEXT NOT NULL, endpoint_revision INTEGER NOT NULL CHECK(endpoint_revision>=0), first_at INTEGER NOT NULL, last_at INTEGER NOT NULL, PRIMARY KEY(entry_id,generation,endpoint,endpoint_revision)) STRICT;
