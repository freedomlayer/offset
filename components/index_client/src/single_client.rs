use std::marker::Unpin;
use futures::{future, stream, Stream, StreamExt};
use futures::channel::oneshot;

use common::conn::ConnPair;
use crypto::crypto_rand::RandValue;

use proto::index::messages::{IndexServerToClient, 
    IndexClientToServer, IndexServerToServer, 
    ResponseRoutes, RouteWithCapacity, MutationsUpdate,
    ForwardMutationsUpdate, Mutation, TimeProofLink,
    RequestRoutes};

type ServerConn = ConnPair<IndexClientToServer, IndexServerToClient>;

#[derive(Debug)]
pub enum SingleClientControl {
    RequestRoutes((RequestRoutes, oneshot::Sender<Vec<RouteWithCapacity>>)),
    SendMutation(Mutation),
}

#[derive(Debug)]
pub enum SingleClientError {
    ControlClosed,
    ServerClosed,
}

#[derive(Debug)]
enum SingleClientEvent {
    FromServer(IndexServerToClient),
    ServerClosed,
    Control(SingleClientControl),
    ControlClosed,
}

async fn single_client_loop<IC>(server_conn: ServerConn,
                     incoming_control: IC,
                     session_id: RandValue) -> Result<(), SingleClientError>
where
    IC: Stream<Item=SingleClientControl> + Unpin,
{
    // TODO:
    // - Setup session id (RandValue, Possibly take as argument?)
    // - Setup counter
    // - Wait for a first time hash. Before such a hash is obtained it is not possible to send
    //      mutations update messages.
    //      - Should send something special on the first hash? 
    //          - Possibly send our full state as mutations.

    let (to_server, from_server) = server_conn;

    let from_server = from_server
        .map(|index_server_to_client| SingleClientEvent::FromServer(index_server_to_client))
        .chain(stream::once(future::ready(SingleClientEvent::ServerClosed)));

    let incoming_control = incoming_control
        .map(|index_client_control| SingleClientEvent::Control(index_client_control))
        .chain(stream::once(future::ready(SingleClientEvent::ControlClosed)));

    let mut events = from_server.select(incoming_control);

    while let Some(event) = await!(events.next()) {
        match event {
            SingleClientEvent::FromServer(index_server_to_client) => unimplemented!(),
            SingleClientEvent::ServerClosed => return Err(SingleClientError::ServerClosed),
            SingleClientEvent::Control(index_client_control) => unimplemented!(),
            SingleClientEvent::ControlClosed => return Err(SingleClientError::ControlClosed),
        }
        // TODO
    }
    Ok(())
}
