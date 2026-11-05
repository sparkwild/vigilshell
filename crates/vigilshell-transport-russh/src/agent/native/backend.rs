use super::*;
impl Run<'_> {
    pub(super) async fn packages(&mut self) -> AgentResult<Value> {
        if matches!(
            self.operation.operation(),
            RuntimeOperation::PackagesList {}
        ) {
            let backend = self.programs.selected().ok().map(|(kind, _)| kind);
            let (name, args) = if self.programs.path("dpkg-query").is_ok() {
                (
                    "dpkg-query",
                    vec![
                        "-W".into(),
                        "-f=${binary:Package}\t${Version}\t${Status}\n".into(),
                    ],
                )
            } else {
                (
                    "rpm",
                    vec![
                        "-qa".into(),
                        "--qf".into(),
                        "%{NAME}\t%{VERSION}-%{RELEASE}\n".into(),
                    ],
                )
            };
            let result = self.exec(name, &args, false).await?;
            if result.code != 0 {
                return Err("native_package_query_failed");
            }
            return Ok(
                json!({"backend":backend,"queryProgram":name,"packageManagers":self.programs.managers,"packages":result.text.lines().collect::<Vec<_>>()}),
            );
        }
        let (kind, program) = self.programs.selected()?;
        let program = program.to_owned();
        let (targets, action) = match self.operation.operation() {
            RuntimeOperation::PackagesInstall { packages } => (packages.clone(), "install"),
            RuntimeOperation::PackagesUpdate { packages } => (packages.clone(), "upgrade"),
            RuntimeOperation::PackagesRemove { packages } => (packages.clone(), "remove"),
            _ => return Err("package_operation_required"),
        };
        let mut operands = Vec::new();
        let mut requested = Vec::new();
        for (index, target) in targets.iter().enumerate() {
            match target {
                PackageTarget::Repository { name, version } => {
                    requested.push((name.clone(), version.clone()));
                    operands.push(match version {
                        Some(v) => format!(
                            "{name}{}{v}",
                            if kind == PackageManagerKind::Apt {
                                "="
                            } else {
                                "-"
                            }
                        ),
                        None if kind == PackageManagerKind::Apt => {
                            format!("{name}{}", if action == "remove" { "-" } else { "+" })
                        }
                        None => name.clone(),
                    });
                }
                PackageTarget::Local { path, sha256 } => {
                    let extension = if kind == PackageManagerKind::Apt {
                        "deb"
                    } else {
                        "rpm"
                    };
                    if !path.ends_with(&format!(".{extension}")) {
                        return Err("package_backend_mismatch");
                    }
                    let root = self.root.clone();
                    self.mkdir(&root).await?;
                    let destination = format!("{root}/package-{index}.{extension}");
                    self.file(
                        RuntimeOperation::FilesStat {
                            target: scoped(path),
                            hash: true,
                        },
                        false,
                    )
                    .await?;
                    let copied = self
                        .exec(
                            "cp",
                            &[
                                "--no-clobber".into(),
                                "--no-dereference".into(),
                                "--".into(),
                                path.clone(),
                                destination.clone(),
                            ],
                            false,
                        )
                        .await?;
                    if copied.code != 0 {
                        return Err("local_package_unavailable");
                    }
                    let value = self
                        .file(
                            RuntimeOperation::FilesStat {
                                target: scoped(&destination),
                                hash: true,
                            },
                            false,
                        )
                        .await?;
                    if value["hash"].as_str() != Some(sha256.as_str()) {
                        return Err("local_package_changed");
                    }
                    let (name, args) = if kind == PackageManagerKind::Apt {
                        (
                            "dpkg-deb",
                            vec![
                                "--field".into(),
                                destination.clone(),
                                "Package".into(),
                                "Version".into(),
                            ],
                        )
                    } else {
                        (
                            "rpm",
                            vec![
                                "-qp".into(),
                                "--qf".into(),
                                "%{NAME}\n%{EPOCHNUM}:%{VERSION}-%{RELEASE}".into(),
                                "--".into(),
                                destination.clone(),
                            ],
                        )
                    };
                    let metadata = self.exec(name, &args, false).await?;
                    if metadata.code != 0 {
                        return Err("invalid_local_package");
                    }
                    let lines: Vec<_> = metadata.text.lines().collect();
                    let name = lines
                        .first()
                        .copied()
                        .unwrap_or("")
                        .strip_prefix("Package: ")
                        .unwrap_or(lines.first().copied().unwrap_or(""));
                    let version = lines
                        .get(1)
                        .copied()
                        .unwrap_or("")
                        .strip_prefix("Version: ")
                        .unwrap_or(lines.get(1).copied().unwrap_or(""));
                    if !valid_package_name(name) || version.is_empty() {
                        return Err("invalid_local_package");
                    }
                    requested.push((name.into(), Some(version.into())));
                    operands.push(destination);
                }
            }
        }
        let mut args: Vec<String> = if kind == PackageManagerKind::Apt {
            vec![
                "-y",
                "-o",
                "Dpkg::Options::=--force-confold",
                "-o",
                "APT::Get::AutomaticRemove=false",
            ]
        } else {
            vec![
                "-y",
                "--setopt=clean_requirements_on_remove=False",
                "--setopt=localpkg_gpgcheck=True",
                "--setopt=gpgcheck=True",
            ]
        }
        .into_iter()
        .map(str::to_owned)
        .collect();
        if kind != PackageManagerKind::Apt {
            let repositories = if kind == PackageManagerKind::Dnf5 {
                let r = self
                    .exec(
                        &program,
                        &[
                            "repo".into(),
                            "list".into(),
                            "--all".into(),
                            "--json".into(),
                        ],
                        false,
                    )
                    .await?;
                if r.code != 0 {
                    return Err("package_sources_unavailable");
                }
                !serde_json::from_str::<Value>(&r.text)
                    .map_err(|_| "package_sources_unavailable")?
                    .as_array()
                    .ok_or("package_sources_unavailable")?
                    .is_empty()
            } else {
                true
            };
            if repositories {
                args.push("--setopt=*.gpgkey=".into());
            }
        }
        if kind == PackageManagerKind::Apt && action == "upgrade" {
            args.extend(["--only-upgrade".into(), "install".into()]);
        } else {
            args.push(action.into());
        }
        args.push("--".into());
        args.extend(operands);
        if kind == PackageManagerKind::Apt
            && action != "remove"
            && targets
                .iter()
                .any(|t| matches!(t, PackageTarget::Repository { .. }))
        {
            self.report.effects = FileCommitState::Partial;
            let update = self
                .exec(
                    "apt-get",
                    &[
                        "-o".into(),
                        "APT::Update::Error-Mode=any".into(),
                        "update".into(),
                    ],
                    true,
                )
                .await?;
            if update.code != 0 {
                return Err("native_repository_refresh_failed");
            }
        }
        self.check()?;
        self.report.effects = FileCommitState::Partial;
        let operation = self.exec(&program, &args, true).await?;
        let mut observed = Vec::new();
        for (name, version) in requested {
            let (query, args) = if kind == PackageManagerKind::Apt {
                (
                    "dpkg-query",
                    vec![
                        "-W".into(),
                        "-f=${Version}\t${db:Status-Status}".into(),
                        "--".into(),
                        name.clone(),
                    ],
                )
            } else {
                (
                    "rpm",
                    vec![
                        "-q".into(),
                        "--qf".into(),
                        "%{EPOCHNUM}:%{VERSION}-%{RELEASE}".into(),
                        "--".into(),
                        name.clone(),
                    ],
                )
            };
            let result = self
                .exec(query, &args, false)
                .await
                .map_err(|_| "native_package_result_unknown")?;
            if result.code > 1 {
                return Err("native_package_result_unknown");
            }
            let installed = result.code == 0
                && (kind != PackageManagerKind::Apt
                    || result.text.trim_end().ends_with("\tinstalled"));
            let actual = if kind == PackageManagerKind::Apt {
                result.text.split('\t').next().unwrap_or("")
            } else {
                result.text.trim()
            };
            observed.push(json!({"name":name,"installed":installed,"state":result.text.trim(),"queryExitCode":result.code}));
            if operation.code == 0
                && (installed != (action != "remove")
                    || (action != "remove"
                        && version.as_ref().is_some_and(|v| {
                            if kind == PackageManagerKind::Apt || v.contains(':') {
                                v != actual
                            } else {
                                actual.strip_prefix("0:").unwrap_or(actual) != v
                            }
                        })))
            {
                return Err("native_package_result_unknown");
            }
        }
        let value = json!({"nativePackageResult":{"backend":kind,"exitCode":operation.code,"action":action,"packages":observed}});
        self.report.json = Some(value.to_string());
        if operation.code != 0 {
            return Err("native_package_operation_failed");
        }
        Ok(value)
    }
}
