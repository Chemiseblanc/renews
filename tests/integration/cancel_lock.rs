use base64::{Engine as _, engine::general_purpose::STANDARD};
use renews::parse_message;
use sha2::{Digest, Sha256};

use crate::utils::{self, ClientMock};

#[tokio::test]
async fn cancel_key_allows_cancel() {
    let (storage, auth) = utils::setup().await;
    storage.add_group("misc.test", false).await.unwrap();

    let key = "secret";
    let key_b64 = STANDARD.encode(key);
    let lock_hash = Sha256::digest(key_b64.as_bytes());
    let lock_b64 = STANDARD.encode(lock_hash);
    let orig = format!(
        "Message-ID: <a@test>\r\nNewsgroups: misc.test\r\nCancel-Lock: sha256:{}\r\n\r\nBody",
        lock_b64
    );
    let (_, msg) = parse_message(&orig).unwrap();
    storage.store_article("misc.test", &msg).await.unwrap();

    let cancel = format!(
        "Message-ID: <c@test>\r\nNewsgroups: misc.test\r\nControl: cancel <a@test>\r\nCancel-Key: sha256:{}\r\n\r\n.\r\n",
        key_b64
    );
    ClientMock::new()
        .expect("IHAVE <c@test>", "335 Send it; end with <CR-LF>.<CR-LF>")
        .expect(
            cancel.trim_end_matches("\r\n"),
            "235 Article transferred OK",
        )
        .run(storage.clone(), auth)
        .await;
    assert!(
        storage
            .get_article_by_id("<a@test>")
            .await
            .unwrap()
            .is_none()
    );
}
