use super::*;
impl WorkspaceService {
    pub(in crate::workspace) async fn directory_action(
        self: &Arc<Self>,
        owner: &str,
        action: ToolAction,
    ) -> Result<WorkspaceResult, WorkspaceError> {
        let reply = match action {
            ToolAction::Identities {
                session,
                uids,
                gids,
            } => {
                if uids.len() + gids.len() > 64 {
                    return Err(WorkspaceError::InvalidInput);
                }
                let group = self.tool_group(owner, &session)?;
                ToolResult::Identities(
                    self.tools_client(&group)
                        .await?
                        .identities(uids, gids)
                        .await
                        .unwrap_or_default(),
                )
            }
            ToolAction::CancelList { request } | ToolAction::ReleaseList { request } => {
                let mut state = self.state.lock().unwrap();
                if state
                    .listings
                    .get(&request)
                    .is_some_and(|l| l.owner != owner)
                {
                    return Err(WorkspaceError::NotFound);
                }
                if let Some(listing) = state.listings.remove(&request) {
                    listing.cancel.store(true, Ordering::Release);
                }
                ToolResult::Done
            }
            ToolAction::List {
                session,
                path,
                cursor,
                request,
            } => {
                let group = self.tool_group(owner, &session)?;
                valid_remote_path(&path)?;
                if !valid_id(&request) {
                    return Err(WorkspaceError::InvalidInput);
                }
                if let Some(cursor) = cursor {
                    let offset = cursor
                        .parse::<usize>()
                        .map_err(|_| WorkspaceError::InvalidInput)?;
                    let mut state = self.state.lock().unwrap();
                    let listing = state
                        .listings
                        .get_mut(&request)
                        .filter(|l| {
                            l.owner == owner
                                && l.session == session
                                && !l.cancel.load(Ordering::Acquire)
                        })
                        .ok_or(WorkspaceError::NotFound)?;
                    if listing.touched.elapsed() >= Duration::from_secs(60) {
                        return Err(WorkspaceError::NotFound);
                    }
                    let entries = listing.entries.as_ref().ok_or(WorkspaceError::Busy)?;
                    let page = directory_page(&listing.path, entries, offset)?;
                    listing.touched = Instant::now();
                    ToolResult::Directory(page)
                } else {
                    let cancel = Arc::new(AtomicBool::new(false));
                    let permit = {
                        let mut state = self.state.lock().unwrap();
                        state.listings.retain(|_, l| {
                            l.entries.is_none() || l.touched.elapsed() < Duration::from_secs(60)
                        });
                        if state.listings.contains_key(&request) {
                            return Err(WorkspaceError::Busy);
                        }
                        if state.listings.values().filter(|l| l.owner == owner).count() >= 8 {
                            return Err(WorkspaceError::Busy);
                        }
                        let gate = state
                            .listing_reads
                            .entry(owner.into())
                            .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(4)))
                            .clone();
                        let permit = gate.try_acquire_owned().map_err(|_| WorkspaceError::Busy)?;
                        state.listings.insert(
                            request.clone(),
                            Listing {
                                owner: owner.into(),
                                session,
                                path: path.clone(),
                                cancel: cancel.clone(),
                                entries: None,
                                touched: Instant::now(),
                            },
                        );
                        permit
                    };
                    let result = async {
                        self.tools_client(&group)
                            .await?
                            .list(path, cancel.clone())
                            .await
                    }
                    .await;
                    drop(permit);
                    let mut state = self.state.lock().unwrap();
                    let valid = state
                        .listings
                        .get(&request)
                        .is_some_and(|l| Arc::ptr_eq(&l.cancel, &cancel));
                    if !valid || cancel.load(Ordering::Acquire) {
                        return Err(WorkspaceError::Cancelled);
                    }
                    match result.and_then(|(path, entries)| {
                        Ok((directory_page(&path, &entries, 0)?, path, entries))
                    }) {
                        Ok((page, path, entries)) => {
                            let listing = state.listings.get_mut(&request).unwrap();
                            listing.path = path;
                            listing.entries = Some(entries);
                            listing.touched = Instant::now();
                            ToolResult::Directory(page)
                        }
                        Err(error) => {
                            state.listings.remove(&request);
                            return Err(error);
                        }
                    }
                }
            }
            ToolAction::CreateDirectory { session, path } => {
                let group = self.tool_group(owner, &session)?;
                self.tools_client(&group).await?.mkdir(path).await?;
                ToolResult::Done
            }
            ToolAction::Rename { session, from, to } => {
                let group = self.tool_group(owner, &session)?;
                self.tools_client(&group).await?.rename(from, to).await?;
                ToolResult::Done
            }
            ToolAction::Remove {
                session,
                path,
                directory,
            } => {
                let group = self.tool_group(owner, &session)?;
                self.tools_client(&group)
                    .await?
                    .remove(path, directory)
                    .await?;
                ToolResult::Done
            }
            _ => return Err(WorkspaceError::InvalidInput),
        };
        Ok(WorkspaceResult::Tools(reply))
    }
}
