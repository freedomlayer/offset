use byteorder::{BigEndian, WriteBytesExt};

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{
    CancelSendFundsOp, CollectSendFundsOp, FriendTcOp, FriendsRoute, OptLocalRelays, Receipt,
    RequestSendFundsOp, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, UpdateFriend};
use proto::net::messages::NetAddress;

use common::int_convert::usize_to_u64;

/// Canonically serialize an object
/// This serialization is used for security related applications (For example, signatures and
/// hashing), therefore the serialization result must be the same on any system.
pub trait CanonicalSerialize {
    fn canonical_serialize(&self) -> Vec<u8>;
}

impl<T> CanonicalSerialize for Option<T>
where
    T: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        match &self {
            None => {
                res_data.push(0);
            }
            Some(t) => {
                res_data.push(1);
                res_data.extend_from_slice(&t.canonical_serialize());
            }
        };
        res_data
    }
}

impl<T> CanonicalSerialize for Vec<T>
where
    T: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        // Write length:
        res_data
            .write_u64::<BigEndian>(usize_to_u64(self.len()).unwrap())
            .unwrap();
        // Write all items:
        for t in self.iter() {
            res_data.extend_from_slice(&t.canonical_serialize());
        }
        res_data
    }
}

impl CanonicalSerialize for String {
    fn canonical_serialize(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl CanonicalSerialize for &str {
    fn canonical_serialize(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

// Used mostly for testing:
impl CanonicalSerialize for u32 {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        res_data.write_u32::<BigEndian>(*self).unwrap();
        res_data
    }
}

impl<T, W> CanonicalSerialize for (T, W)
where
    T: CanonicalSerialize,
    W: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let (t, w) = self;
        let mut res_data = Vec::new();
        res_data.extend_from_slice(&t.canonical_serialize());
        res_data.extend_from_slice(&w.canonical_serialize());
        res_data
    }
}

impl CanonicalSerialize for RequestSendFundsOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.src_hashed_lock);
        res_bytes.extend_from_slice(&self.route.canonical_serialize());
        res_bytes
            .write_u128::<BigEndian>(self.dest_payment)
            .unwrap();
        res_bytes.extend_from_slice(&self.invoice_id);
        // We do not sign over`left_fees`, because this field changes as the request message is
        // forwarded.
        // res_bytes.write_u128::<BigEndian>(self.left_fees).unwrap();
        res_bytes
    }
}

impl CanonicalSerialize for ResponseSendFundsOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.dest_hashed_lock);
        res_bytes.extend_from_slice(&self.rand_nonce);
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}

impl CanonicalSerialize for CancelSendFundsOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes
    }
}

impl CanonicalSerialize for CollectSendFundsOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.src_plain_lock);
        res_bytes.extend_from_slice(&self.dest_plain_lock);
        res_bytes
    }
}

impl CanonicalSerialize for FriendTcOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            FriendTcOp::EnableRequests => {
                res_bytes.push(0u8);
            }
            FriendTcOp::DisableRequests => {
                res_bytes.push(1u8);
            }
            FriendTcOp::SetRemoteMaxDebt(remote_max_debt) => {
                res_bytes.push(2u8);
                res_bytes.write_u128::<BigEndian>(*remote_max_debt).unwrap();
            }
            FriendTcOp::RequestSendFunds(request_send_funds) => {
                res_bytes.push(3u8);
                res_bytes.append(&mut request_send_funds.canonical_serialize())
            }
            FriendTcOp::ResponseSendFunds(response_send_funds) => {
                res_bytes.push(4u8);
                res_bytes.append(&mut response_send_funds.canonical_serialize())
            }
            FriendTcOp::CancelSendFunds(cancel_send_funds) => {
                res_bytes.push(5u8);
                res_bytes.append(&mut cancel_send_funds.canonical_serialize())
            }
            FriendTcOp::CollectSendFunds(commit_send_funds) => {
                res_bytes.push(6u8);
                res_bytes.append(&mut commit_send_funds.canonical_serialize())
            }
        }
        res_bytes
    }
}

impl CanonicalSerialize for FriendsRoute {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes
            .write_u64::<BigEndian>(usize_to_u64(self.public_keys.len()).unwrap())
            .unwrap();
        for public_key in &self.public_keys {
            res_bytes.extend_from_slice(public_key);
        }
        res_bytes
    }
}

impl CanonicalSerialize for Receipt {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.response_hash);
        res_bytes.extend_from_slice(&self.invoice_id);
        res_bytes
            .write_u128::<BigEndian>(self.dest_payment)
            .unwrap();
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}

impl CanonicalSerialize for NetAddress {
    fn canonical_serialize(&self) -> Vec<u8> {
        self.as_str().canonical_serialize()
    }
}

impl<B> CanonicalSerialize for RelayAddress<B>
where
    B: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.public_key);
        res_bytes.extend_from_slice(&self.address.canonical_serialize());
        res_bytes
    }
}

impl<B> CanonicalSerialize for OptLocalRelays<B>
where
    B: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            OptLocalRelays::Empty => res_bytes.push(0u8),
            OptLocalRelays::Relays(relays) => {
                res_bytes.push(1u8);
                res_bytes.append(&mut relays.canonical_serialize());
            }
        };
        res_bytes
    }
}

impl CanonicalSerialize for UpdateFriend {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.public_key);
        res_bytes
            .write_u128::<BigEndian>(self.send_capacity)
            .unwrap();
        res_bytes
            .write_u128::<BigEndian>(self.recv_capacity)
            .unwrap();
        res_bytes
    }
}

impl CanonicalSerialize for IndexMutation {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            IndexMutation::UpdateFriend(update_friend) => {
                res_bytes.push(0);
                res_bytes.extend(update_friend.canonical_serialize());
            }
            IndexMutation::RemoveFriend(public_key) => {
                res_bytes.push(1);
                res_bytes.extend_from_slice(public_key);
            }
        };
        res_bytes
    }
}
