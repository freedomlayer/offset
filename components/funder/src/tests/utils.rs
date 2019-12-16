use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

use common::mutable_state::MutableState;
use signature::canonical::CanonicalSerialize;

use futures::channel::mpsc;
use futures::stream::select;
use futures::task::{Spawn, SpawnExt};
use futures::{future, FutureExt, SinkExt, StreamExt};

use crypto::identity::{SoftwareEd25519Identity};
use crypto::test_utils::DummyRandom;
use crypto::rand::RandGen;

use proto::crypto::{PublicKey, Uid, PrivateKey};

use proto::report::messages::{
    ChannelStatusReport, FriendLivenessReport, FunderReport, FunderReportMutations,
    RequestsStatusReport,
};

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::funder::messages::{
    AddFriend, Currency, FriendStatus, FunderControl, FunderIncomingControl, FunderOutgoingControl,
    Rate, RemoveFriend, RemoveFriendCurrency, RequestsStatus, ResponseClosePayment,
    SetFriendCurrencyMaxDebt, SetFriendCurrencyRate, SetFriendCurrencyRequestsStatus,
    SetFriendStatus, TransactionResult,
};

use database::DatabaseClient;

use identity::{create_identity, IdentityClient};

use crate::ephemeral::Ephemeral;
use crate::funder::inner_funder_loop;
use crate::report::create_report;
use crate::state::FunderState;

use crate::types::{
    ChannelerConfig, FunderIncomingComm, FunderOutgoingComm, IncomingLivenessMessage,
};

const TEST_MAX_NODE_RELAYS: usize = 16;
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
        public_key: PublicKey::from(&[index; PublicKey::len()]),
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

async fn router_handle_outgoing_comm<'a, B: 'a>(
    nodes: &'a mut HashMap<PublicKey, Node<B>>,
    src_public_key: PublicKey,
    outgoing_comm: FunderOutgoingComm<B>,
) where
    B: Debug,
{
    match outgoing_comm {
        FunderOutgoingComm::FriendMessage((dest_public_key, friend_message)) => {
            let node = nodes.get_mut(&dest_public_key).unwrap();
            assert!(node.friends.contains(&src_public_key));
            let incoming_comm_message =
                FunderIncomingComm::Friend((src_public_key.clone(), friend_message));
            let _ = node.comm_out.send(incoming_comm_message).await;
        }
        FunderOutgoingComm::ChannelerConfig(channeler_config) => {
            match channeler_config {
                ChannelerConfig::UpdateFriend(channeler_add_friend) => {
                    let node = nodes.get_mut(&src_public_key).unwrap();
                    if node
                        .friends
                        .contains(&channeler_add_friend.friend_public_key)
                    {
                        // Nothing changed, we exit early.
                        return;
                    }

                    node.friends
                        .insert(channeler_add_friend.friend_public_key.clone());

                    let mut comm_out = node.comm_out.clone();

                    let remote_node = nodes.get(&channeler_add_friend.friend_public_key).unwrap();
                    let mut remote_node_comm_out = remote_node.comm_out.clone();
                    if remote_node.friends.contains(&src_public_key) {
                        // If there is a match, notify both sides about online state:
                        let incoming_comm_message = FunderIncomingComm::Liveness(
                            IncomingLivenessMessage::Online(src_public_key.clone()),
                        );
                        let _ = remote_node_comm_out.send(incoming_comm_message).await;

                        let incoming_comm_message =
                            FunderIncomingComm::Liveness(IncomingLivenessMessage::Online(
                                channeler_add_friend.friend_public_key.clone(),
                            ));
                        let _ = comm_out.send(incoming_comm_message).await;
                    }
                }
                ChannelerConfig::RemoveFriend(friend_public_key) => {
                    let node = nodes.get_mut(&src_public_key).unwrap();
                    assert!(node.friends.remove(&friend_public_key));
                    let mut comm_out = node.comm_out.clone();

                    if nodes
                        .get(&friend_public_key)
                        .unwrap()
                        .friends
                        .contains(&src_public_key)
                    {
                        let incoming_comm_message = FunderIncomingComm::Liveness(
                            IncomingLivenessMessage::Offline(friend_public_key.clone()),
                        );
                        let _ = comm_out.send(incoming_comm_message).await;
                    }
                }
                ChannelerConfig::SetRelays(_) => {
                    // Do nothing here. We use a mock router instead of a set of relays,
                    // so changing the address has no meaning.
                }
            }
        }
    }
}

/// A future that forwards communication between nodes. Used for testing.
/// Simulates the Channeler interface
async fn router<B, S>(incoming_new_node: mpsc::Receiver<NewNode<B>>, spawner: S)
where
    B: Send + Debug + 'static,
    S: Spawn + Clone,
{
    let mut nodes: HashMap<PublicKey, Node<B>> = HashMap::new();
    let (comm_sender, comm_receiver) = mpsc::channel::<(PublicKey, FunderOutgoingComm<B>)>(0);

    let incoming_new_node = incoming_new_node.map(|new_node| RouterEvent::NewNode(new_node));
    let comm_receiver = comm_receiver.map(|tuple| RouterEvent::OutgoingComm(tuple));

    let mut events = select(incoming_new_node, comm_receiver);

    while let Some(event) = events.next().await {
        match event {
            RouterEvent::NewNode(new_node) => {
                let NewNode {
                    public_key,
                    comm_in,
                    comm_out,
                } = new_node;
                nodes.insert(
                    public_key.clone(),
                    Node {
                        friends: HashSet::new(),
                        comm_out,
                    },
                );

                let c_public_key = public_key.clone();
                let mut c_comm_sender = comm_sender.clone();
                let mut mapped_comm_in = comm_in
                    .map(move |funder_outgoing_comm| (c_public_key.clone(), funder_outgoing_comm))
                    .map(Ok);
                let fut = async move { c_comm_sender.send_all(&mut mapped_comm_in).await.unwrap() };
                spawner.spawn(fut.then(|_| future::ready(()))).unwrap();
            }
            RouterEvent::OutgoingComm((src_public_key, outgoing_comm)) => {
                router_handle_outgoing_comm(&mut nodes, src_public_key, outgoing_comm).await;
            }
        };
    }
}

pub struct NodeControl<B: Clone> {
    pub public_key: PublicKey,
    send_control: mpsc::Sender<FunderIncomingControl<B>>,
    recv_control: mpsc::Receiver<FunderOutgoingControl<B>>,
    pub report: FunderReport<B>,
    next_app_request_id: u64,
}

#[derive(Debug)]
pub enum NodeRecv<B: Clone> {
    ReportMutations(FunderReportMutations<B>),
    ResponseClosePayment(ResponseClosePayment),
    TransactionResult(TransactionResult),
}

impl<B> NodeControl<B>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize,
{
    pub async fn send(&mut self, funder_control: FunderControl<B>) {
        // Convert self.next_app_request_id to a Uid:
        let mut app_request_id_inner = [0u8; Uid::len()];
        let mut next_app_request_id = self.next_app_request_id;
        for j in 0..8 {
            app_request_id_inner[j] = (next_app_request_id & 0xff) as u8;
            next_app_request_id >>= 8;
        }

        // Advance self.next_app_request_id:
        self.next_app_request_id = self.next_app_request_id.checked_add(1).unwrap();

        let app_request_id = Uid::from(&app_request_id_inner);
        let funder_incoming_control = FunderIncomingControl {
            app_request_id: app_request_id.clone(),
            funder_control,
        };
        self.send_control
            .send(funder_incoming_control)
            .await
            .unwrap();

        loop {
            match self.recv().await.unwrap() {
                NodeRecv::ReportMutations(funder_report_mutations) => {
                    if funder_report_mutations.opt_app_request_id == Some(app_request_id.clone()) {
                        break;
                    }
                }
                _ => {}
            };
        }
    }

    pub async fn recv(&mut self) -> Option<NodeRecv<B>> {
        let funder_outgoing_control = self.recv_control.next().await?;
        match funder_outgoing_control {
            FunderOutgoingControl::ReportMutations(funder_report_mutations) => {
                for mutation in &funder_report_mutations.mutations {
                    self.report.mutate(&mutation).unwrap();
                }
                Some(NodeRecv::ReportMutations(funder_report_mutations))
            }
            FunderOutgoingControl::ResponseClosePayment(response_close_payment) => {
                Some(NodeRecv::ResponseClosePayment(response_close_payment))
            }
            FunderOutgoingControl::TransactionResult(transaction_result) => {
                Some(NodeRecv::TransactionResult(transaction_result))
            }
        }
    }

    pub async fn recv_until<'a, P: 'a>(&'a mut self, predicate: P)
    where
        P: Fn(&FunderReport<B>) -> bool,
    {
        while !predicate(&self.report) {
            match self.recv().await.unwrap() {
                NodeRecv::ReportMutations(_) => {}
                NodeRecv::TransactionResult(_) => unreachable!(),
                NodeRecv::ResponseClosePayment(_) => unreachable!(),
            };
        }
    }

    pub async fn recv_until_transaction_result(&mut self) -> Option<TransactionResult> {
        loop {
            match self.recv().await? {
                NodeRecv::ReportMutations(_) => {}
                NodeRecv::TransactionResult(transaction_result) => return Some(transaction_result),
                NodeRecv::ResponseClosePayment(_) => {}
            };
        }
    }

    pub async fn recv_until_response_close_payment(&mut self) -> Option<ResponseClosePayment> {
        loop {
            match self.recv().await? {
                NodeRecv::ReportMutations(_) => {}
                NodeRecv::TransactionResult(_) => {}
                NodeRecv::ResponseClosePayment(response_close_payment) => {
                    return Some(response_close_payment)
                }
            };
        }
    }

    pub async fn add_relay<'a>(&'a mut self, named_relay_address: NamedRelayAddress<B>) {
        self.send(FunderControl::AddRelay(named_relay_address.clone()))
            .await;
    }

    pub async fn remove_relay<'a>(&'a mut self, public_key: PublicKey) {
        self.send(FunderControl::RemoveRelay(public_key.clone()))
            .await;
    }

    pub async fn add_friend<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        relays: Vec<RelayAddress<B>>,
        name: &'a str,
    ) {
        let add_friend = AddFriend {
            friend_public_key: friend_public_key.clone(),
            relays,
            name: name.into(),
        };
        self.send(FunderControl::AddFriend(add_friend)).await;
    }

    pub async fn remove_friend<'a>(&'a mut self, friend_public_key: &'a PublicKey) {
        let remove_friend = RemoveFriend {
            friend_public_key: friend_public_key.clone(),
        };
        self.send(FunderControl::RemoveFriend(remove_friend)).await;
    }

    pub async fn set_friend_status<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        status: FriendStatus,
    ) {
        let set_friend_status = SetFriendStatus {
            friend_public_key: friend_public_key.clone(),
            status: status.clone(),
        };
        self.send(FunderControl::SetFriendStatus(set_friend_status))
            .await;
    }

    pub async fn set_remote_max_debt<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        currency: &'a Currency,
        remote_max_debt: u128,
    ) {
        let set_remote_max_debt = SetFriendCurrencyMaxDebt {
            friend_public_key: friend_public_key.clone(),
            currency: currency.clone(),
            remote_max_debt: remote_max_debt.into(),
        };
        self.send(FunderControl::SetFriendCurrencyMaxDebt(set_remote_max_debt))
            .await;
    }

    #[allow(unused)]
    pub async fn remove_friend_currency<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        currency: &'a Currency,
    ) {
        let remove_friend_currency = RemoveFriendCurrency {
            friend_public_key: friend_public_key.clone(),
            currency: currency.clone(),
        };
        self.send(FunderControl::RemoveFriendCurrency(remove_friend_currency))
            .await;
    }

    pub async fn set_requests_status<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        currency: &'a Currency,
        requests_status: RequestsStatus,
    ) {
        let set_requests_status = SetFriendCurrencyRequestsStatus {
            friend_public_key: friend_public_key.clone(),
            currency: currency.clone(),
            status: requests_status.clone(),
        };
        self.send(FunderControl::SetFriendCurrencyRequestsStatus(
            set_requests_status,
        ))
        .await;
    }

    pub async fn set_friend_currency_rate<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        currency: &'a Currency,
        rate: Rate,
    ) {
        let set_friend_currency_rate = SetFriendCurrencyRate {
            friend_public_key: friend_public_key.clone(),
            currency: currency.clone(),
            rate,
        };
        self.send(FunderControl::SetFriendCurrencyRate(
            set_friend_currency_rate,
        ))
        .await;
    }

    /// A shim, allowing to easily enable multiple currencies at once
    pub async fn set_friend_currencies<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        currencies: Vec<Currency>,
    ) {
        for currency in currencies {
            self.set_friend_currency_rate(friend_public_key, &currency, Rate::new())
                .await;
        }
    }

    pub async fn wait_until_currency_active<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        currency: &'a Currency,
    ) {
        let pred = |report: &FunderReport<_>| {
            let friend = match report.friends.get(&friend_public_key) {
                None => return false,
                Some(friend) => friend,
            };
            if friend.liveness != FriendLivenessReport::Online {
                return false;
            }
            let currency_reports = match &friend.channel_status {
                ChannelStatusReport::Consistent(channel_consistent) => {
                    &channel_consistent.currency_reports
                }
                _ => return false,
            };

            if let Some(_) = currency_reports
                .iter()
                .position(|currency_report| &currency_report.currency == currency)
            {
                true
            } else {
                false
            }
        };
        self.recv_until(pred).await;
    }

    pub async fn wait_until_ready<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        currency: &'a Currency,
    ) {
        let pred = |report: &FunderReport<_>| {
            let friend = match report.friends.get(&friend_public_key) {
                None => return false,
                Some(friend) => friend,
            };
            if friend.liveness != FriendLivenessReport::Online {
                return false;
            }
            let currency_reports = match &friend.channel_status {
                ChannelStatusReport::Consistent(channel_consistent) => {
                    &channel_consistent.currency_reports
                }
                _ => return false,
            };
            if let Some(pos) = currency_reports
                .iter()
                .position(|currency_report| &currency_report.currency == currency)
            {
                let currency_report = &currency_reports[pos];
                currency_report.requests_status.remote
                    == RequestsStatusReport::from(&RequestsStatus::Open)
            } else {
                false
            }
        };
        self.recv_until(pred).await;
    }

    pub async fn wait_friend_balance<'a>(
        &'a mut self,
        friend_public_key: &'a PublicKey,
        currency: &'a Currency,
        balance: i128,
    ) {
        let pred = |report: &FunderReport<_>| {
            let friend = report.friends.get(friend_public_key).unwrap();
            let currency_reports = match &friend.channel_status {
                ChannelStatusReport::Consistent(channel_consistent) => {
                    &channel_consistent.currency_reports
                }
                _ => return false,
            };

            let opt_pos = currency_reports
                .iter()
                .position(|currency_report| &currency_report.currency == currency);
            if let Some(pos) = opt_pos {
                currency_reports[pos].balance.balance == balance
            } else {
                false
            }
        };
        self.recv_until(pred).await;
    }
}

/// Create a few node_controls, together with a router connecting them all.
/// This allows having a conversation between any two nodes.
/// We use A = u32:
pub async fn create_node_controls<S>(num_nodes: usize, spawner: S) -> Vec<NodeControl<u32>>
where
    S: Spawn + Clone + Send + 'static,
{
    let (mut send_new_node, recv_new_node) = mpsc::channel::<NewNode<u32>>(0);
    spawner
        .spawn(router(recv_new_node, spawner.clone()))
        .unwrap();

    // Avoid problems with casting to u8:
    assert!(num_nodes < 256);
    let mut node_controls = Vec::new();

    for i in 0..num_nodes {
        let rng = DummyRandom::new(&[i as u8]);
        let private_key = PrivateKey::rand_gen(&rng);
        let identity1 = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
        let (requests_sender, identity_server) = create_identity(identity1);
        let identity_client = IdentityClient::new(requests_sender);
        spawner
            .spawn(identity_server.then(|_| future::ready(())))
            .unwrap();

        let public_key = identity_client.request_public_key().await.unwrap();
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
            while let Some(request) = incoming_db_requests.next().await {
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
            TEST_MAX_NODE_RELAYS,
            TEST_MAX_OPERATIONS_IN_BATCH,
            TEST_MAX_PENDING_USER_REQUESTS,
            None,
        );

        spawner
            .spawn(funder_fut.then(|_| future::ready(())))
            .unwrap();

        /*
        let base_report = match recv_control.next().await.unwrap() {
            FunderOutgoingControl::Report(report) => report,
            _ => unreachable!(),
        };
        */

        let new_node = NewNode {
            public_key: public_key.clone(),
            comm_in: recv_comm,
            comm_out: send_comm,
        };
        send_new_node.send(new_node).await.unwrap();

        node_controls.push(NodeControl {
            public_key: identity_client.request_public_key().await.unwrap(),
            send_control,
            recv_control,
            report: base_report,
            next_app_request_id: 0,
        });
    }
    node_controls
}
