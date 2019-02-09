use std::io;
use std::io::prelude::*;
use std::path::PathBuf;

use std::fmt::Debug;
use std::fs;

use serde::Serialize;
use serde::de::DeserializeOwned;

use serde_json;
use atomicwrites;

use common::mutable_state::MutableState;
use crate::atomic_db::AtomicDb;

#[derive(Debug)]
pub enum FileDbError<ME> {
    ReadError(io::Error),
    WriteError(atomicwrites::Error<io::Error>),
    DeserializeError(serde_json::error::Error),
    SerializeError(serde_json::error::Error),
    MutateError(ME),
    FileAlreadyExists,
}


pub struct FileDb<S> {
    /// Connection to the database
    path_buf: PathBuf,
    /// Current state represented by the database:
    state: S,
}


impl<S> FileDb<S>
where
    S: Clone + Serialize + DeserializeOwned + MutableState,
    S::Mutation: Clone + Serialize + DeserializeOwned,
    S::MutateError: Debug,
{
    /// Create a new database file from an initial state
    /// Aborts if destination file already exists
    pub fn create(path_buf: PathBuf, initial_state: S) 
        -> Result<Self, FileDbError<S::MutateError>> {

        if path_buf.exists() {
            return Err(FileDbError::FileAlreadyExists);
        }

        // There is no file, we create a new file:
        // Serialize the state:
        let serialized_str = serde_json::to_string(&initial_state)
            .map_err(FileDbError::SerializeError)?;
        // Save the new state to file, atomically:
        let af = atomicwrites::AtomicFile::new(
            &path_buf, atomicwrites::AllowOverwrite);
        af.write(|fw| {
            fw.write_all(serialized_str.as_bytes())
        }).map_err(FileDbError::WriteError)?;

        let state: S = serde_json::from_str(&serialized_str)
            .map_err(FileDbError::DeserializeError)?;

        Ok(FileDb {
            path_buf,
            state,
        })
    }

    /// Load an existing database from file
    /// Returns an error if database file does not exist
    pub fn load(path_buf: PathBuf) -> Result<Self, FileDbError<S::MutateError>> {

        let serialized_str = fs::read_to_string(&path_buf)
                    .map_err(FileDbError::ReadError)?;

        let state: S = serde_json::from_str(&serialized_str)
            .map_err(FileDbError::DeserializeError)?;

        Ok(FileDb {
            path_buf,
            state,
        })
    }
}


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
        pub fn new(x: u32) -> Self {
            DummyState {
                x,
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

        // We are not allowed to load a nonexistent database:
        assert!(FileDb::<DummyState>::load(file_path.clone()).is_err());

        // Create a new database:
        let initial_state = DummyState::new(0);
        let mut file_db = FileDb::<DummyState>::create(file_path.clone(), initial_state).unwrap();

        file_db.mutate_db(&[DummyMutation::Inc, 
                         DummyMutation::Inc,
                         DummyMutation::Dec]).unwrap();

        let state = file_db.get_state();
        assert_eq!(state.x, 1);

        file_db.mutate_db(&[DummyMutation::Inc, 
                         DummyMutation::Inc,
                         DummyMutation::Dec]).unwrap();

        let state = file_db.get_state();
        assert_eq!(state.x, 2);

        drop(file_db);

        // Check persistency:
        let file_db = FileDb::<DummyState>::load(file_path.clone()).unwrap();
        let state = file_db.get_state();
        assert_eq!(state.x, 2);


        // We should not be able to accidentally erase our state:
        let initial_state = DummyState::new(0);
        assert!(FileDb::<DummyState>::create(file_path.clone(), initial_state).is_err());

        // Remove temporary directory:
        dir.close().unwrap();
    }
}
