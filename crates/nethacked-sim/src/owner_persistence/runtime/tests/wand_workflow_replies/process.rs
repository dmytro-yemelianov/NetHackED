use super::*;
use std::{fs, process::Command};
const RECORD_LIMIT: usize = PAYLOAD;
const ROLE: &str = "NETRUST_I5D_ROLE";
const DIRECTORY: &str = "NETRUST_I5D_DIRECTORY";

#[test]
fn retained_reply_reconstructs_in_separate_process_with_independent_host_approval() {
    if let Ok(role) = std::env::var(ROLE) {
        let path = std::path::PathBuf::from(std::env::var_os(DIRECTORY).unwrap());
        let (w, id, cfg) = fixture();
        let cfg = cfg.with_wand_workflow_replies().unwrap();
        if role == "produce" {
            let bridge = import(&w, cfg.clone());
            let mut legacy = bridge.export_native().unwrap();
            let ticket = bridge.pause_wand(action(id)).unwrap();
            let approval = bridge.approve_live_wands().unwrap();
            bridge
                .attach_wand_extension("wand-cancel-extension")
                .unwrap();
            fs::write(path.join("owners.bin"), bridge.capture().unwrap()).unwrap();
            // This is explicitly host-controlled input, never a field of owners.bin.
            fs::write(
                path.join("host-approval.json"),
                approval.export_trusted_record(RECORD_LIMIT).unwrap(),
            )
            .unwrap();
            let wrong = import(&w, cfg);
            wrong.pause_wand(WandGameplayAction::Cancel).unwrap();
            fs::write(
                path.join("wrong-host-approval.json"),
                wrong
                    .approve_live_wands()
                    .unwrap()
                    .export_trusted_record(RECORD_LIMIT)
                    .unwrap(),
            )
            .unwrap();
            let reply = bridge.resume_wand_reply(&ticket).unwrap();
            assert!(reply.error.is_none());
            assert_eq!(
                reply.events,
                legacy.handle_zap_selected_wand(0, Some(id), Direction::East, 6)
            );
            assert_eq!(bytes(&bridge), legacy.to_save_json().unwrap());
            fs::write(
                path.join("expected-reply.json"),
                encode(&reply, RECORD_LIMIT).unwrap(),
            )
            .unwrap();
            fs::write(path.join("expected-native.json"), bytes(&bridge)).unwrap();
            let expected_rng: Vec<u64> = (0..4).map(|_| bridge.wand_rng().next_u64()).collect();
            let legacy_rng: Vec<u64> = (0..4).map(|_| legacy.rng.next_u64()).collect();
            assert_eq!(expected_rng, legacy_rng);
            fs::write(
                path.join("expected-rng.json"),
                encode(&expected_rng, RECORD_LIMIT).unwrap(),
            )
            .unwrap();
            fs::write(path.join("producer.pid"), std::process::id().to_string()).unwrap();
        } else {
            assert_eq!(role, "consume");
            assert_ne!(
                fs::read_to_string(path.join("producer.pid")).unwrap(),
                std::process::id().to_string()
            );
            let saved = fs::read(path.join("owners.bin")).unwrap();
            assert!(OwnerBridge::reconstruct(&saved, cfg.clone()).is_err());
            let wrong = wand_workflows::WandWorkflowApproval::approve_trusted_record(
                &fs::read(path.join("wrong-host-approval.json")).unwrap(),
                RECORD_LIMIT,
            )
            .unwrap();
            assert!(OwnerBridge::reconstruct(
                &saved,
                cfg.clone().with_wand_workflow_approval(wrong).unwrap()
            )
            .is_err());
            let host_bytes = fs::read(path.join("host-approval.json")).unwrap();
            assert!(
                wand_workflows::WandWorkflowApproval::approve_trusted_record(
                    &host_bytes,
                    host_bytes.len() - 1
                )
                .is_err()
            );
            let approval = wand_workflows::WandWorkflowApproval::approve_trusted_record(
                &host_bytes,
                RECORD_LIMIT,
            )
            .unwrap();
            assert_eq!(
                approval.export_trusted_record(RECORD_LIMIT).unwrap(),
                host_bytes
            );
            let bridge = OwnerBridge::reconstruct(
                &saved,
                cfg.with_wand_workflow_approval(approval).unwrap(),
            )
            .unwrap();
            let ticket = bridge.pending_wands().unwrap().remove(0);
            let reply = bridge.resume_wand_reply(&ticket).unwrap();
            assert_eq!(
                encode(&reply, RECORD_LIMIT).unwrap(),
                fs::read(path.join("expected-reply.json")).unwrap()
            );
            assert_eq!(
                bytes(&bridge),
                fs::read_to_string(path.join("expected-native.json")).unwrap()
            );
            let before = bytes(&bridge);
            assert!(bridge.resume_wand_reply(&ticket).is_err());
            assert_eq!(bytes(&bridge), before);
            let draws: Vec<u64> = (0..4).map(|_| bridge.wand_rng().next_u64()).collect();
            assert_eq!(
                encode(&draws, RECORD_LIMIT).unwrap(),
                fs::read(path.join("expected-rng.json")).unwrap()
            );
            let before = bytes(&bridge);
            let future = bridge.dispatch_wand(action(id)).unwrap();
            assert!(future.error.is_none());
            assert_eq!(
                future.events,
                vec![crate::GameEvent::LogMessage {
                    text: "Never mind.".into()
                }]
            );
            assert_eq!(bytes(&bridge), before);
        }
        return;
    }
    let path = std::env::temp_dir().join(format!("netrust-i5d-process-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    for role in ["produce", "consume"] {
        let output = Command::new(std::env::current_exe().unwrap()).args([
            "--exact",
            "owner_persistence::runtime::tests::wand_workflow_replies::process::retained_reply_reconstructs_in_separate_process_with_independent_host_approval",
            "--nocapture",
        ]).env(ROLE, role).env(DIRECTORY, &path).output().unwrap();
        fs::write(path.join(format!("{role}.stdout.log")), &output.stdout).unwrap();
        fs::write(path.join(format!("{role}.stderr.log")), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "I5d {role} failed; logs: {}",
            path.display()
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "child filter ran no test: {}",
            path.display()
        );
    }
    fs::remove_dir_all(path).unwrap();
}
