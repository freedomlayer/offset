use std::io;
use std::io::prelude::*;
use std::string::String;
use std::fs::File;

use serde::Serialize;
use serde::de::DeserializeOwned;

use serde_json;
use atomicwrites;

use funder::state::{FunderMutation, FunderState};

pub enum DbCoreError {
    ReadError(io::Error),
    WriteError(atomicwrites::Error<io::Error>),
    DeserializeError(serde_json::error::Error),
    SerializeError(serde_json::error::Error),
}

pub struct DbConn {
    /// DbCore file path
    db_path: String,
}

#[allow(unused)]
impl DbConn {
    fn new(db_path: &str) -> DbConn {
        DbConn {
            db_path: String::from(db_path),
        }
    }
}

#[allow(unused)]
pub struct DbCore<A: Clone> {
    /// Connection to the database
    db_conn: DbConn,
    /// Current FunderState represented by the database
    funder_state: FunderState<A>,
}


#[allow(unused)]
impl<A: Clone + Serialize + DeserializeOwned> DbCore<A> {
    pub fn new(db_conn: DbConn) -> Result<DbCore<A>, DbCoreError> {

        // TODO: Should create a new funder_state here if does not exist?
        let mut fr = File::open(&db_conn.db_path)
            .map_err(DbCoreError::ReadError)?;

        let mut serialized_str = String::new();
        fr.read_to_string(&mut serialized_str)
            .map_err(DbCoreError::ReadError)?;

        let funder_state: FunderState<A> = serde_json::from_str(&serialized_str)
            .map_err(DbCoreError::DeserializeError)?;

        Ok(DbCore {
            db_conn,
            funder_state,
        })
    }

    /// Get current FunderState represented by the database
    pub fn state(&self) -> &FunderState<A> {
        &self.funder_state
    }

    /// Apply a mutation over the database, and save it.
    pub fn mutate(&mut self, funder_mutations: Vec<FunderMutation<A>>) -> Result<(), DbCoreError> {
        // Apply mutation to funder_state:
        for funder_mutation in funder_mutations {
            self.funder_state.mutate(&funder_mutation);
        }

        // Serialize the funder_state:
        let serialized_str = serde_json::to_string(&self.funder_state)
            .map_err(DbCoreError::SerializeError)?;

        // Save the new funder_state to file, atomically:
        let af = atomicwrites::AtomicFile::new(
            &self.db_conn.db_path, atomicwrites::AllowOverwrite);
        af.write(|fw| {
            fw.write_all(serialized_str.as_bytes())
        }).map_err(DbCoreError::WriteError)?;
        
        Ok(())
    }
}
