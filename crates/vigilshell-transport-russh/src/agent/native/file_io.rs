use super::*;
impl Run<'_> {
    pub(super) async fn file(
        &mut self,
        operation: RuntimeOperation,
        business: bool,
    ) -> AgentResult<Value> {
        self.check()?;
        let permit = native_file_operation(&self.operation, operation, &self.root)?;
        let report = files::execute(
            self.connection,
            permit,
            FileTransferHandles::default(),
            self.control.clone(),
        )
        .await
        .map_err(|_| "native_result_unknown")?;
        let json = self
            .accept_file_report(report, business)?
            .ok_or("native_result_unknown")?;
        serde_json::from_str(&json).map_err(|_| "native_result_unknown")
    }
    pub(super) async fn mkdir(&mut self, path: &str) -> AgentResult<()> {
        self.file(
            RuntimeOperation::FilesMkdir {
                target: scoped(path),
                parents: true,
            },
            false,
        )
        .await?;
        Ok(())
    }
    pub(super) async fn read(&mut self, path: &str) -> AgentResult<Vec<u8>> {
        let before = self
            .file(
                RuntimeOperation::FilesStat {
                    target: scoped(path),
                    hash: true,
                },
                false,
            )
            .await?;
        let expected = before["hash"]
            .as_str()
            .ok_or("native_result_unknown")?
            .to_owned();
        let mut bytes = vec![];
        let mut offset = 0;
        loop {
            let value = self
                .file(
                    RuntimeOperation::FilesRead {
                        target: scoped(path),
                        offset,
                        length: 65536,
                    },
                    false,
                )
                .await?;
            let chunk = STANDARD
                .decode(value["data"].as_str().ok_or("native_result_unknown")?)
                .map_err(|_| "native_result_unknown")?;
            offset += chunk.len() as u64;
            bytes.extend(chunk);
            if bytes.len() > 1024 * 1024 {
                return Err("native_file_limit");
            }
            if value["eof"] == true {
                break;
            }
            if value["nextOffset"].as_u64() != Some(offset) {
                return Err("native_result_unknown");
            }
        }
        if hash(&bytes) != expected {
            return Err(self.file_error("file_changed"));
        }
        let after = self
            .file(
                RuntimeOperation::FilesStat {
                    target: scoped(path),
                    hash: true,
                },
                false,
            )
            .await?;
        if after["hash"].as_str() != Some(expected.as_str()) {
            return Err(self.file_error("file_changed"));
        }
        Ok(bytes)
    }
    pub(super) async fn put(
        &mut self,
        path: &str,
        bytes: &[u8],
        expected: Option<String>,
        create: bool,
        business: bool,
    ) -> AgentResult<()> {
        let operation = RuntimeOperation::FilesWrite {
            target: scoped(path),
            data: String::new(),
            mode: if create {
                FileWriteMode::Create
            } else {
                FileWriteMode::Replace
            },
            expected_hash: expected,
        };
        let permit = native_file_operation(&self.operation, operation, &self.root)?;
        let report = files::write_maintenance(
            self.connection,
            permit,
            bytes.to_vec(),
            self.control.clone(),
        )
        .await
        .map_err(|_| "native_result_unknown")?;
        self.accept_file_report(report, business)?;
        Ok(())
    }
    fn file_error(&self, code: &str) -> &'static str {
        match code {
            "file_changed" if self.operation.operation().is_package_operation() => {
                "package_source_changed"
            }
            "file_changed" if self.operation.operation().is_service_operation() => {
                "service_definition_changed"
            }
            "file_changed" => "file_changed",
            "already_exists" if self.operation.operation().is_package_operation() => {
                "package_source_exists"
            }
            "already_exists" => "native_file_exists",
            "file_denied" => "native_account_permission_denied",
            "cancelled" => "native_cancelled",
            "file_not_found" => "native_file_not_found",
            "file_capability_unavailable" => "native_file_capability_unavailable",
            _ => "native_file_operation_failed",
        }
    }
    fn accept_file_report(
        &mut self,
        report: FileExecutionReport,
        business: bool,
    ) -> AgentResult<Option<String>> {
        self.report.recovery_paths.extend(report.recovery_paths);
        self.report.recovery_paths.sort();
        self.report.recovery_paths.dedup();
        if report.commit == FileCommitState::Unknown {
            self.report.effects = FileCommitState::Unknown;
        } else if business
            && matches!(
                report.commit,
                FileCommitState::Committed | FileCommitState::Partial
            )
        {
            self.report.effects = FileCommitState::Partial;
        }
        if let Some(code) = report.error {
            return Err(self.file_error(&code));
        }
        Ok(report.json)
    }
}
