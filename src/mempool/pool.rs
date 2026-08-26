use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    Transaction,
    error::{MiniChainError, Result},
};

#[derive(Clone)]
pub struct Mempool {
    state: Arc<RwLock<HashMap<Uuid, Transaction>>>,
    capacity: usize,
    max_age: Duration,
}

impl Mempool {
    pub fn new(capacity: usize, max_age: Duration) -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::with_capacity(capacity))),
            capacity,
            max_age,
        }
    }

    pub async fn insert(&self, transaction: Transaction) -> Result<()> {
        self.insert_at(transaction, Utc::now()).await
    }

    pub async fn insert_at(&self, transaction: Transaction, now: DateTime<Utc>) -> Result<()> {
        transaction.validate()?;
        if is_expired(&transaction, now, self.max_age) {
            return Err(MiniChainError::ExpiredTransaction { id: transaction.id });
        }

        let mut transactions = self.state.write().await;
        if transactions.contains_key(&transaction.id) {
            return Err(MiniChainError::DuplicateMempoolTransaction { id: transaction.id });
        }
        if transactions.len() >= self.capacity {
            return Err(MiniChainError::MempoolFull {
                capacity: self.capacity,
            });
        }
        transactions.insert(transaction.id, transaction);
        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> Option<Transaction> {
        self.state.read().await.get(&id).cloned()
    }

    pub async fn len(&self) -> usize {
        self.state.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.state.read().await.is_empty()
    }

    pub async fn remove_committed(&self, ids: &[Uuid]) -> usize {
        let mut transactions = self.state.write().await;
        ids.iter()
            .filter(|id| transactions.remove(id).is_some())
            .count()
    }

    pub async fn remove_expired(&self, now: DateTime<Utc>) -> usize {
        let mut transactions = self.state.write().await;
        let previous_len = transactions.len();
        transactions.retain(|_, transaction| !is_expired(transaction, now, self.max_age));
        previous_len - transactions.len()
    }
}

fn is_expired(transaction: &Transaction, now: DateTime<Utc>, max_age: Duration) -> bool {
    let Ok(max_age) = chrono::Duration::from_std(max_age) else {
        return false;
    };
    transaction.timestamp < now - max_age
}
