use std::io;
use std::io::prelude::*;
use std::string::String;
use std::fs::File;

use serde::Serialize;
use serde::de::DeserializeOwned;

use serde_json;
use atomicwrites;

use common::canonical_serialize::CanonicalSerialize;

use crate::state::{FunderMutation, FunderState};
use crate::database::atomic_db::AtomicDb;

#[derive(Debug)]
pub enum FileDbError {
    ReadError(io::Error),
    WriteError(atomicwrites::Error<io::Error>),
    DeserializeError(serde_json::error::Error),
    SerializeError(serde_json::error::Error),
}

pub struct FileDbConn {
    /// DbCore file path
    db_path: String,
}

#[allow(unused)]
impl FileDbConn {
    fn new(db_path: &str) -> FileDbConn {
        FileDbConn {
            db_path: String::from(db_path),
        }
    }
}

#[allow(unused)]
pub struct FileDb<A: Clone,P,RS,FS,MS> {
    /// Connection to the database
    db_conn: FileDbConn,
    /// Current FunderState represented by the database
    funder_state: FunderState<A,P,RS,FS,MS>,
}


#[allow(unused)]
impl<A,P,RS,FS,MS> FileDb<A,P,RS,FS,MS> 
where
    A: Clone + Serialize + DeserializeOwned + 'static,
{

    pub fn new(db_conn: FileDbConn) -> Result<Self, FileDbError> {

        // TODO: Should create a new funder_state here if does not exist?
        let mut fr = File::open(&db_conn.db_path)
            .map_err(FileDbError::ReadError)?;

        let mut serialized_str = String::new();
        fr.read_to_string(&mut serialized_str)
            .map_err(FileDbError::ReadError)?;

        let funder_state: FunderState<A> = serde_json::from_str(&serialized_str)
            .map_err(FileDbError::DeserializeError)?;

        Ok(FileDb {
            db_conn,
            funder_state,
        })
    }
}


#[allow(unused)]
impl<A,P,RS,FS,MS> AtomicDb for FileDb<A,P,RS,FS,MS> 
where
    A: CanonicalSerialize + Clone + Serialize + DeserializeOwned + 'static,
{
    type State = FunderState<A,P,RS,FS,MS>;
    type Mutation = FunderMutation<A,P,RS,FS,MS>;
    type Error = FileDbError;

    /// Get current FunderState represented by the database
    fn get_state(&self) -> &Self::State {
        &self.funder_state
    }

    /// Apply a set of mutations atomically the database, and save it.
    fn mutate(&mut self, funder_mutations: Vec<FunderMutation<A,P,RS,FS,MS>>) -> Result<(), FileDbError> {
        // Apply mutation to funder_state:
        for funder_mutation in funder_mutations {
            self.funder_state.mutate(&funder_mutation);
        }

        // Serialize the funder_state:
        let serialized_str = serde_json::to_string(&self.funder_state)
            .map_err(FileDbError::SerializeError)?;

        // Save the new funder_state to file, atomically:
        let af = atomicwrites::AtomicFile::new(
            &self.db_conn.db_path, atomicwrites::AllowOverwrite);
        af.write(|fw| {
            fw.write_all(serialized_str.as_bytes())
        }).map_err(FileDbError::WriteError)?;
        
        Ok(())
    }
}
