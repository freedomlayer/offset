
use crypto::identity::{PublicKey, Signature};
use crypto::uid::Uid;
use crypto::crypto_rand::RandValue;
use crypto::hash::HashResult;

use proto::funder::messages::{FriendsRoute, InvoiceId, 
    RequestSendFunds, MoveToken, FriendMessage,
    FriendTcOp, PendingRequest, SendFundsReceipt};

use proto::funder::signature_buff::{operations_hash, 
    friend_move_token_signature_buff};

use identity::IdentityClient;

use proto::report::messages::{FunderReport, FunderReportMutation};



/// Keep information from a RequestSendFunds message.
/// This information will be used later to deal with a corresponding {Response,Failure}SendFunds messages,
/// as those messages do not repeat the information sent in the request.
pub fn create_pending_request(request_send_funds: &RequestSendFunds) -> PendingRequest {
    PendingRequest {
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

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct MoveTokenHashed {
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
                 identity_client: IdentityClient) -> MoveToken {

    let mut friend_move_token = MoveToken {
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

    let sig_buffer = friend_move_token_signature_buff(&friend_move_token);
    friend_move_token.new_token = await!(identity_client.request_signature(sig_buffer)).unwrap();
    friend_move_token
}


/// Create a hashed version of the MoveToken.
/// Hashed version contains the hash of the operations instead of the operations themselves,
/// hence it is usually shorter.
pub fn create_hashed(friend_move_token: &MoveToken) -> MoveTokenHashed {
    MoveTokenHashed {
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

    pub fn create_pending_request(&self) -> PendingRequest {
        PendingRequest {
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
    /// Set relay address used for the local node
    SetAddress(Option<A>),
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
    /// Set relay address for local node
    /// This is the address the Channeler will connect to 
    /// and listen for new connections
    SetAddress(Option<A>),
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
