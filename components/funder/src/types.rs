use std::collections::hash_set::HashSet;

use byteorder::{WriteBytesExt, BigEndian};

use utils::int_convert::{usize_to_u64};

use crypto::identity::{PublicKey, Signature, verify_signature};
use crypto::uid::Uid;
use crypto::crypto_rand::RandValue;
use crypto::hash;
use crypto::hash::{HashResult, sha_512_256};

use identity::IdentityClient;

use super::messages::ResponseSendFundsResult;
use super::report::FunderReport;

// Prefix used for chain hashing of token channel funds.
// NEXT is used for hashing for the next move token funds.
const TOKEN_NEXT: &[u8] = b"NEXT";


pub const INVOICE_ID_LEN: usize = 32;

/// The universal unique identifier of an invoice.
define_fixed_bytes!(InvoiceId, INVOICE_ID_LEN);

// pub const CHANNEL_TOKEN_LEN: usize = 32;
// define_fixed_bytes!(ChannelToken, CHANNEL_TOKEN_LEN);

// The hash of the previous message sent over the token channel.
// struct ChannelToken(Signature);


#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum FriendStatus {
    Enable = 1,
    Disable = 0,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RequestsStatus {
    Open,
    Closed,
}

impl RequestsStatus {
    pub fn is_open(&self) -> bool {
        if let RequestsStatus::Open = self {
            true
        } else {
            false
        }
    }
}


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum FriendTcOp {
    EnableRequests,
    DisableRequests,
    SetRemoteMaxDebt(u128),
    RequestSendFunds(RequestSendFunds),
    ResponseSendFunds(ResponseSendFunds),
    FailureSendFunds(FailureSendFunds),
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct PendingFriendRequest {
    pub request_id: Uid,
    pub route: FriendsRoute,
    pub dest_payment: u128,
    pub invoice_id: InvoiceId,
}


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSendFunds {
    pub request_id: Uid,
    pub rand_nonce: RandValue,
    pub signature: Signature,
}


/// The ratio can be numeration / T::max_value(), or 1
#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum Ratio<T> {
    One,
    Numerator(T),
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct FreezeLink {
    pub shared_credits: u128,
    pub usable_ratio: Ratio<u128>
}

/// A request to send funds that originates from the user
#[derive(Debug, Clone)]
pub struct UserRequestSendFunds {
    pub request_id: Uid,
    pub route: FriendsRoute,
    pub invoice_id: InvoiceId,
    pub dest_payment: u128,
}


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct RequestSendFunds {
    pub request_id: Uid,
    pub route: FriendsRoute,
    pub dest_payment: u128,
    pub invoice_id: InvoiceId,
    pub freeze_links: Vec<FreezeLink>,
}


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct FailureSendFunds {
    pub request_id: Uid,
    pub reporting_public_key: PublicKey,
    pub rand_nonce: RandValue,
    pub signature: Signature,
}


#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FriendsRoute {
    pub public_keys: Vec<PublicKey>,
}


#[allow(unused)]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct FriendMoveToken {
    pub operations: Vec<FriendTcOp>,
    pub old_token: Signature,
    pub inconsistency_counter: u64,
    pub move_token_counter: u128,
    pub balance: i128,
    pub local_pending_debt: u128,
    pub remote_pending_debt: u128,
    pub rand_nonce: RandValue,
    pub new_token: Signature,
}


impl FriendMoveToken {
    pub async fn new(operations: Vec<FriendTcOp>,
                 old_token: Signature,
                 inconsistency_counter: u64,
                 move_token_counter: u128,
                 balance: i128,
                 local_pending_debt: u128,
                 remote_pending_debt: u128,
                 rand_nonce: RandValue,
                 identity_client: IdentityClient) -> FriendMoveToken {

        let mut friend_move_token = FriendMoveToken {
            operations,
            old_token,
            inconsistency_counter,
            move_token_counter,
            balance,
            local_pending_debt,
            remote_pending_debt,
            rand_nonce,
            new_token: Signature::zero(),
        };

        let sig_buffer = friend_move_token.signature_buff();
        friend_move_token.new_token = await!(identity_client.request_signature(sig_buffer)).unwrap();
        friend_move_token
    }

    fn signature_buff(&self) -> Vec<u8> {
        let mut contents = Vec::new();
        contents.write_u64::<BigEndian>(
            usize_to_u64(self.operations.len()).unwrap()).unwrap();
        for op in &self.operations {
            contents.extend_from_slice(&op.to_bytes());
        }

        let mut sig_buffer = Vec::new();
        sig_buffer.extend_from_slice(&sha_512_256(TOKEN_NEXT));
        sig_buffer.extend_from_slice(&contents);
        sig_buffer.extend_from_slice(&self.old_token);
        sig_buffer.write_u64::<BigEndian>(self.inconsistency_counter).unwrap();
        sig_buffer.write_u128::<BigEndian>(self.move_token_counter).unwrap();
        sig_buffer.write_i128::<BigEndian>(self.balance).unwrap();
        sig_buffer.write_u128::<BigEndian>(self.local_pending_debt).unwrap();
        sig_buffer.write_u128::<BigEndian>(self.remote_pending_debt).unwrap();
        sig_buffer.extend_from_slice(&self.rand_nonce);

        sig_buffer
    }

    pub fn verify(&self, public_key: &PublicKey) -> bool {
        let sig_buffer = self.signature_buff();
        verify_signature(&sig_buffer, public_key, &self.new_token)
    }
}


impl FriendsRoute {
    /*
    pub fn bytes_count(&self) -> usize {
        mem::size_of::<PublicKey>() * 2 +
            FriendRouteLink::bytes_count()
                * self.route_links.len()
    }
    */

    pub fn len(&self) -> usize {
        self.public_keys.len()
    }

    /// Check if the route is valid.
    /// A valid route must have at least 2 nodes, and is in one of the following forms:
    /// A -- B -- C -- D -- E -- F -- A   (Single cycle, first == last)
    /// A -- B -- C -- D -- E -- F        (A route with no repetitions)
    pub fn is_valid(&self) -> bool {
        if self.public_keys.len() < 2 {
            return false;
        }

        let mut seen = HashSet::new();
        for public_key in &self.public_keys[.. self.public_keys.len() - 1] {
            if !seen.insert(public_key.clone()) {
                return false;
            }
        }
        let last_pk = &self.public_keys[self.public_keys.len() - 1];
        if last_pk == &self.public_keys[0] {
            // The last public key closes a cycle
            true
        } else {
            // A route with no repetitions
            seen.insert(last_pk.clone())
        }
    }

    /// Find two consecutive public keys (pk1, pk2) inside a friends route.
    pub fn find_pk_pair(&self, pk1: &PublicKey, pk2: &PublicKey) -> Option<usize> {
        let pks = &self.public_keys;
        for i in 0 ..= pks.len().checked_sub(2)? {
            if pk1 == &pks[i] && pk2 == &pks[i+1] {
                return Some(i);
            }
        }
        None
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.write_u64::<BigEndian>(
            usize_to_u64(self.public_keys.len()).unwrap()).unwrap();
        for public_key in &self.public_keys {
            res_bytes.extend_from_slice(public_key);
        }
        res_bytes
    }

    /// Produce a cryptographic hash over the contents of the route.
    pub fn hash(&self) -> HashResult {
        hash::sha_512_256(&self.to_bytes())
    }

    /// Find the index of a public key inside the route.
    /// source is considered to be index 0.
    /// dest is considered to be the last index.
    ///
    /// Note that the returned index does not map directly to an 
    /// index of self.route_links vector.
    pub fn pk_to_index(&self, public_key: &PublicKey) -> Option<usize> {
        self.public_keys
            .iter()
            .position(|cur_public_key| cur_public_key == public_key)
    }

    /// Get the public key of a node according to its index.
    pub fn index_to_pk(&self, index: usize) -> Option<&PublicKey> {
        self.public_keys.get(index)
    }
}


impl Ratio<u128> {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match *self {
            Ratio::One => {
                res_bytes.write_u8(0).unwrap();
            },
            Ratio::Numerator(num) => {
                res_bytes.write_u8(1).unwrap();
                res_bytes.write_u128::<BigEndian>(num).unwrap();
            },
        }
        res_bytes
    }
}

impl FreezeLink {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.write_u128::<BigEndian>(self.shared_credits)
            .expect("Could not serialize u64!");
        res_bytes.extend_from_slice(&self.usable_ratio.to_bytes());
        res_bytes
    }
}

impl RequestSendFunds {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.route.to_bytes());
        res_bytes.write_u128::<BigEndian>(self.dest_payment).unwrap();
        for freeze_link in &self.freeze_links {
            res_bytes.extend_from_slice(&freeze_link.to_bytes());
        }
        res_bytes
    }

    pub fn create_pending_request(&self) -> PendingFriendRequest {
        PendingFriendRequest {
            request_id: self.request_id,
            route: self.route.clone(),
            dest_payment: self.dest_payment,
            invoice_id: self.invoice_id.clone(),
        }
    }
}

impl ResponseSendFunds {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.rand_nonce);
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}


impl FailureSendFunds {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.reporting_public_key);
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}

#[allow(unused)]
impl FriendTcOp {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            FriendTcOp::EnableRequests => {
                res_bytes.push(0u8);
            },
            FriendTcOp::DisableRequests => {
                res_bytes.push(1u8);
            }
            FriendTcOp::SetRemoteMaxDebt(remote_max_debt) => {
                res_bytes.push(2u8);
                res_bytes.write_u128::<BigEndian>(*remote_max_debt)
                    .expect("Failed to serialize u64 (remote_max_debt)");
            }
            FriendTcOp::RequestSendFunds(request_send_funds) => {
                res_bytes.push(3u8);
                res_bytes.append(&mut request_send_funds.to_bytes())
            }
            FriendTcOp::ResponseSendFunds(response_send_funds) => {
                res_bytes.push(4u8);
                res_bytes.append(&mut response_send_funds.to_bytes())
            }
            FriendTcOp::FailureSendFunds(failure_send_funds) => {
                res_bytes.push(5u8);
                res_bytes.append(&mut failure_send_funds.to_bytes())
            }
            
        }
        res_bytes
    }

    /// Get an approximation of the amount of bytes required to represent 
    /// this operation.
    pub fn approx_bytes_count(&self) -> usize {
        self.to_bytes().len()
    }
}

impl UserRequestSendFunds {
    pub fn to_request(self) -> RequestSendFunds {
        RequestSendFunds {
            request_id: self.request_id,
            route: self.route,
            invoice_id: self.invoice_id,
            dest_payment: self.dest_payment,
            freeze_links: Vec::new(),
        }
    }

    pub fn create_pending_request(&self) -> PendingFriendRequest {
        PendingFriendRequest {
            request_id: self.request_id,
            route: self.route.clone(),
            dest_payment: self.dest_payment,
            invoice_id: self.invoice_id.clone(),
        }
    }
}


#[derive(Debug, Clone)]
pub struct SetFriendRemoteMaxDebt {
    pub friend_public_key: PublicKey,
    pub remote_max_debt: u128,
}

#[derive(Debug, Clone)]
pub struct ResetFriendChannel {
    pub friend_public_key: PublicKey,
    pub current_token: Signature,
}

#[derive(Debug, Clone)]
pub struct SetFriendInfo<A> {
    pub friend_public_key: PublicKey,
    pub address: A,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct AddFriend<A> {
    pub friend_public_key: PublicKey,
    pub address: A,
    pub name: String,
    pub balance: i128, // Initial balance
}

#[derive(Debug, Clone)]
pub struct RemoveFriend {
    pub friend_public_key: PublicKey,
}

#[derive(Debug, Clone)]
pub struct SetFriendStatus {
    pub friend_public_key: PublicKey,
    pub status: FriendStatus,
}

#[derive(Debug, Clone)]
pub struct SetRequestsStatus {
    pub friend_public_key: PublicKey,
    pub status: RequestsStatus,
}


#[derive(Debug, Clone)]
pub struct ReceiptAck {
    pub request_id: Uid,
    pub receipt_hash: HashResult,
}

#[derive(Debug, Clone)]
pub enum IncomingControlMessage<A> {
    AddFriend(AddFriend<A>),
    RemoveFriend(RemoveFriend),
    SetRequestsStatus(SetRequestsStatus),
    SetFriendStatus(SetFriendStatus),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    SetFriendInfo(SetFriendInfo<A>),
    ResetFriendChannel(ResetFriendChannel),
    RequestSendFunds(UserRequestSendFunds),
    ReceiptAck(ReceiptAck),
}

#[derive(Debug, Clone)]
pub enum IncomingLivenessMessage {
    Online(PublicKey),
    Offline(PublicKey),
}

/// A `SendFundsReceipt` is received if a `RequestSendFunds` is successful.
/// It can be used a proof of payment for a specific `invoice_id`.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SendFundsReceipt {
    pub response_hash: HashResult,
    // = sha512/256(requestId || sha512/256(route) || randNonce)
    pub invoice_id: InvoiceId,
    pub dest_payment: u128,
    pub signature: Signature,
    // Signature{key=recipientKey}(
    //   "FUND_SUCCESS" ||
    //   sha512/256(requestId || sha512/256(route) || randNonce) ||
    //   invoiceId ||
    //   destPayment
    // )
}


impl SendFundsReceipt {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.response_hash);
        res_bytes.extend_from_slice(&self.invoice_id);
        res_bytes.write_u128::<BigEndian>(self.dest_payment).unwrap();
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FriendMoveTokenRequest {
    pub friend_move_token: FriendMoveToken,
    // Do we want the remote side to return the token:
    pub token_wanted: bool,
}

#[allow(unused)]
pub struct FriendInconsistencyError {
    pub reset_token: Signature,
    pub balance_for_reset: i128,
}


#[derive(Debug, Clone)]
pub enum FriendMessage {
    MoveTokenRequest(FriendMoveTokenRequest),
    InconsistencyError(ResetTerms),
}


#[derive(Debug)]
pub struct ResponseReceived {
    pub request_id: Uid,
    pub result: ResponseSendFundsResult,
}

#[derive(Debug)]
pub enum ChannelerConfig<A> {
    AddFriend((PublicKey, A)),
    RemoveFriend(PublicKey),
}

#[derive(Debug, Clone)]
pub enum IncomingCommMessage {
    Liveness(IncomingLivenessMessage),
    Friend((PublicKey, FriendMessage)),
}

/// An incoming message to the Funder:
#[derive(Clone, Debug)]
pub enum FunderIncoming<A> {
    Init,
    Control(IncomingControlMessage<A>),
    Comm(IncomingCommMessage),
}

#[allow(unused)]
#[derive(Debug)]
pub enum FunderOutgoing<A: Clone> {
    Control(FunderOutgoingControl<A>),
    Comm(FunderOutgoingComm<A>),
}

#[derive(Debug)]
pub enum FunderOutgoingControl<A: Clone> {
    ResponseReceived(ResponseReceived),
    Report(FunderReport<A>),
}

#[derive(Debug)]
pub enum FunderOutgoingComm<A> {
    FriendMessage((PublicKey, FriendMessage)),
    ChannelerConfig(ChannelerConfig<A>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetTerms {
    pub reset_token: Signature,
    pub inconsistency_counter: u64,
    pub balance_for_reset: i128,
}
