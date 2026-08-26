use std::{path::PathBuf, sync::Arc};

use crate::{network::NodeConfig, storage::RedbStorage};

use super::error::{CliError, CliResult};

#[derive(Clone)]
pub struct CliContext {
    pub config_path: PathBuf,
    pub json: bool,
}

impl CliContext {
    pub fn config(&self) -> CliResult<NodeConfig> {
        NodeConfig::from_file(&self.config_path).map_err(CliError::from)
    }

    pub fn offline_storage(&self) -> CliResult<Arc<RedbStorage>> {
        let config = self.config()?;
        Ok(Arc::new(RedbStorage::open(
            &config.storage_path,
            config.chain_id,
            &config.network_id,
        )?))
    }
}
