use std::collections::{HashMap, HashSet};

use common::canonical_serialize::CanonicalSerialize;
use common::mutable_state::MutableState;

use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;
use futures::{future, FutureExt, StreamExt, SinkExt};

use crypto::identity::{SoftwareEd25519Identity, generate_pkcs8_key_pair, PublicKey,
                        PUBLIC_KEY_LEN};
use crypto::test_utils::DummyRandom;

use proto::report::messages::{FunderReport, FunderReportMutation, ChannelStatusReport,
                    FriendLivenessReport, FriendStatusReport, RequestsStatusReport};

use proto::funder::messages::{FunderIncomingControl,
    AddFriend, FriendStatus, SetFriendStatus, 
    SetFriendRemoteMaxDebt, RequestsStatus, SetRequestsStatus, 
    FunderOutgoingControl, ResponseReceived};
use proto::app_server::messages::{NamedRelayAddress, RelayAddress};


use database::DatabaseClient;

use identity::{create_identity, IdentityClient};

use crate::state::FunderState;
use crate::funder::inner_funder_loop;
use crate::ephemeral::Ephemeral;
use crate::report::create_report;


use crate::types::{ChannelerConfig, FunderOutgoingComm, FunderIncomingComm,
                IncomingLivenessMessage};

const TEST_MAX_OPERATIONS_IN_BATCH: usize = 16;
const TEST_MAX_PENDING_USER_REQUESTS: usize = 16;

// This is required to make sure the tests are not stuck.
//
// We could instead have CHANNEL_SIZE = 0 with some kind of (event_sender, event_receiver) pair, to make
// sure an asynchronous event was fully processed before continuing with the next one, but this
// approach makes tests difficult to write.
const CHANNEL_SIZE: usize = 64;

/// A helper function to quickly create a dummy NamedRelayAddress.
pub fn dummy_named_relay_address(index: u8) -> NamedRelayAddress<u32> {
    NamedRelayAddress {
        public_key: PublicKey::from(&[index; PUBLIC_KEY_LEN]),
        address: index as u32,
        name: format!("relay-{}", index),
    }
}

/// A helper function to quickly create a dummy RelayAddress.
pub fn dummy_relay_address(index: u8) -> RelayAddress<u32> {
    dummy_named_relay_address(index).into()
}

#[derive(Debug)]
struct Node<B> {
    friends: HashSet<PublicKey>,
    comm_out: mpsc::Sender<FunderIncomingComm<B>>,
}

#[derive(Debug)]
struct NewNode<B> {
    public_key: PublicKey,
    comm_in: mpsc::Receiver<FunderOutgoingComm<B>>,
    comm_out: mpsc::Sender<FunderIncomingComm<B>>,
}

#[derive(Debug)]
enum RouterEvent<B> {
    NewNode(NewNode<B>),
    OutgoingComm((PublicKey, FunderOutgoingComm<B>)), // (src_public_key, outgoing_comm)
}

async fn router_handle_outgoing_comm<'a, B:'a>(nodes: &'a mut HashMap<PublicKey, Node<B>>, 
                                        src_public_key: PublicKey,
                                        outgoing_comm: FunderOutgoingComm<B>) {
    match outgoing_comm {
        FunderOutgoingComm::FriendMessage((dest_public_key, friend_message)) => {
            let node = nodes.get_mut(&dest_public_key).unwrap();
            assert!(node.friends.contains(&src_public_key));
            let incoming_comm_message = FunderIncomingComm::Friend((src_public_key.clone(), friend_message));
            await!(node.comm_out.send(incoming_comm_message)).unwrap();
        },
        FunderOutgoingComm::ChannelerConfig(channeler_config) => {
            match channeler_config {
                ChannelerConfig::UpdateFriend(channeler_add_friend) => {
                    let node = nodes.get_mut(&src_public_key).unwrap();
                    node.friends.insert(channeler_add_friend.friend_public_key.clone());
                    let mut comm_out = node.comm_out.clone();

                    let remote_node = nodes.get(&channeler_add_friend.friend_public_key).unwrap();
                    let mut remote_node_comm_out = remote_node.comm_out.clone();
                    if remote_node.friends.contains(&src_public_key) {
                        // If there is a match, notify both sides about online state:
                        let incoming_comm_message = FunderIncomingComm::Liveness(
                            IncomingLivenessMessage::Online(src_public_key.clone()));
                        await!(remote_node_comm_out.send(incoming_comm_message)).unwrap();

                        let incoming_comm_message = FunderIncomingComm::Liveness(
                            IncomingLivenessMessage::Online(channeler_add_friend.friend_public_key.clone()));
                        await!(comm_out.send(incoming_comm_message)).unwrap();
                    }
                },
                ChannelerConfig::RemoveFriend(friend_public_key) => {
                    let node = nodes.get_mut(&src_public_key).unwrap();
                    assert!(node.friends.remove(&friend_public_key));
                    let mut comm_out = node.comm_out.clone();

                    if nodes.get(&friend_public_key).unwrap().friends.contains(&src_public_key) {
                        let incoming_comm_message = FunderIncomingComm::Liveness(
                            IncomingLivenessMessage::Offline(friend_public_key.clone()));
                        await!(comm_out.send(incoming_comm_message)).unwrap();
                    }
                },
                ChannelerConfig::SetRelays(_) => {
                    // Do nothing here. We use a mock router instead of a set of relays,
                    // so changing the address has no meaning.
                },
            }
        },
    }
}

/// A future that forwards communication between nodes. Used for testing.
/// Simulates the Channeler interface
async fn router<B,S>(incoming_new_node: mpsc::Receiver<NewNode<B>>, 
                    mut spawner: S)
where
    B: Send + 'static,
    S: Spawn + Clone,
{
    let mut nodes: HashMap<PublicKey, Node<B>> = HashMap::new();
    let (comm_sender, comm_receiver) = mpsc::channel::<(PublicKey, FunderOutgoingComm<B>)>(0);

    let incoming_new_node = incoming_new_node
        .map(|new_node| RouterEvent::NewNode(new_node));
    let comm_receiver = comm_receiver
        .map(|tuple| RouterEvent::OutgoingComm(tuple));

    let mut events = incoming_new_node.select(comm_receiver);

    while let Some(event) = await!(events.next()) {
        match event {
            RouterEvent::NewNode(new_node) => {
                let NewNode {public_key, comm_in, comm_out} = new_node;
                nodes.insert(public_key.clone(), Node { friends: HashSet::new(), comm_out});

                let c_public_key = public_key.clone();
                let mut c_comm_sender = comm_sender.clone();
                let mut mapped_comm_in = comm_in.map(move |funder_outgoing_comm|
                                                 (c_public_key.clone(), funder_outgoing_comm));
                let fut = async move {
                    await!(c_comm_sender.send_all(&mut mapped_comm_in)).unwrap()
                };
                spawner.spawn(fut.then(|_| future::ready(()))).unwrap();
            },
            RouterEvent::OutgoingComm((src_public_key, outgoing_comm)) => {
                await!(router_handle_outgoing_comm(&mut nodes, src_public_key, outgoing_comm));
            }
        };
    }
}

pub struct NodeControl<B: Clone> {
    pub public_key: PublicKey,
    send_control: mpsc::Sender<FunderIncomingControl<B>>,
    recv_control: mpsc::Receiver<FunderOutgoingControl<B>>,
    pub report: FunderReport<B>,
}

#[derive(Debug)]
pub enum NodeRecv<B:Clone> {
    ReportMutations(Vec<FunderReportMutation<B>>),
    ResponseReceived(ResponseReceived),
}

impl<B> NodeControl<B> 
where
    B: Clone + PartialEq + Eq + CanonicalSerialize,
{
    pub async fn send(&mut self, msg: FunderIncomingControl<B>) -> Option<()> {
        await!(self.send_control.send(msg))
            .ok()
            .map(|_| ())
    }

    pub async fn recv(&mut self) -> Option<NodeRecv<B>> {
        let funder_outgoing_control = await!(self.recv_control.next())?;
        match funder_outgoing_control {
            FunderOutgoingControl::ReportMutations(mutations) => {
                for mutation in &mutations {
                    self.report.mutate(&mutation).unwrap();
                }
                Some(NodeRecv::ReportMutations(mutations))
            },
            FunderOutgoingControl::ResponseReceived(response_received) =>
                Some(NodeRecv::ResponseReceived(response_received)),
        }
    }

    pub async fn recv_until<'a, P: 'a>(&'a mut self, predicate: P)
    where
        P: Fn(&FunderReport<B>) -> bool,
    {
        while !predicate(&self.report) {
            match await!(self.recv()).unwrap() {
                NodeRecv::ReportMutations(_) => {},
                NodeRecv::ResponseReceived(_) => unreachable!(),
            };
        }
    }

    pub async fn recv_until_response(&mut self) -> Option<ResponseReceived> {
        loop {
            match await!(self.recv())? {
                NodeRecv::ReportMutations(_) => {},
                NodeRecv::ResponseReceived(response_received) => return Some(response_received),
            };
        }
    }

    pub async fn add_relay<'a>(&'a mut self, named_relay_address: NamedRelayAddress<B>) {
        await!(self.send(FunderIncomingControl::AddRelay(named_relay_address.clone()))).unwrap();
        let pred = |report: &FunderReport<_>| report.relays.contains(&named_relay_address);
        await!(self.recv_until(pred));
    }

    pub async fn remove_relay<'a>(&'a mut self, public_key: PublicKey) {
        await!(self.send(FunderIncomingControl::RemoveRelay(public_key.clone()))).unwrap();
        let pred = |report: &FunderReport<_>| {
            for relay in &report.relays {
                if relay.public_key == public_key {
                    return true;
                }
            }
            false
        };
        await!(self.recv_until(pred));
    }

    pub async fn add_friend<'a>(&'a mut self, 
                         friend_public_key: &'a PublicKey,
                         address: Vec<RelayAddress<B>>,
                         name: &'a str,
                         balance: i128) {

        let add_friend = AddFriend {
            friend_public_key: friend_public_key.clone(),
            address,
            name: name.into(),
            balance, 
        };
        await!(self.send(FunderIncomingControl::AddFriend(add_friend))).unwrap();
        let pred = |report: &FunderReport<_>| report.friends.contains_key(&friend_public_key);
        await!(self.recv_until(pred));
    }

    pub async fn set_friend_status<'a>(&'a mut self, 
                         friend_public_key: &'a PublicKey,
                         status: FriendStatus) {

        let set_friend_status = SetFriendStatus {
            friend_public_key: friend_public_key.clone(),
            status: status.clone(),
        };
        await!(self.send(FunderIncomingControl::SetFriendStatus(set_friend_status))).unwrap();
        let pred = |report: &FunderReport<_>| {
           match report.friends.get(&friend_public_key) {
               None => false,
               Some(friend) => friend.status == FriendStatusReport::from(&status),
           }
        };
        await!(self.recv_until(pred));
    }

    pub async fn set_remote_max_debt<'a>(&'a mut self, 
                         friend_public_key: &'a PublicKey,
                         remote_max_debt: u128) {

        let set_remote_max_debt = SetFriendRemoteMaxDebt {
            friend_public_key: friend_public_key.clone(),
            remote_max_debt: remote_max_debt,
        };
        await!(self.send(FunderIncomingControl::SetFriendRemoteMaxDebt(set_remote_max_debt))).unwrap();

        let pred = |report: &FunderReport<_>| {
           let friend = match report.friends.get(&friend_public_key) {
               Some(friend) => friend,
               None => return false,
           };
           let tc_report = match &friend.channel_status {
               ChannelStatusReport::Consistent(tc_report) => tc_report,
               _ => return false,
           };
           tc_report.balance.remote_max_debt == remote_max_debt
        };
        await!(self.recv_until(pred));
    }

    pub async fn set_requests_status<'a>(&'a mut self, 
                         friend_public_key: &'a PublicKey,
                         requests_status: RequestsStatus) {

        let set_requests_status = SetRequestsStatus {
            friend_public_key: friend_public_key.clone(),
            status: requests_status.clone(),
        };
        await!(self.send(FunderIncomingControl::SetRequestsStatus(set_requests_status))).unwrap();

        let pred = |report: &FunderReport<_>| {
           let friend = match report.friends.get(&friend_public_key) {
               Some(friend) => friend,
               None => return false,
           };
           let tc_report = match &friend.channel_status {
               ChannelStatusReport::Consistent(tc_report) => tc_report,
               _ => return false,
           };
           tc_report.requests_status.local == RequestsStatusReport::from(&requests_status)
        };
        await!(self.recv_until(pred));
    }

    pub async fn wait_until_ready<'a>(&'a mut self, 
                         friend_public_key: &'a PublicKey) {

        let pred = |report: &FunderReport<_>| {
           let friend = match report.friends.get(&friend_public_key) {
               None => return false,
               Some(friend) => friend,
           };
           if friend.liveness != FriendLivenessReport::Online {
               return false;
           }
           let tc_report = match &friend.channel_status {
               ChannelStatusReport::Consistent(tc_report) => tc_report,
               _ => return false,
           };
           tc_report.requests_status.remote == RequestsStatusReport::from(&RequestsStatus::Open)
        };
        await!(self.recv_until(pred));
    }
}

/// Create a few node_controls, together with a router connecting them all.
/// This allows having a conversation between any two nodes.
/// We use A = u32:
pub async fn create_node_controls<S>(num_nodes: usize, 
                              mut spawner: S)
                                -> Vec<NodeControl<u32>>
where
    S: Spawn + Clone + Send + 'static,
{

    let (mut send_new_node, recv_new_node) = mpsc::channel::<NewNode<u32>>(0);
    spawner.spawn(router(recv_new_node, spawner.clone())).unwrap();

    // Avoid problems with casting to u8:
    assert!(num_nodes < 256);
    let mut node_controls = Vec::new();

    for i in 0 .. num_nodes {
        let rng = DummyRandom::new(&[i as u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender, identity_server) = create_identity(identity1);
        let identity_client = IdentityClient::new(requests_sender);
        spawner.spawn(identity_server.then(|_| future::ready(()))).unwrap();


        let public_key = await!(identity_client.request_public_key()).unwrap();
        let relays = vec![dummy_named_relay_address(i as u8)];
        let funder_state = FunderState::new(public_key.clone(), relays);
        let ephemeral = Ephemeral::new();
        let base_report = create_report(&funder_state, &ephemeral);

        // let report = create_report(&self.state, &self.ephemeral);
        // self.add_outgoing_control(FunderOutgoingControl::Report(report));


        let (db_request_sender, mut incoming_db_requests) = mpsc::channel(0);
        let db_client = DatabaseClient::new(db_request_sender);

        let fut_dispose_db_requests = async move {
            // Read all incoming db requests:
            while let Some(request) = await!(incoming_db_requests.next()) {
                let _ = request.response_sender.send(());
            }
        };
        spawner.spawn(fut_dispose_db_requests).unwrap();

        let (send_control, incoming_control) = mpsc::channel(CHANNEL_SIZE);
        let (control_sender, recv_control) = mpsc::channel(CHANNEL_SIZE);

        let (send_comm, incoming_comm) = mpsc::channel(CHANNEL_SIZE);
        let (comm_sender, recv_comm) = mpsc::channel(CHANNEL_SIZE);

        let funder_fut = inner_funder_loop(
            identity_client.clone(),
            DummyRandom::new(&[i as u8]),
            incoming_control,
            incoming_comm,
            control_sender,
            comm_sender,
            funder_state,
            db_client,
            TEST_MAX_OPERATIONS_IN_BATCH,
            TEST_MAX_PENDING_USER_REQUESTS,
            None);

        spawner.spawn(funder_fut.then(|_| future::ready(()))).unwrap();

        /*
        let base_report = match await!(recv_control.next()).unwrap() {
            FunderOutgoingControl::Report(report) => report,
            _ => unreachable!(),
        };
        */

        let new_node = NewNode {
            public_key: public_key.clone(),
            comm_in: recv_comm,
            comm_out: send_comm,
        };
        await!(send_new_node.send(new_node)).unwrap();

        node_controls.push(NodeControl {
            public_key: await!(identity_client.request_public_key()).unwrap(),
            send_control,
            recv_control,
            report: base_report,
        });
    }
    node_controls
}

