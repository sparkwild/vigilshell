//! 来源只关联已记录文件效果和内容摘要；历史正文始终只供展示与关联。
use super::*;
impl LocalExecutor {
    pub(super) async fn script_origin(
        &self,
        task: &ExecutionTask,
        snapshot: &ScriptSnapshot,
    ) -> Option<ScriptOrigin> {
        if !matches!(
            task.request.operation,
            RuntimeOperation::Script {
                source: ScriptSource::RemoteFile { .. },
                ..
            }
        ) {
            return None;
        }
        let filter = TaskHistoryFilter {
            entry_id: Some(task.entry_id.clone()),
            connection_id: Some(task.connection_id.clone()),
            phase: Some("succeeded".into()),
            ..Default::default()
        };
        let DbReply::AgentHistory(AgentHistoryReply::Tasks(page)) = self
            .database
            .execute(DbCommand::AgentHistory(AgentHistoryCommand::Tasks {
                filter,
            }))
            .await
            .ok()?
        else {
            return None;
        };
        for row in page.tasks {
            if let Ok(DbReply::Execution(ExecutionDbReply::Task(Some(previous)))) = self
                .database
                .execute(DbCommand::Execution(ExecutionDbCommand::Get {
                    entry: task.entry_id.clone(),
                    id: row.id.clone(),
                }))
                .await
            {
                if previous.request.identity.connection_revision
                    != task.request.identity.connection_revision
                    || !matches!(
                        previous.request.operation,
                        RuntimeOperation::FilesWrite { .. }
                            | RuntimeOperation::FilesEdit { .. }
                            | RuntimeOperation::TransfersUpload { .. }
                    )
                    || !previous
                        .request
                        .operation
                        .business_targets()
                        .ok()?
                        .iter()
                        .any(|p| p.path == snapshot.path && p.effect == FileEffect::Write)
                {
                    continue;
                }
                let report = previous.file_report?;
                let hash = report.hash.or_else(|| {
                    report
                        .json
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v["hash"].as_str().map(str::to_owned))
                });
                return Some(ScriptOrigin {
                    task_id: previous.id,
                    session_id: previous.context_id,
                    tool: previous.request.operation.label().into(),
                    content_matches: hash.as_deref() == Some(snapshot.hash.as_str()),
                });
            }
            let body: serde_json::Value = serde_json::from_str(&row.raw_body).ok()?;
            let tool = body["tool"].as_str().unwrap_or("");
            if body["connectionRevision"].as_u64()
                != Some(task.request.identity.connection_revision)
                || ![
                    "files_write.write",
                    "files_write.edit",
                    "files_transfer.upload",
                ]
                .contains(&tool)
                || !body["targets"].as_array().is_some_and(|targets| {
                    targets.iter().any(|p| {
                        p["path"].as_str() == Some(snapshot.path.as_str()) && p["effect"] == "write"
                    })
                })
            {
                continue;
            }
            let session = row.display.context_id?;
            let mut origin = ScriptOrigin {
                task_id: row.id.clone(),
                session_id: session.clone(),
                tool: tool.into(),
                content_matches: false,
            };
            let mut cursor = row.display.output_end.unwrap_or(0).saturating_sub(256);
            let mut text = String::new();
            for _ in 0..3 {
                let Ok(DbReply::Agent(AgentDbReply::Activity(page))) = self
                    .database
                    .execute(DbCommand::Agent(AgentDbCommand::recording(
                        RecordingCommand::Read {
                            session_id: session.clone(),
                            task_id: Some(row.id.clone()),
                            cursor,
                            include_output: true,
                        },
                    )))
                    .await
                else {
                    return Some(origin);
                };
                if page.incomplete {
                    return Some(origin);
                }
                for record in page.records {
                    match record.body {
                        ActivityBody::Transfer {
                            hash: Some(hash), ..
                        } if tool == "files_transfer.upload" => {
                            origin.content_matches = hash == snapshot.hash
                        }
                        ActivityBody::Output {
                            stream,
                            text: chunk,
                            ..
                        } if stream == "stdout" => {
                            if text.len() + chunk.len() > 65536 {
                                return Some(origin);
                            }
                            text.push_str(&chunk);
                        }
                        _ => {}
                    }
                }
                if page.next_cursor <= cursor {
                    break;
                }
                cursor = page.next_cursor;
            }
            let complete = body
                .pointer("/operation/kind")
                .is_some_and(|v| v == "files_edit")
                || body
                    .pointer("/operation/kind")
                    .is_some_and(|v| v == "files_write")
                    && body
                        .pointer("/operation/mode")
                        .is_some_and(|v| v == "create" || v == "replace");
            if complete {
                origin.content_matches = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|v| v["hash"].as_str().map(str::to_owned))
                    .as_deref()
                    == Some(snapshot.hash.as_str());
            }
            return Some(origin);
        }
        None
    }
}
