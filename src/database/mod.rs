use std::io;
use std::io::prelude::*;
use std::string::String;
use std::fs::File;

use serde::Serialize;
use serde::de::DeserializeOwned;

use serde_json;
use atomicwrites;

use funder::state::{FunderMutation, FunderState};

pub enum DatabaseError {
    ReadError(io::Error),
    WriteError(atomicwrites::Error<io::Error>),
    DeserializeError(serde_json::error::Error),
    SerializeError(serde_json::error::Error),
}

pub struct DbConn {
    /// Database file path
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
pub struct Database<A: Clone> {
    /// Connection to the database
    db_conn: DbConn,
    /// Current FunderState represented by the database
    funder_state: FunderState<A>,
}


#[allow(unused)]
impl<A: Clone + Serialize + DeserializeOwned> Database<A> {
    fn new(db_conn: DbConn) -> Result<Database<A>, DatabaseError> {

        let mut fr = File::open(&db_conn.db_path)
            .map_err(DatabaseError::ReadError)?;

        let mut serialized_str = String::new();
        fr.read_to_string(&mut serialized_str)
            .map_err(DatabaseError::ReadError)?;

        let funder_state: FunderState<A> = serde_json::from_str(&serialized_str)
            .map_err(DatabaseError::DeserializeError)?;

        Ok(Database {
            db_conn,
            funder_state,
        })
    }

    /// Get current FunderState represented by the database
    fn state(&self) -> &FunderState<A> {
        &self.funder_state
    }

    /// Apply a mutation over the database, and save it.
    fn mutate(&mut self, funder_mutation: FunderMutation<A>) -> Result<(), DatabaseError> {
        // Apply mutation to funder_state:
        self.funder_state.mutate(&funder_mutation);

        // Serialize the funder_state:
        let serialized_str = serde_json::to_string(&self.funder_state)
            .map_err(DatabaseError::SerializeError)?;

        // Save the new funder_state to file, atomically:
        let af = atomicwrites::AtomicFile::new(
            &self.db_conn.db_path, atomicwrites::AllowOverwrite);
        af.write(|fw| {
            fw.write_all(serialized_str.as_bytes())
        }).map_err(DatabaseError::WriteError)?;
        
        Ok(())
    }
}
