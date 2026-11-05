use super::*;
fn timezone() -> RuntimeOperation {
    RuntimeOperation::SystemSettingsUpdate {
        setting: SystemSetting::Timezone,
        value: "UTC".into(),
    }
}
#[tokio::test]
async fn native_maintenance_keeps_protection_and_power_is_only_accepted() {
    let f = Fixture::new().await;
    let task = f.start(timezone(), "timezone").await;
    assert_eq!(f.finished(&task.id).await.phase, TaskPhase::Succeeded);
    assert!(f.service.policy(&f.connection_id).execution.protect_system);
    let power = f
        .start(
            RuntimeOperation::PowerControl {
                action: PowerAction::Reboot,
            },
            "power",
        )
        .await;
    let power = f.finished(&power.id).await;
    assert_eq!(power.phase, TaskPhase::RequestAccepted);
    assert_eq!(power.exit_code, None);
    f.service.shutdown().await;
}
#[tokio::test]
async fn native_mutations_share_one_connection_lock() {
    let f = Fixture::new().await;
    f.connection.hold.store(true, Ordering::Release);
    let one = f.start(timezone(), "one").await;
    f.running(&one.id).await;
    let two = f.start(timezone(), "two").await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        f.service.get_task(&two.id).await.unwrap().phase,
        TaskPhase::Queued
    );
    assert_eq!(f.connection.requests.lock().unwrap().len(), 1);
    f.service
        .cancel_task(&one.id, ActivityActor::User)
        .await
        .unwrap();
    f.finished(&one.id).await;
    f.running(&two.id).await;
    f.service
        .cancel_task(&two.id, ActivityActor::User)
        .await
        .unwrap();
    f.finished(&two.id).await;
    f.service.shutdown().await;
}
#[tokio::test]
async fn capabilities_keep_policy_environment_and_session_availability_separate() {
    let f = Fixture::new().await;
    f.service.state.lock().unwrap().contexts["session"]
        .view
        .lock()
        .unwrap()
        .support
        .as_mut()
        .unwrap()
        .supported_tools
        .retain(|tool| tool != "system_settings_manage.timezone");
    let AgentReply::Capabilities { tools, .. } = f
        .service
        .invoke(
            f.entry.clone(),
            AgentAction::CapabilitiesGet {
                session_id: "session".into(),
            },
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    let tool = tools
        .iter()
        .find(|t| t.tool == "system_settings_manage.timezone")
        .unwrap();
    assert!(tool.configured);
    assert!(!tool.supported && !tool.usable);
    f.service.shutdown().await;
}
