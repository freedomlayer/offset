use byteorder::{WriteBytesExt, BigEndian};

use utils::int_convert::{usize_to_u64};

use crypto::identity::{PublicKey, Signature, verify_signature};
use crypto::uid::Uid;
use crypto::crypto_rand::RandValue;
use crypto::hash::{HashResult, sha_512_256};

use proto::funder::messages::{FriendsRoute, InvoiceId, 
    RequestSendFunds, FriendMoveToken, FriendMessage,
    FriendTcOp};

use identity::IdentityClient;

use super::report::{FunderReport, FunderReportMutation};

// Prefix used for chain hashing of token channel funds.
// NEXT is used for hashing for the next move token funds.
const TOKEN_NEXT: &[u8] = b"NEXT";


// pub const CHANNEL_TOKEN_LEN: usize = 32;
// define_fixed_bytes!(ChannelToken, CHANNEL_TOKEN_LEN);

// The hash of the previous message sent over the token channel.
// struct ChannelToken(Signature);


#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
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
pub struct PendingFriendRequest {
    pub request_id: Uid,
    pub route: FriendsRoute,
    pub dest_payment: u128,
    pub invoice_id: InvoiceId,
}

pub fn create_pending_request(request_send_funds: &RequestSendFunds) -> PendingFriendRequest {
    PendingFriendRequest {
        request_id: request_send_funds.request_id,
        route: request_send_funds.route.clone(),
        dest_payment: request_send_funds.dest_payment,
        invoice_id: request_send_funds.invoice_id.clone(),
    }
}


/// A request to send funds that originates from the user
#[derive(Debug, Clone)]
pub struct UserRequestSendFunds {
    pub request_id: Uid,
    pub route: FriendsRoute,
    pub invoice_id: InvoiceId,
    pub dest_payment: u128,
}


#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct FriendMoveTokenHashed {
    pub operations_hash: HashResult,
    pub old_token: Signature,
    pub inconsistency_counter: u64,
    pub move_token_counter: u128,
    pub balance: i128,
    pub local_pending_debt: u128,
    pub remote_pending_debt: u128,
    pub rand_nonce: RandValue,
    pub new_token: Signature,
}


pub async fn create_friend_move_token(operations: Vec<FriendTcOp>,
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

    let sig_buffer = signature_buff(&friend_move_token);
    friend_move_token.new_token = await!(identity_client.request_signature(sig_buffer)).unwrap();
    friend_move_token
}

/// Combine all operations into one hash value.
fn operations_hash(friend_move_token: &FriendMoveToken) -> HashResult {
    let mut operations_data = Vec::new();
    operations_data.write_u64::<BigEndian>(
        usize_to_u64(friend_move_token.operations.len()).unwrap()).unwrap();
    for op in &friend_move_token.operations {
        operations_data.extend_from_slice(&op.to_bytes());
    }
    sha_512_256(&operations_data)
}

fn signature_buff(friend_move_token: &FriendMoveToken) -> Vec<u8> {
    let mut sig_buffer = Vec::new();
    sig_buffer.extend_from_slice(&sha_512_256(TOKEN_NEXT));
    sig_buffer.extend_from_slice(&operations_hash(friend_move_token));
    sig_buffer.extend_from_slice(&friend_move_token.old_token);
    sig_buffer.write_u64::<BigEndian>(friend_move_token.inconsistency_counter).unwrap();
    sig_buffer.write_u128::<BigEndian>(friend_move_token.move_token_counter).unwrap();
    sig_buffer.write_i128::<BigEndian>(friend_move_token.balance).unwrap();
    sig_buffer.write_u128::<BigEndian>(friend_move_token.local_pending_debt).unwrap();
    sig_buffer.write_u128::<BigEndian>(friend_move_token.remote_pending_debt).unwrap();
    sig_buffer.extend_from_slice(&friend_move_token.rand_nonce);

    sig_buffer
}

/// Verify that new_token is a valid signature over the rest of the fields.
pub fn verify_friend_move_token(friend_move_token: &FriendMoveToken, public_key: &PublicKey) -> bool {
    let sig_buffer = signature_buff(friend_move_token);
    verify_signature(&sig_buffer, public_key, &friend_move_token.new_token)
}

/// Create a hashed version of the FriendMoveToken.
/// Hashed version contains the hash of the operations instead of the operations themselves,
/// hence it is usually shorter.
pub fn create_hashed(friend_move_token: &FriendMoveToken) -> FriendMoveTokenHashed {
    FriendMoveTokenHashed {
        operations_hash: operations_hash(friend_move_token),
        old_token: friend_move_token.old_token.clone(),
        inconsistency_counter: friend_move_token.inconsistency_counter,
        move_token_counter: friend_move_token.move_token_counter,
        balance: friend_move_token.balance,
        local_pending_debt: friend_move_token.local_pending_debt,
        remote_pending_debt: friend_move_token.remote_pending_debt,
        rand_nonce: friend_move_token.rand_nonce.clone(),
        new_token: friend_move_token.new_token.clone(),
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
    pub receipt_signature: Signature,
}

#[derive(Debug, Clone)]
pub enum FunderIncomingControl<A> {
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

#[allow(unused)]
pub struct FriendInconsistencyError {
    pub reset_token: Signature,
    pub balance_for_reset: i128,
}


#[derive(Debug)]
pub enum ResponseSendFundsResult {
    Success(SendFundsReceipt),
    Failure(PublicKey), // Reporting public key.
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
pub enum FunderIncomingComm {
    Liveness(IncomingLivenessMessage),
    Friend((PublicKey, FriendMessage)),
}

/// An incoming message to the Funder:
#[derive(Clone, Debug)]
pub enum FunderIncoming<A> {
    Init,
    Control(FunderIncomingControl<A>),
    Comm(FunderIncomingComm),
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
    ReportMutations(Vec<FunderReportMutation<A>>),
}

#[derive(Debug)]
pub enum FunderOutgoingComm<A> {
    FriendMessage((PublicKey, FriendMessage)),
    ChannelerConfig(ChannelerConfig<A>),
}
