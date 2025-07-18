use renews::config::Config;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::utils::{self, ClientMock};

#[tokio::test]
async fn ihave_rejects_large_article() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let cfg: Arc<RwLock<Config>> = Arc::new(RwLock::new(
        toml::from_str(
            r#"
addr = ":119"
[[group_settings]]
pattern = "*"
max_article_bytes = 10
"#,
        )
        .unwrap(),
    ));
    let cfg_val = cfg.read().await.clone();
    ClientMock::new()
        .expect("IHAVE <1@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect(
            "Message-ID: <1@test>\r\nNewsgroups: misc.test\r\nFrom: a@test\r\nSubject: big\r\n\r\n0123456789A\r\n.",
            "437 article rejected",
        )
        .run_with_cfg(cfg_val, storage.clone(), auth)
        .await;
    assert!(
        storage
            .get_article_by_id("<1@test>")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn ihave_rejects_large_article_with_suffix() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();
    let cfg: Arc<RwLock<Config>> = Arc::new(RwLock::new(
        toml::from_str(
            r#"
addr = ":119"
[[group_settings]]
pattern = "*"
max_article_bytes = "1K"
"#,
        )
        .unwrap(),
    ));
    let cfg_val = cfg.read().await.clone();
    ClientMock::new()
        .expect("IHAVE <2@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect(
            &format!(
                "Message-ID: <2@test>\r\nNewsgroups: misc.test\r\nFrom: b@test\r\nSubject: big\r\n\r\n{}\r\n.",
                "A".repeat(1100)
            ),
            "437 article rejected",
        )
        .run_with_cfg(cfg_val, storage.clone(), auth)
        .await;
    assert!(
        storage
            .get_article_by_id("<2@test>")
            .await
            .unwrap()
            .is_none()
    );
}
