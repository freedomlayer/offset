use std::io;
use std::io::prelude::*;
use std::string::String;
use std::fs::File;
use std::path::{Path, PathBuf};

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


#[allow(unused)]
pub struct FileDb<S> {
    /// Connection to the database
    path_buf: PathBuf,
    /// Current state represented by the database:
    state: S,
}


#[allow(unused)]
impl<S> FileDb<S>
where
    S: Clone + Serialize + DeserializeOwned + MutableState,
    S::Mutation: Clone + Serialize + DeserializeOwned,
    S::MutateError: Debug,
{
    pub fn new(path_buf: PathBuf) -> Result<Self, FileDbError<S::MutateError>> {

        // Open file database, or create a new initial one:
        let mut fr = match File::open(&path_buf) {
            Ok(fr) => fr,
            Err(_) => {
                // There is no file, we create a new file:
                let initial_state = S::initial();

                // Serialize the state:
                let serialized_str = serde_json::to_string(&initial_state)
                    .map_err(FileDbError::SerializeError)?;
                // Save the new state to file, atomically:
                let af = atomicwrites::AtomicFile::new(
                    &path_buf, atomicwrites::AllowOverwrite);
                af.write(|fw| {
                    fw.write_all(serialized_str.as_bytes())
                }).map_err(FileDbError::WriteError)?;

                File::open(&path_buf)
                    .map_err(FileDbError::ReadError)?
            },
        };

        let mut serialized_str = String::new();
        fr.read_to_string(&mut serialized_str)
            .map_err(FileDbError::ReadError)?;

        let state: S = serde_json::from_str(&serialized_str)
            .map_err(FileDbError::DeserializeError)?;

        Ok(FileDb {
            path_buf,
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
    fn mutate_db(&mut self, mutations: &[Self::Mutation]) -> Result<(), Self::Error> {
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
            &self.path_buf, atomicwrites::AllowOverwrite);
        af.write(|fw| {
            fw.write_all(serialized_str.as_bytes())
        }).map_err(FileDbError::WriteError)?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// A dummy state (used for testing)
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct DummyState {
        pub x: u32,
    }

    impl DummyState {
        pub fn new() -> Self {
            DummyState {
                x: 0u32,
            }
        }
    }

    /// A dummy mutation (used for testing)
    #[derive(Debug, Serialize, Deserialize, Clone)]
    enum DummyMutation {
        Inc,
        Dec
    }

    #[derive(Debug)]
    struct DummyMutateError;

    impl MutableState for DummyState {
        type Mutation = DummyMutation;
        type MutateError = DummyMutateError;

        fn initial() -> Self {
            DummyState::new()
        }

        fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
            match mutation {
                DummyMutation::Inc => {
                    self.x = self.x.saturating_add(1);
                },
                DummyMutation::Dec => {
                    self.x = self.x.saturating_sub(1);
                },
            };
            Ok(())
        }
    }

    #[test]
    fn test_file_db_basic() {
        // Create a temporary directory:
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("database_file");
        let mut file_db = FileDb::<DummyState>::new(file_path.clone()).unwrap();

        file_db.mutate_db(&[DummyMutation::Inc, 
                         DummyMutation::Inc,
                         DummyMutation::Dec]);

        let state = file_db.get_state();
        assert_eq!(state.x, 1);

        file_db.mutate_db(&[DummyMutation::Inc, 
                         DummyMutation::Inc,
                         DummyMutation::Dec]);

        let state = file_db.get_state();
        assert_eq!(state.x, 2);

        drop(file_db);

        // Check persistency:
        let mut file_db = FileDb::<DummyState>::new(file_path.clone()).unwrap();
        let state = file_db.get_state();
        assert_eq!(state.x, 2);

        // Remove temporary directory:
        dir.close().unwrap();
    }
}
