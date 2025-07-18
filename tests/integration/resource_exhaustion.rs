//! Tests for resource exhaustion and queue failure modes

use crate::utils::{ClientMock, create_malformed_article, setup};
use renews::{
    Message,
    queue::{ArticleQueue, QueuedArticle},
};
use smallvec::smallvec;

#[tokio::test]
async fn test_queue_capacity_exhaustion() {
    // Create a queue with very small capacity and no workers to process articles
    let queue = ArticleQueue::new(2);

    // Create test articles
    let article1 = QueuedArticle {
        message: Message {
            headers: smallvec![
                ("From".to_string(), "test1@example.com".to_string()),
                ("Subject".to_string(), "Test 1".to_string()),
                ("Message-ID".to_string(), "<test1@example.com>".to_string()),
            ],
            body: "Test body 1".to_string(),
        },
        size: 100,
        is_control: false,
        already_validated: false,
    };

    let article2 = QueuedArticle {
        message: Message {
            headers: smallvec![
                ("From".to_string(), "test2@example.com".to_string()),
                ("Subject".to_string(), "Test 2".to_string()),
                ("Message-ID".to_string(), "<test2@example.com>".to_string()),
            ],
            body: "Test body 2".to_string(),
        },
        size: 100,
        is_control: false,
        already_validated: false,
    };

    let article3 = QueuedArticle {
        message: Message {
            headers: smallvec![
                ("From".to_string(), "test3@example.com".to_string()),
                ("Subject".to_string(), "Test 3".to_string()),
                ("Message-ID".to_string(), "<test3@example.com>".to_string()),
            ],
            body: "Test body 3".to_string(),
        },
        size: 100,
        is_control: false,
        already_validated: false,
    };

    // Fill the queue to capacity
    assert!(queue.submit(article1).await.is_ok());
    assert!(queue.submit(article2).await.is_ok());

    // This should fail due to capacity
    let result = tokio::time::timeout(
        tokio::time::Duration::from_millis(100),
        queue.submit(article3),
    )
    .await;

    // Should either timeout (hanging on full queue) or return error
    assert!(result.is_err() || result.unwrap().is_err());
}

#[tokio::test]
async fn test_empty_queue_operations() {
    let queue = ArticleQueue::new(10);

    // Get the receiver for the queue
    let receiver = queue.receiver();

    // Try to receive from empty queue (should not block, should fail immediately)
    let result = receiver.try_recv();

    // Should return error since queue is empty
    assert!(result.is_err());
}

#[tokio::test]
async fn test_small_capacity_queue_exhaustion() {
    let queue = ArticleQueue::new(1); // Very small capacity

    let article1 = QueuedArticle {
        message: Message {
            headers: smallvec![
                ("From".to_string(), "test1@example.com".to_string()),
                ("Subject".to_string(), "Test 1".to_string()),
                ("Message-ID".to_string(), "<test1@example.com>".to_string()),
            ],
            body: "Test body 1".to_string(),
        },
        size: 100,
        is_control: false,
        already_validated: false,
    };

    let article2 = QueuedArticle {
        message: Message {
            headers: smallvec![
                ("From".to_string(), "test2@example.com".to_string()),
                ("Subject".to_string(), "Test 2".to_string()),
                ("Message-ID".to_string(), "<test2@example.com>".to_string()),
            ],
            body: "Test body 2".to_string(),
        },
        size: 100,
        is_control: false,
        already_validated: false,
    };

    // First article should succeed
    assert!(queue.submit(article1).await.is_ok());

    // Second article should fail due to capacity exhaustion
    // Note: This test might pass if the queue is processed quickly,
    // but in a real scenario with workers, this would typically fail
    let result = tokio::time::timeout(
        tokio::time::Duration::from_millis(100),
        queue.submit(article2),
    )
    .await;

    // Should either timeout or fail immediately
    assert!(result.is_err() || result.unwrap().is_err());
}

#[tokio::test]
async fn test_large_article_handling() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    // Create a very large article (10KB) to test server's handling of large content
    let large_body = "x".repeat(10000);
    let large_article = format!(
        "From: test@example.com\r\nSubject: Large Article\r\nNewsgroups: test.group\r\nMessage-ID: <large@example.com>\r\n\r\n{large_body}\r\n.\r\n"
    );

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 password required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            vec![large_article],
            vec!["240 article received"], // Server accepts large articles without default size limits
        )
        .run_tls(storage, auth)
        .await;
}

#[tokio::test]
async fn test_malformed_article_submission() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    // Create an article with malformed headers (missing required headers)
    let malformed_article = create_malformed_article("missing_from");

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 password required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            vec![malformed_article],
            vec!["441 posting failed"], // Should fail due to malformed headers
        )
        .run_tls(storage, auth)
        .await;
}

#[tokio::test]
async fn test_missing_required_headers() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    // Article missing From header
    let article_no_from = "Subject: Test\r\nNewsgroups: test.group\r\n\r\nBody\r\n.\r\n";

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 password required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            vec![article_no_from.to_string()],
            vec!["441 posting failed"],
        )
        .run_tls(storage, auth)
        .await;
}

#[tokio::test]
async fn test_concurrent_queue_access() {
    let queue = ArticleQueue::new(100);

    // Create multiple concurrent submission tasks
    let mut handles = Vec::new();

    for i in 0..10 {
        let queue_clone = queue.clone();
        let handle = tokio::spawn(async move {
            let article = QueuedArticle {
                message: Message {
                    headers: smallvec![
                        ("From".to_string(), format!("test{i}@example.com")),
                        ("Subject".to_string(), format!("Test {i}")),
                        ("Message-ID".to_string(), format!("<test{i}@example.com>")),
                    ],
                    body: format!("Test body {i}"),
                },
                size: 100,
                is_control: false,
                already_validated: false,
            };

            queue_clone.submit(article).await
        });
        handles.push(handle);
    }

    // Wait for all submissions to complete
    let results = futures_util::future::join_all(handles).await;

    // All should succeed since queue capacity is sufficient
    for result in results {
        assert!(result.unwrap().is_ok());
    }
}

#[tokio::test]
async fn test_extremely_long_headers() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    // Create an article with extremely long header values
    let long_subject = "x".repeat(10000);
    let article_long_header = format!(
        "From: test@example.com\r\nSubject: {long_subject}\r\nNewsgroups: test.group\r\n\r\nBody\r\n.\r\n"
    );

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 password required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            vec![article_long_header],
            vec!["240 article received"], // Might succeed or fail depending on implementation
        )
        .run_tls(storage, auth)
        .await;
}

#[tokio::test]
async fn test_null_bytes_in_article() {
    let (storage, auth) = setup().await;
    storage.add_group("test.group", false).await.unwrap();
    auth.add_user("testuser", "password").await.unwrap();

    // Create an article with null bytes (binary content)
    let article_with_nulls = "From: test@example.com\r\nSubject: Test\r\nNewsgroups: test.group\r\n\r\nBody with \0 null byte\r\n.\r\n";

    ClientMock::new()
        .expect("AUTHINFO USER testuser", "381 password required")
        .expect("AUTHINFO PASS password", "281 authentication accepted")
        .expect(
            "POST",
            "340 send article to be posted. End with <CR-LF>.<CR-LF>",
        )
        .expect_request_multi(
            vec![article_with_nulls.to_string()],
            vec!["240 article received"], // Behavior depends on implementation
        )
        .run_tls(storage, auth)
        .await;
}
