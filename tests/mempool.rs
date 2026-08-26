use std::{collections::BTreeMap, sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use minichain::{MiniChainError, Operation, Transaction, ValidatorIdentity, mempool::Mempool};
use serde_json::json;
use uuid::Uuid;

fn transaction(id: u128, timestamp: chrono::DateTime<Utc>) -> Transaction {
    Transaction::with_identity(
        Uuid::from_u128(id),
        timestamp,
        Operation::AuditEvent,
        format!("AUDIT-{id}"),
        json!({"event": "verified"}),
        BTreeMap::new(),
        &ValidatorIdentity::from_secret_bytes("auditor", [8; 32]),
    )
}

#[tokio::test]
async fn duplicate_and_capacity_limits_are_enforced() {
    let now = Utc.timestamp_opt(1_735_689_700, 0).single().unwrap();
    let pool = Mempool::new(1, Duration::from_secs(60));
    let first = transaction(1, now);
    pool.insert_at(first.clone(), now).await.unwrap();
    assert_eq!(
        pool.insert_at(first.clone(), now).await.unwrap_err(),
        MiniChainError::DuplicateMempoolTransaction { id: first.id }
    );
    assert_eq!(
        pool.insert_at(transaction(2, now), now).await.unwrap_err(),
        MiniChainError::MempoolFull { capacity: 1 }
    );
}

#[tokio::test]
async fn expired_transactions_are_rejected_and_removed() {
    let now = Utc.timestamp_opt(1_735_689_700, 0).single().unwrap();
    let pool = Mempool::new(10, Duration::from_secs(60));
    let old = transaction(1, now - chrono::Duration::seconds(61));
    assert_eq!(
        pool.insert_at(old.clone(), now).await.unwrap_err(),
        MiniChainError::ExpiredTransaction { id: old.id }
    );

    pool.insert_at(transaction(2, now), now).await.unwrap();
    assert_eq!(
        pool.remove_expired(now + chrono::Duration::seconds(61))
            .await,
        1
    );
    assert!(pool.is_empty().await);
}

#[tokio::test]
async fn committed_transactions_are_removed() {
    let now = Utc.timestamp_opt(1_735_689_700, 0).single().unwrap();
    let pool = Mempool::new(10, Duration::from_secs(60));
    let transaction = transaction(1, now);
    pool.insert_at(transaction.clone(), now).await.unwrap();
    assert_eq!(pool.remove_committed(&[transaction.id]).await, 1);
    assert!(pool.is_empty().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_insertion_keeps_all_unique_transactions() {
    let now = Utc::now();
    let pool = Arc::new(Mempool::new(100, Duration::from_secs(60)));
    let mut tasks = Vec::new();
    for id in 1..=50 {
        let pool = Arc::clone(&pool);
        tasks.push(tokio::spawn(async move {
            pool.insert_at(transaction(id, now), now).await
        }));
    }
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    assert_eq!(pool.len().await, 50);
}
