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

use std::{io, mem};
use std::path::Path;

use futures::prelude::*;
use futures::sync::{mpsc, oneshot};
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

enum State {
    Living,
    Closing(Box<Future<Item=(), Error=DBServiceError>>),
    Empty,
}

pub struct DBService {
    conn: Option<Connection>,
    state: State,

    funder_sender: mpsc::Sender<DatabaseToFunder>,
    funder_receiver: mpsc::Receiver<FunderToDatabase>,
    networker_sender: mpsc::Sender<DatabaseToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToDatabase>,
    indexer_client_sender: mpsc::Sender<DatabaseToIndexerClient>,
    indexer_client_receiver: mpsc::Receiver<IndexerClientToDatabase>,

    close_sender: Option<oneshot::Sender<()>>,
    close_receiver: oneshot::Receiver<()>,

    buffered_msg_to_funder: Option<DatabaseToFunder>,
    buffered_msg_to_networker: Option<DatabaseToNetworker>,
    buffered_msg_to_indexer_client: Option<DatabaseToIndexerClient>,
}

#[derive(Debug)]
pub enum DBServiceError {
    Io(io::Error),
    Sqlite(::rusqlite::Error),
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

impl DBService {
    /// Create a new `DBService` from the given path.
    ///
    /// If the given `path` doesn't point to a valid SQLite database,
    /// an error will be returned.
    pub fn new<P: AsRef<Path>>(
        path: P,
        funder_sender: mpsc::Sender<DatabaseToFunder>,
        funder_receiver: mpsc::Receiver<FunderToDatabase>,
        networker_sender: mpsc::Sender<DatabaseToNetworker>,
        networker_receiver: mpsc::Receiver<NetworkerToDatabase>,
        indexer_client_sender: mpsc::Sender<DatabaseToIndexerClient>,
        indexer_client_receiver: mpsc::Receiver<IndexerClientToDatabase>,
    ) -> Result<(CloseHandle, DBService), DBServiceError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
        )?;

        if !verify_database(&conn) {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid database").into())
        } else {
            let (close_handle, (close_sender, close_receiver)) = create_close_handle();

            let db_service = DBService {
                conn: Some(conn),
                state: State::Living,

                funder_sender,
                funder_receiver,
                networker_sender,
                networker_receiver,
                indexer_client_sender,
                indexer_client_receiver,

                close_sender: Some(close_sender),
                close_receiver,

                buffered_msg_to_funder: None,
                buffered_msg_to_networker: None,
                buffered_msg_to_indexer_client: None,
            };

            Ok((close_handle, db_service))
        }
    }

    fn handle_networker_msg(&mut self, msg: NetworkerToDatabase) {
        debug_assert!(self.buffered_msg_to_networker.is_none());
        unimplemented!()
    }

    fn handle_funder_msg(&mut self, msg: FunderToDatabase) {
        debug_assert!(self.buffered_msg_to_funder.is_none());
        unimplemented!()
    }

    fn handle_indexer_client_msg(&mut self, msg: IndexerClientToDatabase) {
        debug_assert!(self.buffered_msg_to_indexer_client.is_none());
        match msg {
            IndexerClientToDatabase::RequestLoadIndexingProvider => {

            }
            IndexerClientToDatabase::StoreIndexingProvider(info) => {

            }
            IndexerClientToDatabase::StoreRoute { id, route } => {

            }
        }

        unimplemented!()
    }

    fn start_close(&mut self) -> Poll<(), DBServiceError> {
        self.funder_receiver.close();
        self.networker_receiver.close();
        self.indexer_client_receiver.close();

        let conn = self.conn.take().expect("Attempted to start close twice!");

        let close_sender =
            self.close_sender.take().expect("Attempted to start close twice!");

        let mut close_fut = Ok(()).into_future()
            .and_then(move |_| {
                if close_sender.send(()).is_err() {
                    Err(DBServiceError::SendCloseNotificationFailed)
                } else {
                    Ok(())
                }
            });

        match close_fut.poll()? {
            Async::Ready(()) => {
                trace!("DB Service closed!");
                Ok(Async::Ready(()))
            },
            Async::NotReady => {
                self.state = State::Closing(Box::new(close_fut));
                Ok(Async::NotReady)
            }
        }
    }
}

impl Future for DBService {
    type Item = ();
    type Error = DBServiceError;

    fn poll(&mut self) -> Poll<(), DBServiceError> {
        match mem::replace(&mut self.state, State::Empty) {
            State::Empty => unreachable!(),
            State::Closing(mut closing_fut) => {
                match closing_fut.poll()? {
                    Async::Ready(_) => {
                        trace!("DBService closed!");
                        return Ok(Async::Ready(()))
                    }
                    Async::NotReady => {
                        self.state = State::Closing(closing_fut);
                        return Ok(Async::NotReady);
                    }
                }
            }
            State::Living => {
                self.state = State::Living;
            }
        }

        // If we've got some items buffered already, we need to write them to the
        // sink before we can do anything else
        if let Some(msg) = self.buffered_msg_to_funder.take() {
            match self.funder_sender.start_send(msg) {
                Err(_) => {
                    error!("an error occurred when trying to send message to funder, closing");
                    return self.start_close();
                }
                Ok(AsyncSink::Ready) => (),
                Ok(AsyncSink::NotReady(msg)) => {
                    self.buffered_msg_to_funder = Some(msg);
                }
            }
        }
        while self.buffered_msg_to_funder.is_none() {
            match self.funder_receiver.poll() {
                Err(_) => {
                    error!("funder receiver error, closing");
                    return self.start_close();
                }
                Ok(item) => {
                    match item {
                        Async::Ready(None) => {
                            error!("remote sender closed, closing");
                            return self.start_close();
                        }
                        Async::Ready(Some(msg)) => {
                            self.handle_funder_msg(msg);
                        }
                        Async::NotReady => {
                            // NOTE: This should always return `Ok(Async::Ready)`,
                            // so we ignore the error handling here.
                            drop(self.funder_sender.poll_complete());
                            break;
                        }
                    }
                }
            }
        }

        if let Some(msg) = self.buffered_msg_to_networker.take() {
            match self.networker_sender.start_send(msg) {
                Err(_) => {
                    error!("an error occurred when trying to send message to networker, closing");
                    return self.start_close();
                }
                Ok(AsyncSink::Ready) => (),
                Ok(AsyncSink::NotReady(msg)) => {
                    self.buffered_msg_to_networker = Some(msg);
                }
            }
        }
        while self.buffered_msg_to_networker.is_none() {
            match self.networker_receiver.poll() {
                Err(_) => {
                    error!("networker receiver error, closing");
                    return self.start_close();
                }
                Ok(item) => {
                    match item {
                        Async::Ready(None) => {
                            error!("remote sender closed, closing");
                            return self.start_close();
                        }
                        Async::Ready(Some(msg)) => {
                            self.handle_networker_msg(msg);
                        }
                        Async::NotReady => {
                            // NOTE: This should always return `Ok(Async::Ready)`,
                            // so we ignore the error handling here.
                            drop(self.networker_sender.poll_complete());
                            break;
                        }
                    }
                }
            }
        }

        if let Some(msg) = self.buffered_msg_to_indexer_client.take() {
            match self.indexer_client_sender.start_send(msg) {
                Err(_) => {
                    error!("an error occurred when trying to send message to indexer client, closing");
                    return self.start_close();
                }
                Ok(AsyncSink::Ready) => (),
                Ok(AsyncSink::NotReady(msg)) => {
                    self.buffered_msg_to_indexer_client = Some(msg);
                }
            }
        }
        while self.buffered_msg_to_indexer_client.is_none() {
            match self.indexer_client_receiver.poll() {
                Err(_) => {
                    error!("indexer client receiver error, closing");
                    return self.start_close();
                }
                Ok(item) => {
                    match item {
                        Async::Ready(None) => {
                            error!("remote sender closed, closing");
                            return self.start_close();
                        }
                        Async::Ready(Some(msg)) => {
                            self.handle_indexer_client_msg(msg);
                        }
                        Async::NotReady => {
                            // NOTE: This should always return `Ok(Async::Ready)`,
                            // so we ignore the error handling here.
                            drop(self.indexer_client_sender.poll_complete());
                            break;
                        }
                    }
                }
            }
        }

        Ok(Async::NotReady)
    }
}

fn verify_database(conn: &Connection) -> bool {
    true
}