#![allow(unused)]

use futures::sync::{oneshot, mpsc};
use futures::prelude::{async, await};

use serde::Serialize;
use serde::de::DeserializeOwned;

use funder::state::{FunderMutation};
use super::core::{DbCore, DbCoreError};


pub struct ApplyFunderMutation<A> {
    funder_mutation: FunderMutation<A>,
    /// A oneshot to respond that the mutation was applied and the new state was saved.
    resposne: oneshot::Sender<()>,
}

pub enum DbServiceError {
    DbCoreError(DbCoreError),
}

#[async]
pub fn db_service<A: Clone + Serialize + DeserializeOwned>(db_core: DbCore<A>, 
                incoming: mpsc::Receiver<ApplyFunderMutation<A>>) -> Result<!, DbServiceError> {
    unimplemented!();
}
