use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::Utc;
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use uuid::Uuid;

use crate::{
    Block, Blockchain, MerkleTree, Operation, Transaction,
    consensus::Validator,
    error::{MiniChainError, Result},
};

use super::{
    ChainMetadata, RecordIndex, RecordStatus, RecordVerification, Snapshot, StorageStats,
    StoredTransaction,
    models::{STORAGE_SCHEMA_VERSION, SnapshotPayload},
};

const BLOCKS: TableDefinition<u64, &[u8]> = TableDefinition::new("blocks");
const BLOCK_HASHES: TableDefinition<&str, u64> = TableDefinition::new("block_hashes");
const TRANSACTIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("transactions");
const RECORDS: TableDefinition<&str, &[u8]> = TableDefinition::new("records");
const METADATA: TableDefinition<&str, &[u8]> = TableDefinition::new("metadata");
const SNAPSHOTS: TableDefinition<&str, &[u8]> = TableDefinition::new("snapshots");
const VALIDATORS: TableDefinition<&str, &[u8]> = TableDefinition::new("validators");
const CHAIN_METADATA_KEY: &str = "chain";

pub trait Storage {
    fn commit_block(&self, block: Block) -> Result<()>;
    fn get_block(&self, height: u64) -> Result<Block>;
    fn get_block_by_hash(&self, hash: &str) -> Result<Block>;
    fn get_blocks(&self, start: u64, end: u64) -> Result<Vec<Block>>;
    fn get_transaction(&self, id: Uuid) -> Result<StoredTransaction>;
    fn get_record(&self, id: &str) -> Result<RecordIndex>;
    fn metadata(&self) -> Result<ChainMetadata>;
    fn recover(&self) -> Result<Blockchain>;
}

#[derive(Clone)]
pub struct RedbStorage {
    database: Arc<Database>,
    path: PathBuf,
    chain_id: Uuid,
    network_id: String,
}

impl RedbStorage {
    pub fn open(
        path: impl AsRef<Path>,
        chain_id: Uuid,
        network_id: impl Into<String>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let database = Database::create(&path).map_err(|_| MiniChainError::StorageUnavailable)?;
        let storage = Self {
            database: Arc::new(database),
            path,
            chain_id,
            network_id: network_id.into(),
        };
        storage.create_tables()?;
        storage.initialize_if_empty()?;
        storage.recover()?;
        Ok(storage)
    }

    pub fn latest_block(&self) -> Result<Block> {
        let metadata = self.metadata()?;
        self.get_block(metadata.current_height)
    }

    pub fn stats(&self) -> Result<StorageStats> {
        let read = self.read_transaction()?;
        let blocks = read
            .open_table(BLOCKS)
            .map_err(storage_unavailable)?
            .len()
            .map_err(storage_unavailable)?;
        let transactions = read
            .open_table(TRANSACTIONS)
            .map_err(storage_unavailable)?
            .len()
            .map_err(storage_unavailable)?;
        let records = read
            .open_table(RECORDS)
            .map_err(storage_unavailable)?
            .len()
            .map_err(storage_unavailable)?;
        let metadata = self.metadata()?;
        Ok(StorageStats {
            blocks,
            transactions,
            records,
            current_height: metadata.current_height,
            latest_block_hash: metadata.latest_block_hash,
            database_bytes: fs::metadata(&self.path).ok().map(|value| value.len()),
        })
    }

    pub fn save_validator(&self, validator: &Validator) -> Result<()> {
        validator.validate()?;
        let bytes = serialize(validator)?;
        let write = self.database.begin_write().map_err(storage_unavailable)?;
        {
            write
                .open_table(VALIDATORS)
                .map_err(storage_unavailable)?
                .insert(validator.id.as_str(), bytes.as_slice())
                .map_err(storage_unavailable)?;
        }
        write.commit().map_err(storage_unavailable)
    }

    pub fn get_validator(&self, id: &str) -> Result<Validator> {
        let read = self.read_transaction()?;
        let table = read.open_table(VALIDATORS).map_err(storage_unavailable)?;
        let value = table
            .get(id)
            .map_err(storage_unavailable)?
            .ok_or_else(|| MiniChainError::UnknownValidator { id: id.to_owned() })?;
        let validator: Validator = deserialize(value.value())?;
        validator.validate()?;
        Ok(validator)
    }

    pub fn validators(&self) -> Result<Vec<Validator>> {
        let read = self.read_transaction()?;
        let table = read.open_table(VALIDATORS).map_err(storage_unavailable)?;
        table
            .iter()
            .map_err(storage_unavailable)?
            .map(|entry| {
                let (_, value) = entry.map_err(storage_unavailable)?;
                let validator: Validator = deserialize(value.value())?;
                validator.validate()?;
                Ok(validator)
            })
            .collect()
    }

    pub fn transactions(&self) -> Result<Vec<StoredTransaction>> {
        let read = self.read_transaction()?;
        let table = read.open_table(TRANSACTIONS).map_err(storage_unavailable)?;
        let mut transactions = table
            .iter()
            .map_err(storage_unavailable)?
            .map(|entry| {
                let (_, value) = entry.map_err(storage_unavailable)?;
                deserialize::<StoredTransaction>(value.value())
            })
            .collect::<Result<Vec<_>>>()?;
        transactions.sort_by_key(|stored| (stored.block_height, stored.transaction_index));
        Ok(transactions)
    }

    pub fn record_history(&self, id: &str) -> Result<Vec<StoredTransaction>> {
        self.get_record(id)?;
        Ok(self
            .transactions()?
            .into_iter()
            .filter(|stored| stored.transaction.record_id == id)
            .collect())
    }

    pub fn rebuild_indexes(&self) -> Result<()> {
        let (chain, metadata) = self.load_authoritative_chain()?;
        let transactions = transaction_entries(&chain)?;
        let records = record_entries(&chain)?;
        let write = self.database.begin_write().map_err(storage_unavailable)?;
        write
            .delete_table(TRANSACTIONS)
            .map_err(storage_unavailable)?;
        write.delete_table(RECORDS).map_err(storage_unavailable)?;
        {
            let mut table = write
                .open_table(TRANSACTIONS)
                .map_err(storage_unavailable)?;
            for (id, bytes) in &transactions {
                table
                    .insert(id.as_str(), bytes.as_slice())
                    .map_err(storage_unavailable)?;
            }
        }
        {
            let mut table = write.open_table(RECORDS).map_err(storage_unavailable)?;
            for (id, bytes) in &records {
                table
                    .insert(id.as_str(), bytes.as_slice())
                    .map_err(storage_unavailable)?;
            }
        }
        write.commit().map_err(storage_unavailable)?;
        self.verify_indexes(&chain)?;
        if self.metadata()? != metadata {
            return Err(MiniChainError::MetadataMismatch);
        }
        Ok(())
    }

    pub fn create_snapshot(&self) -> Result<Snapshot> {
        let chain = self.recover()?;
        let metadata = self.metadata()?;
        let payload = SnapshotPayload {
            version: STORAGE_SCHEMA_VERSION,
            chain_id: metadata.chain_id,
            height: metadata.current_height,
            latest_block_hash: metadata.latest_block_hash,
            genesis_hash: metadata.genesis_hash,
            created_at: Utc::now(),
            blocks: chain.blocks().to_vec(),
        };
        let snapshot = Snapshot::new(payload).map_err(corrupt_serialization)?;
        let bytes = serialize(&snapshot)?;
        let write = self.database.begin_write().map_err(storage_unavailable)?;
        {
            let mut table = write.open_table(SNAPSHOTS).map_err(storage_unavailable)?;
            table
                .insert(snapshot.id.to_string().as_str(), bytes.as_slice())
                .map_err(storage_unavailable)?;
        }
        write.commit().map_err(storage_unavailable)?;
        Ok(snapshot)
    }

    pub fn get_snapshot(&self, id: Uuid) -> Result<Snapshot> {
        let read = self.read_transaction()?;
        let table = read.open_table(SNAPSHOTS).map_err(storage_unavailable)?;
        let key = id.to_string();
        let value = table
            .get(key.as_str())
            .map_err(storage_unavailable)?
            .ok_or(MiniChainError::SnapshotInvalid { id })?;
        deserialize(value.value())
    }

    pub fn snapshots(&self) -> Result<Vec<Snapshot>> {
        let read = self.read_transaction()?;
        let table = read.open_table(SNAPSHOTS).map_err(storage_unavailable)?;
        let mut snapshots = table
            .iter()
            .map_err(storage_unavailable)?
            .map(|entry| {
                let (_, value) = entry.map_err(storage_unavailable)?;
                deserialize(value.value())
            })
            .collect::<Result<Vec<_>>>()?;
        snapshots.sort_by_key(Snapshot::created_at);
        Ok(snapshots)
    }

    pub fn verify_record(&self, id: &str) -> Result<RecordVerification> {
        let chain = self.recover()?;
        let indexed = self.get_record(id)?;
        let mut latest = None;
        let mut expected = None;
        for block in chain.blocks() {
            for (transaction_index, transaction) in block.transactions.iter().enumerate() {
                if transaction.record_id != id {
                    continue;
                }
                match transaction.operation {
                    Operation::CreateRecord | Operation::UpdateRecord => {
                        expected = Some(RecordIndex {
                            record_id: id.to_owned(),
                            data: transaction.payload.clone(),
                            status: RecordStatus::Active,
                            latest_transaction_id: transaction.id,
                            block_height: block.header.index,
                        });
                    }
                    Operation::RevokeRecord => {
                        if let Some(record) = expected.as_mut() {
                            record.status = RecordStatus::Revoked;
                            record.latest_transaction_id = transaction.id;
                            record.block_height = block.header.index;
                        }
                    }
                    Operation::VerifyRecord | Operation::AuditEvent => continue,
                }
                latest = Some(StoredTransaction {
                    transaction: transaction.clone(),
                    block_height: block.header.index,
                    block_hash: block.hash.clone(),
                    transaction_index,
                });
            }
        }
        let expected =
            expected.ok_or_else(|| MiniChainError::RecordNotFound { id: id.to_owned() })?;
        let latest = latest.ok_or_else(|| MiniChainError::RecordNotFound { id: id.to_owned() })?;
        if indexed != expected {
            return Err(MiniChainError::IndexCorrupted { index: "records" });
        }
        let block = chain
            .block_at(latest.block_height)
            .ok_or(MiniChainError::BlockNotFound {
                height: latest.block_height,
            })?;
        let hashes = block
            .transactions
            .iter()
            .map(|transaction| transaction.hash.clone())
            .collect::<Vec<_>>();
        let merkle_proof_valid = MerkleTree::from_hashes(&hashes)
            .and_then(|tree| tree.proof(&latest.transaction.hash))
            .is_ok_and(|proof| proof.verify(&block.header.merkle_root));
        Ok(RecordVerification {
            record: indexed,
            latest_transaction: latest,
            cryptographically_verified: merkle_proof_valid,
            merkle_proof_valid,
            chain_valid: true,
        })
    }

    pub fn verify_snapshot(&self, id: Uuid) -> Result<Snapshot> {
        let snapshot = self.get_snapshot(id)?;
        if !snapshot.verify_integrity() || snapshot.payload.version != STORAGE_SCHEMA_VERSION {
            return Err(MiniChainError::SnapshotInvalid { id });
        }
        let metadata = self.metadata()?;
        if snapshot.payload.chain_id != metadata.chain_id
            || snapshot.payload.genesis_hash != metadata.genesis_hash
        {
            return Err(MiniChainError::SnapshotIncompatible);
        }
        let chain = Blockchain::from_blocks(snapshot.payload.blocks.clone())
            .map_err(|_| MiniChainError::SnapshotInvalid { id })?;
        if chain.height() != snapshot.payload.height
            || chain.tip().hash != snapshot.payload.latest_block_hash
        {
            return Err(MiniChainError::SnapshotInvalid { id });
        }
        Ok(snapshot)
    }

    pub fn restore_snapshot(&self, id: Uuid, force: bool) -> Result<Blockchain> {
        let snapshot = self.verify_snapshot(id)?;
        if self.metadata()?.current_height > 0 && !force {
            return Err(MiniChainError::RestoreWouldOverwrite);
        }
        self.replace_chain(&snapshot.payload.blocks)?;
        self.recover()
    }

    fn record_updates_for_block(&self, block: &Block) -> Result<BTreeMap<String, RecordIndex>> {
        let mut updates = BTreeMap::new();
        for transaction in &block.transactions {
            match transaction.operation {
                Operation::CreateRecord | Operation::UpdateRecord => {
                    updates.insert(
                        transaction.record_id.clone(),
                        RecordIndex {
                            record_id: transaction.record_id.clone(),
                            data: transaction.payload.clone(),
                            status: RecordStatus::Active,
                            latest_transaction_id: transaction.id,
                            block_height: block.header.index,
                        },
                    );
                }
                Operation::RevokeRecord => {
                    if !updates.contains_key(&transaction.record_id) {
                        match self.get_record(&transaction.record_id) {
                            Ok(record) => {
                                updates.insert(transaction.record_id.clone(), record);
                            }
                            Err(MiniChainError::RecordNotFound { .. }) => continue,
                            Err(error) => return Err(error),
                        }
                    }
                    let record = updates
                        .get_mut(&transaction.record_id)
                        .expect("the record was inserted above");
                    record.status = RecordStatus::Revoked;
                    record.latest_transaction_id = transaction.id;
                    record.block_height = block.header.index;
                }
                Operation::VerifyRecord | Operation::AuditEvent => {}
            }
        }
        Ok(updates)
    }

    fn create_tables(&self) -> Result<()> {
        let write = self.database.begin_write().map_err(storage_unavailable)?;
        {
            write.open_table(BLOCKS).map_err(storage_unavailable)?;
        }
        {
            write
                .open_table(BLOCK_HASHES)
                .map_err(storage_unavailable)?;
        }
        {
            write
                .open_table(TRANSACTIONS)
                .map_err(storage_unavailable)?;
        }
        {
            write.open_table(RECORDS).map_err(storage_unavailable)?;
        }
        {
            write.open_table(METADATA).map_err(storage_unavailable)?;
        }
        {
            write.open_table(SNAPSHOTS).map_err(storage_unavailable)?;
        }
        {
            write.open_table(VALIDATORS).map_err(storage_unavailable)?;
        }
        write.commit().map_err(storage_unavailable)
    }

    fn initialize_if_empty(&self) -> Result<()> {
        let read = self.read_transaction()?;
        let metadata_missing = read
            .open_table(METADATA)
            .map_err(storage_unavailable)?
            .get(CHAIN_METADATA_KEY)
            .map_err(storage_unavailable)?
            .is_none();
        let blocks_empty = read
            .open_table(BLOCKS)
            .map_err(storage_unavailable)?
            .is_empty()
            .map_err(storage_unavailable)?;
        drop(read);
        match (metadata_missing, blocks_empty) {
            (false, _) => Ok(()),
            (true, false) => Err(MiniChainError::StorageCorruption {
                reason: "metadata is missing".to_owned(),
            }),
            (true, true) => self.replace_chain(Blockchain::new()?.blocks()),
        }
    }

    fn load_authoritative_chain(&self) -> Result<(Blockchain, ChainMetadata)> {
        let metadata = self.metadata()?;
        if metadata.schema_version != STORAGE_SCHEMA_VERSION
            || metadata.chain_id != self.chain_id
            || metadata.network_id != self.network_id
        {
            return Err(MiniChainError::MetadataMismatch);
        }
        let mut blocks = Vec::with_capacity(metadata.current_height as usize + 1);
        for height in 0..=metadata.current_height {
            blocks.push(self.get_block(height)?);
        }
        let read = self.read_transaction()?;
        let block_count = read
            .open_table(BLOCKS)
            .map_err(storage_unavailable)?
            .len()
            .map_err(storage_unavailable)?;
        let hash_count = read
            .open_table(BLOCK_HASHES)
            .map_err(storage_unavailable)?
            .len()
            .map_err(storage_unavailable)?;
        if block_count != metadata.current_height + 1 || hash_count != metadata.current_height + 1 {
            return Err(MiniChainError::IndexCorrupted { index: "blocks" });
        }
        drop(read);
        let chain =
            Blockchain::from_blocks(blocks).map_err(|error| MiniChainError::StorageCorruption {
                reason: error.to_string(),
            })?;
        if chain.blocks()[0].hash != metadata.genesis_hash
            || chain.tip().hash != metadata.latest_block_hash
            || chain.height() != metadata.current_height
        {
            return Err(MiniChainError::MetadataMismatch);
        }
        for block in chain.blocks() {
            if self.get_block_by_hash(&block.hash)? != *block {
                return Err(MiniChainError::IndexCorrupted {
                    index: "block_hashes",
                });
            }
        }
        self.validators()?;
        Ok((chain, metadata))
    }

    fn verify_indexes(&self, chain: &Blockchain) -> Result<()> {
        let expected_transactions = transaction_models(chain);
        let expected_records = record_models(chain);
        let read = self.read_transaction()?;
        let tx_table = read.open_table(TRANSACTIONS).map_err(storage_unavailable)?;
        if tx_table.len().map_err(storage_unavailable)? != expected_transactions.len() as u64 {
            return Err(MiniChainError::IndexCorrupted {
                index: "transactions",
            });
        }
        for (id, expected) in expected_transactions {
            let value = tx_table
                .get(id.as_str())
                .map_err(storage_unavailable)?
                .ok_or(MiniChainError::IndexCorrupted {
                    index: "transactions",
                })?;
            if deserialize::<StoredTransaction>(value.value())? != expected {
                return Err(MiniChainError::IndexCorrupted {
                    index: "transactions",
                });
            }
        }
        let record_table = read.open_table(RECORDS).map_err(storage_unavailable)?;
        if record_table.len().map_err(storage_unavailable)? != expected_records.len() as u64 {
            return Err(MiniChainError::IndexCorrupted { index: "records" });
        }
        for (id, expected) in expected_records {
            let value = record_table
                .get(id.as_str())
                .map_err(storage_unavailable)?
                .ok_or(MiniChainError::IndexCorrupted { index: "records" })?;
            if deserialize::<RecordIndex>(value.value())? != expected {
                return Err(MiniChainError::IndexCorrupted { index: "records" });
            }
        }
        Ok(())
    }

    fn replace_chain(&self, blocks: &[Block]) -> Result<()> {
        let chain = Blockchain::from_blocks(blocks.to_vec()).map_err(|error| {
            MiniChainError::StorageCorruption {
                reason: error.to_string(),
            }
        })?;
        let metadata = ChainMetadata {
            schema_version: STORAGE_SCHEMA_VERSION,
            chain_id: self.chain_id,
            network_id: self.network_id.clone(),
            current_height: chain.height(),
            latest_block_hash: chain.tip().hash.clone(),
            genesis_hash: chain.blocks()[0].hash.clone(),
        };
        let block_entries = block_entries(&chain)?;
        let transactions = transaction_entries(&chain)?;
        let records = record_entries(&chain)?;
        let metadata_bytes = serialize(&metadata)?;
        let write = self.database.begin_write().map_err(storage_unavailable)?;
        write.delete_table(BLOCKS).map_err(storage_unavailable)?;
        write
            .delete_table(BLOCK_HASHES)
            .map_err(storage_unavailable)?;
        write
            .delete_table(TRANSACTIONS)
            .map_err(storage_unavailable)?;
        write.delete_table(RECORDS).map_err(storage_unavailable)?;
        write.delete_table(METADATA).map_err(storage_unavailable)?;
        {
            let mut table = write.open_table(BLOCKS).map_err(storage_unavailable)?;
            for (height, bytes) in &block_entries {
                table
                    .insert(height, bytes.as_slice())
                    .map_err(storage_unavailable)?;
            }
        }
        {
            let mut table = write
                .open_table(BLOCK_HASHES)
                .map_err(storage_unavailable)?;
            for block in chain.blocks() {
                table
                    .insert(block.hash.as_str(), &block.header.index)
                    .map_err(storage_unavailable)?;
            }
        }
        {
            let mut table = write
                .open_table(TRANSACTIONS)
                .map_err(storage_unavailable)?;
            for (id, bytes) in &transactions {
                table
                    .insert(id.as_str(), bytes.as_slice())
                    .map_err(storage_unavailable)?;
            }
        }
        {
            let mut table = write.open_table(RECORDS).map_err(storage_unavailable)?;
            for (id, bytes) in &records {
                table
                    .insert(id.as_str(), bytes.as_slice())
                    .map_err(storage_unavailable)?;
            }
        }
        {
            write
                .open_table(METADATA)
                .map_err(storage_unavailable)?
                .insert(CHAIN_METADATA_KEY, metadata_bytes.as_slice())
                .map_err(storage_unavailable)?;
        }
        write.commit().map_err(storage_unavailable)
    }

    fn read_transaction(&self) -> Result<redb::ReadTransaction> {
        self.database.begin_read().map_err(storage_unavailable)
    }
}

impl Storage for RedbStorage {
    fn commit_block(&self, block: Block) -> Result<()> {
        let metadata = self.metadata()?;
        let previous = self.get_block(metadata.current_height)?;
        Blockchain::validate_successor(&previous, &block)?;
        for transaction in &block.transactions {
            match self.get_transaction(transaction.id) {
                Ok(_) => {
                    return Err(MiniChainError::DuplicateTransaction { id: transaction.id });
                }
                Err(MiniChainError::TransactionNotFound { .. }) => {}
                Err(error) => return Err(error),
            }
        }

        let block_bytes = serialize(&block)?;
        let transaction_entries = block
            .transactions
            .iter()
            .enumerate()
            .map(|(index, transaction)| {
                let stored = StoredTransaction {
                    transaction: transaction.clone(),
                    block_height: block.header.index,
                    block_hash: block.hash.clone(),
                    transaction_index: index,
                };
                Ok((transaction.id.to_string(), serialize(&stored)?))
            })
            .collect::<Result<Vec<_>>>()?;
        let record_updates = self.record_updates_for_block(&block)?;
        let next_metadata = ChainMetadata {
            current_height: block.header.index,
            latest_block_hash: block.hash.clone(),
            ..metadata
        };
        let metadata_bytes = serialize(&next_metadata)?;

        let write = self.database.begin_write().map_err(storage_unavailable)?;
        {
            write
                .open_table(BLOCKS)
                .map_err(storage_unavailable)?
                .insert(&block.header.index, block_bytes.as_slice())
                .map_err(storage_unavailable)?;
        }
        {
            write
                .open_table(BLOCK_HASHES)
                .map_err(storage_unavailable)?
                .insert(block.hash.as_str(), &block.header.index)
                .map_err(storage_unavailable)?;
        }
        {
            let mut table = write
                .open_table(TRANSACTIONS)
                .map_err(storage_unavailable)?;
            for (id, bytes) in &transaction_entries {
                table
                    .insert(id.as_str(), bytes.as_slice())
                    .map_err(storage_unavailable)?;
            }
        }
        {
            let mut table = write.open_table(RECORDS).map_err(storage_unavailable)?;
            for (id, record) in &record_updates {
                let bytes = serialize(record)?;
                table
                    .insert(id.as_str(), bytes.as_slice())
                    .map_err(storage_unavailable)?;
            }
        }
        {
            write
                .open_table(METADATA)
                .map_err(storage_unavailable)?
                .insert(CHAIN_METADATA_KEY, metadata_bytes.as_slice())
                .map_err(storage_unavailable)?;
        }
        write.commit().map_err(storage_unavailable)
    }

    fn get_block(&self, height: u64) -> Result<Block> {
        let read = self.read_transaction()?;
        let table = read.open_table(BLOCKS).map_err(storage_unavailable)?;
        let value = table
            .get(height)
            .map_err(storage_unavailable)?
            .ok_or(MiniChainError::BlockNotFound { height })?;
        deserialize(value.value())
    }

    fn get_block_by_hash(&self, hash: &str) -> Result<Block> {
        let read = self.read_transaction()?;
        let table = read.open_table(BLOCK_HASHES).map_err(storage_unavailable)?;
        let height = table
            .get(hash)
            .map_err(storage_unavailable)?
            .ok_or_else(|| MiniChainError::BlockHashNotFound {
                hash: hash.to_owned(),
            })?
            .value();
        drop(table);
        drop(read);
        self.get_block(height)
    }

    fn get_blocks(&self, start: u64, end: u64) -> Result<Vec<Block>> {
        if start > end {
            return Ok(Vec::new());
        }
        (start..=end).map(|height| self.get_block(height)).collect()
    }

    fn get_transaction(&self, id: Uuid) -> Result<StoredTransaction> {
        let read = self.read_transaction()?;
        let table = read.open_table(TRANSACTIONS).map_err(storage_unavailable)?;
        let key = id.to_string();
        let value = table
            .get(key.as_str())
            .map_err(storage_unavailable)?
            .ok_or(MiniChainError::TransactionNotFound { id })?;
        deserialize(value.value())
    }

    fn get_record(&self, id: &str) -> Result<RecordIndex> {
        let read = self.read_transaction()?;
        let table = read.open_table(RECORDS).map_err(storage_unavailable)?;
        let value = table
            .get(id)
            .map_err(storage_unavailable)?
            .ok_or_else(|| MiniChainError::RecordNotFound { id: id.to_owned() })?;
        deserialize(value.value())
    }

    fn metadata(&self) -> Result<ChainMetadata> {
        let read = self.read_transaction()?;
        let table = read.open_table(METADATA).map_err(storage_unavailable)?;
        let value = table
            .get(CHAIN_METADATA_KEY)
            .map_err(storage_unavailable)?
            .ok_or(MiniChainError::MetadataMismatch)?;
        deserialize(value.value())
    }

    fn recover(&self) -> Result<Blockchain> {
        let (chain, _) = self.load_authoritative_chain()?;
        self.verify_indexes(&chain)?;
        Ok(chain)
    }
}

fn transaction_models(chain: &Blockchain) -> BTreeMap<String, StoredTransaction> {
    let mut entries = BTreeMap::new();
    for block in chain.blocks() {
        for (index, transaction) in block.transactions.iter().enumerate() {
            entries.insert(
                transaction.id.to_string(),
                StoredTransaction {
                    transaction: transaction.clone(),
                    block_height: block.header.index,
                    block_hash: block.hash.clone(),
                    transaction_index: index,
                },
            );
        }
    }
    entries
}

fn record_models(chain: &Blockchain) -> BTreeMap<String, RecordIndex> {
    let mut records = BTreeMap::new();
    for block in chain.blocks() {
        for transaction in &block.transactions {
            apply_record(&mut records, transaction, block.header.index);
        }
    }
    records
}

fn apply_record(
    records: &mut BTreeMap<String, RecordIndex>,
    transaction: &Transaction,
    height: u64,
) {
    match transaction.operation {
        Operation::CreateRecord | Operation::UpdateRecord => {
            records.insert(
                transaction.record_id.clone(),
                RecordIndex {
                    record_id: transaction.record_id.clone(),
                    data: transaction.payload.clone(),
                    status: RecordStatus::Active,
                    latest_transaction_id: transaction.id,
                    block_height: height,
                },
            );
        }
        Operation::RevokeRecord => {
            if let Some(record) = records.get_mut(&transaction.record_id) {
                record.status = RecordStatus::Revoked;
                record.latest_transaction_id = transaction.id;
                record.block_height = height;
            }
        }
        Operation::VerifyRecord | Operation::AuditEvent => {}
    }
}

fn block_entries(chain: &Blockchain) -> Result<Vec<(u64, Vec<u8>)>> {
    chain
        .blocks()
        .iter()
        .map(|block| Ok((block.header.index, serialize(block)?)))
        .collect()
}
fn transaction_entries(chain: &Blockchain) -> Result<Vec<(String, Vec<u8>)>> {
    transaction_models(chain)
        .into_iter()
        .map(|(id, value)| Ok((id, serialize(&value)?)))
        .collect()
}
fn record_entries(chain: &Blockchain) -> Result<Vec<(String, Vec<u8>)>> {
    record_models(chain)
        .into_iter()
        .map(|(id, value)| Ok((id, serialize(&value)?)))
        .collect()
}
fn serialize<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(corrupt_serialization)
}
fn deserialize<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(corrupt_serialization)
}
fn corrupt_serialization(error: serde_json::Error) -> MiniChainError {
    MiniChainError::StorageCorruption {
        reason: error.to_string(),
    }
}
fn storage_unavailable<T>(_error: T) -> MiniChainError {
    MiniChainError::StorageUnavailable
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::{ValidatorIdentity, storage::Snapshot};

    fn populated_storage() -> (TempDir, RedbStorage, Transaction) {
        let directory = TempDir::new().unwrap();
        let storage = RedbStorage::open(
            directory.path().join("chain.redb"),
            Uuid::new_v4(),
            "corruption-test",
        )
        .unwrap();
        let transaction = Transaction::new(
            Operation::CreateRecord,
            "CERT-CORRUPT",
            json!({"course": "MCA"}),
            BTreeMap::new(),
            &ValidatorIdentity::from_secret_bytes("validator-1", [41; 32]),
        );
        let chain = storage.recover().unwrap();
        storage
            .commit_block(
                Block::new(
                    1,
                    chain.tip().hash.clone(),
                    vec![transaction.clone()],
                    "validator-1",
                )
                .unwrap(),
            )
            .unwrap();
        (directory, storage, transaction)
    }

    #[test]
    fn corrupted_block_fails_recovery() {
        let (_directory, storage, _) = populated_storage();
        let write = storage.database.begin_write().unwrap();
        write
            .open_table(BLOCKS)
            .unwrap()
            .insert(&1, b"not-json".as_slice())
            .unwrap();
        write.commit().unwrap();
        assert!(matches!(
            storage.recover(),
            Err(MiniChainError::StorageCorruption { .. })
        ));
    }

    #[test]
    fn missing_block_fails_recovery() {
        let (_directory, storage, _) = populated_storage();
        let write = storage.database.begin_write().unwrap();
        write.open_table(BLOCKS).unwrap().remove(&1).unwrap();
        write.commit().unwrap();
        assert_eq!(
            storage.recover().unwrap_err(),
            MiniChainError::BlockNotFound { height: 1 }
        );
    }

    #[test]
    fn metadata_mismatch_fails_recovery() {
        let (_directory, storage, _) = populated_storage();
        let mut metadata = storage.metadata().unwrap();
        metadata.latest_block_hash = "f".repeat(64);
        let bytes = serialize(&metadata).unwrap();
        let write = storage.database.begin_write().unwrap();
        write
            .open_table(METADATA)
            .unwrap()
            .insert(CHAIN_METADATA_KEY, bytes.as_slice())
            .unwrap();
        write.commit().unwrap();
        assert_eq!(
            storage.recover().unwrap_err(),
            MiniChainError::MetadataMismatch
        );
    }

    #[test]
    fn broken_derived_index_is_detected_and_can_be_rebuilt() {
        let (_directory, storage, transaction) = populated_storage();
        let key = transaction.id.to_string();
        let write = storage.database.begin_write().unwrap();
        write
            .open_table(TRANSACTIONS)
            .unwrap()
            .remove(key.as_str())
            .unwrap();
        write.commit().unwrap();
        assert_eq!(
            storage.recover().unwrap_err(),
            MiniChainError::IndexCorrupted {
                index: "transactions"
            }
        );
        storage.rebuild_indexes().unwrap();
        assert_eq!(storage.recover().unwrap().height(), 1);
    }

    #[test]
    fn modified_and_incompatible_snapshots_are_rejected() {
        let (_directory, storage, _) = populated_storage();
        let snapshot = storage.create_snapshot().unwrap();
        let mut modified = snapshot.clone();
        modified.integrity_hash = "0".repeat(64);
        overwrite_snapshot(&storage, &modified);
        assert_eq!(
            storage.verify_snapshot(snapshot.id).unwrap_err(),
            MiniChainError::SnapshotInvalid { id: snapshot.id }
        );

        let mut payload = snapshot.payload;
        payload.chain_id = Uuid::new_v4();
        let mut incompatible = Snapshot::new(payload).unwrap();
        incompatible.id = snapshot.id;
        overwrite_snapshot(&storage, &incompatible);
        assert_eq!(
            storage.verify_snapshot(snapshot.id).unwrap_err(),
            MiniChainError::SnapshotIncompatible
        );
    }

    fn overwrite_snapshot(storage: &RedbStorage, snapshot: &Snapshot) {
        let bytes = serialize(snapshot).unwrap();
        let key = snapshot.id.to_string();
        let write = storage.database.begin_write().unwrap();
        write
            .open_table(SNAPSHOTS)
            .unwrap()
            .insert(key.as_str(), bytes.as_slice())
            .unwrap();
        write.commit().unwrap();
    }
}
