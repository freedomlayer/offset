//! The database service used to manage the `CSwitch` node's information.
//!
//! # Introduction
//!
//! The CSwitch requires persistency to make sure the mutual credit management
//! between neighbors and friends remain consistent, despite possible failures.
//!
//! # Initialization
//!
//! To create an `DBService`, we need to provide a path to the database.
//!
//! # Data Models
//!
//! ## Neighbors Token Channel Data Model
//!
//! ### TABLE: `NeighborTokenChannel`
//!
//! ### TABLE: `NeighborLocalRequest`
//!
//! ## Indexer Client Data Model
//!
//! ### TABLE: `IndexingProviders`
//!

use std::io;
use std::path::Path;

use futures::prelude::*;
use futures::future::join_all;
use futures::sync::{mpsc, oneshot};

use tokio_core::reactor::Handle;

use rusqlite::{self, Connection, OpenFlags};

use inner_messages::{
    FunderToDatabase,
    DatabaseToFunder,
    NetworkerToDatabase,
    DatabaseToNetworker,
    IndexerClientToDatabase,
    DatabaseToIndexerClient,
};

use close_handle::{CloseHandle, create_close_handle};

#[derive(Debug)]
pub enum DBServiceError {
    Io(io::Error),
    Sqlite(::rusqlite::Error),
    CloseReceiverCanceled,
    RecvFromFunderFailed,
    RecvFromNetworkerFailed,
    RecvFromIndexerClientFailed,
    SendToFunderFailed,
    SendToNetworkerFailed,
    SendToIndexerClientFailed,
    SendCloseNotificationFailed,
}

impl From<io::Error> for DBServiceError {
    fn from(e: io::Error) -> DBServiceError {
        DBServiceError::Io(e)
    }
}

impl From<rusqlite::Error> for DBServiceError {
    fn from(e: rusqlite::Error) -> DBServiceError {
        DBServiceError::Sqlite(e)
    }
}

fn create_indexer_client_reader(
    sender: mpsc::Sender<DatabaseToIndexerClient>,
    receiver: mpsc::Receiver<IndexerClientToDatabase>,
) -> (CloseHandle, impl Future<Item=(), Error=DBServiceError>) {
    let (close_handle, (close_sender, close_receiver)) = create_close_handle();

    let reader = receiver.map_err(|_| {
        DBServiceError::RecvFromIndexerClientFailed
    }).for_each(|msg| {
        match msg {
            IndexerClientToDatabase::RequestLoadIndexingProvider => {}
            IndexerClientToDatabase::StoreIndexingProvider(info) => {}
            IndexerClientToDatabase::StoreRoute { id, route } => {}
        }

        Ok(())
    });

    let united_reader = close_receiver
        .map_err(|_: oneshot::Canceled| {
            DBServiceError::CloseReceiverCanceled
        })
        .select(reader)
        .map_err(move |(reader_error, _)| reader_error)
        .and_then(|_| {
            if close_sender.send(()).is_err() {
                Err(DBServiceError::SendCloseNotificationFailed)
            } else {
                Ok(())
            }
        });

    (close_handle, united_reader)
}

pub fn create_db_service<P: AsRef<Path>>(
    path: P,
    handle: &Handle,
    funder_sender: mpsc::Sender<DatabaseToFunder>,
    funder_receiver: mpsc::Receiver<FunderToDatabase>,
    networker_sender: mpsc::Sender<DatabaseToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToDatabase>,
    indexer_client_sender: mpsc::Sender<DatabaseToIndexerClient>,
    indexer_client_receiver: mpsc::Receiver<IndexerClientToDatabase>,
) -> Result<(CloseHandle, impl Future<Item=(), Error=DBServiceError>), DBServiceError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;

    if !verify_database(&conn) {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid database").into())
    } else {
        let (close_handle, (close_sender, close_receiver)) = create_close_handle();

        // TODO: funder_reader
        // TODO: networker_reader

        let (indexer_client_reader_close_handle, indexer_client_reader) =
            create_indexer_client_reader(indexer_client_sender, indexer_client_receiver);

        // FIXME: Would miss the SendCloseNotificationFailed error
        handle.spawn(indexer_client_reader.map_err(|_| {
            panic!("internal error");
        }));

        let db_service_guard = close_receiver
            .map_err(|_: oneshot::Canceled| DBServiceError::CloseReceiverCanceled)
            .and_then(|()| {
                let indexer_client_reader_close_receiver =
                    indexer_client_reader_close_handle.close().expect("Can not close twice!");

                let internal_close_task = join_all(vec![
                    indexer_client_reader_close_receiver
                    // TODO:
                ]);

                internal_close_task.map_err(|_: oneshot::Canceled| {
                    DBServiceError::CloseReceiverCanceled
                }).and_then(|_| {
                    if close_sender.send(()).is_err() {
                        Err(DBServiceError::SendCloseNotificationFailed)
                    } else {
                        Ok(())
                    }
                })
            });

        Ok((close_handle, db_service_guard))
    }
}

fn verify_database(conn: &Connection) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use tokio_core::reactor::Timeout;
    use tokio_core::reactor::Core;

    #[test]
    fn dispatch_close() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let (db2idx_sender, db2idx_receiver) = mpsc::channel::<DatabaseToIndexerClient>(0);
        let (idx2db_sender, idx2db_receiver) = mpsc::channel::<IndexerClientToDatabase>(0);

        let (db2funder_sender, db2funder_receiver) = mpsc::channel::<DatabaseToFunder>(0);
        let (funder2db_sender, funder2db_receiver) = mpsc::channel::<FunderToDatabase>(0);

        let (db2net_sender, db2net_receiver) = mpsc::channel::<DatabaseToNetworker>(0);
        let (net2db_sender, net2db_receiver) = mpsc::channel::<NetworkerToDatabase>(0);

        let (db_close_handle, db_service) = create_db_service(
            "test.sqlite",
            &handle,
            db2funder_sender,
            funder2db_receiver,
            db2net_sender,
            net2db_receiver,
            db2idx_sender,
            idx2db_receiver,
        ).unwrap();

        handle.spawn(db_service.map_err(|_| ()));

        let timeout = Timeout::new(Duration::from_millis(100), &handle).unwrap();

        let work = timeout.and_then(move |_| {
                let db_close_receiver =
                    db_close_handle.close().expect("Can not close twice!");

                db_close_receiver
                    .map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to close"))
            });

        core.run(work).unwrap();
    }
}