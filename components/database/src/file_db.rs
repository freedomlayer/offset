use std::io;
use std::io::prelude::*;
use std::string::String;
use std::fs::File;

use std::fmt::Debug;

use serde::Serialize;
use serde::de::DeserializeOwned;

use serde_json;
use atomicwrites;

use crate::atomic_db::{AtomicDb, MutableState};

#[derive(Debug)]
pub enum FileDbError<ME> {
    ReadError(io::Error),
    WriteError(atomicwrites::Error<io::Error>),
    DeserializeError(serde_json::error::Error),
    SerializeError(serde_json::error::Error),
    MutateError(ME),
}

pub struct FileDbConn {
    /// DbCore file path
    db_path: String,
}

impl FileDbConn {
    pub fn new(db_path: &str) -> FileDbConn {
        FileDbConn {
            db_path: String::from(db_path),
        }
    }
}

#[allow(unused)]
pub struct FileDb<S> {
    /// Connection to the database
    db_conn: FileDbConn,
    /// Current state represented by the database:
    state: S,
}


#[allow(unused)]
impl<S> FileDb<S>
where
    S: MutableState + Clone + Serialize + DeserializeOwned,
{
    pub fn new(db_conn: FileDbConn) -> Result<Self, FileDbError<S::MutateError>> {

        // Open file database, or create a new initial one:
        let mut fr = match File::open(&db_conn.db_path) {
            Ok(fr) => fr,
            Err(_) => {
                // There is no file, we create a new file:
                let initial_state = S::initial();

                // Serialize the state:
                let serialized_str = serde_json::to_string(&initial_state)
                    .map_err(FileDbError::SerializeError)?;
                // Save the new state to file, atomically:
                let af = atomicwrites::AtomicFile::new(
                    &db_conn.db_path, atomicwrites::AllowOverwrite);
                af.write(|fw| {
                    fw.write_all(serialized_str.as_bytes())
                }).map_err(FileDbError::WriteError)?;

                File::open(&db_conn.db_path)
                    .map_err(FileDbError::ReadError)?
            },
        };

        let mut serialized_str = String::new();
        fr.read_to_string(&mut serialized_str)
            .map_err(FileDbError::ReadError)?;

        let state: S = serde_json::from_str(&serialized_str)
            .map_err(FileDbError::DeserializeError)?;

        Ok(FileDb {
            db_conn,
            state,
        })
    }
}


#[allow(unused)]
impl<S> AtomicDb for FileDb<S> 
where 
    S: Clone + Serialize + DeserializeOwned + MutableState,
    S::Mutation: Clone + Serialize + DeserializeOwned,
    S::MutateError: Debug,
{
    type State = S;
    type Mutation = S::Mutation;
    type Error = FileDbError<S::MutateError>;

    /// Get current FunderState represented by the database
    fn get_state(&self) -> &Self::State {
        &self.state
    }

    /// Apply a set of mutations atomically the database, and save it.
    fn mutate(&mut self, mutations: &[Self::Mutation]) -> Result<(), Self::Error> {
        // Apply all mutations to state:
        for mutation in mutations.iter() {
            self.state.mutate(mutation)
                .map_err(|mutate_error| FileDbError::MutateError(mutate_error))?;
        }

        // Serialize the state:
        let serialized_str = serde_json::to_string(&self.state)
            .map_err(FileDbError::SerializeError)?;

        // Save the new state to file, atomically:
        let af = atomicwrites::AtomicFile::new(
            &self.db_conn.db_path, atomicwrites::AllowOverwrite);
        af.write(|fw| {
            fw.write_all(serialized_str.as_bytes())
        }).map_err(FileDbError::WriteError)?;
        
        Ok(())
    }
}
