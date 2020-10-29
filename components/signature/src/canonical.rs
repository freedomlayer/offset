use byteorder::{BigEndian, WriteBytesExt};
// use std::collections::HashMap;

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{
    CancelSendFundsOp, Currency, CurrencyOperations, FriendTcOp, FriendsRoute, McBalance, Receipt,
    RequestSendFundsOp, ResponseSendFundsOp,
};
use proto::index_server::messages::{IndexMutation, RemoveFriendCurrency, UpdateFriendCurrency};
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

impl CanonicalSerialize for bool {
    fn canonical_serialize(&self) -> Vec<u8> {
        if *self {
            vec![1]
        } else {
            vec![0]
        }
    }
}

impl CanonicalSerialize for u32 {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        res_data.write_u32::<BigEndian>(*self).unwrap();
        res_data
    }
}

impl CanonicalSerialize for u64 {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        res_data.write_u64::<BigEndian>(*self).unwrap();
        res_data
    }
}

impl CanonicalSerialize for u128 {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        res_data.write_u128::<BigEndian>(*self).unwrap();
        res_data
    }
}

impl CanonicalSerialize for i128 {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        res_data.write_i128::<BigEndian>(*self).unwrap();
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

/*
impl<K, V> CanonicalSerialize for HashMap<K, V>
where
    K: CanonicalSerialize + Ord + Clone,
    V: CanonicalSerialize + Clone,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut vec: Vec<(K, V)> = self.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        vec.sort_by(|(k1, v1), (k2, v2)| k1.cmp(k2));
        vec.canonical_serialize()
    }
}
*/

impl CanonicalSerialize for Currency {
    fn canonical_serialize(&self) -> Vec<u8> {
        self.as_str().canonical_serialize()
    }
}

impl CanonicalSerialize for CurrencyOperations {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.currency.canonical_serialize());
        res_bytes.extend_from_slice(&self.operations.canonical_serialize());
        res_bytes
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
        res_bytes
            .write_u128::<BigEndian>(self.total_dest_payment)
            .unwrap();
        res_bytes.extend_from_slice(&self.invoice_hash);
        res_bytes.extend_from_slice(&self.hmac);
        res_bytes.write_u128::<BigEndian>(self.left_fees).unwrap();
        res_bytes
    }
}

impl CanonicalSerialize for ResponseSendFundsOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.src_plain_lock);
        res_bytes.write_u128::<BigEndian>(self.serial_num).unwrap();
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

impl CanonicalSerialize for FriendTcOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            FriendTcOp::RequestSendFunds(request_send_funds) => {
                res_bytes.push(0u8);
                res_bytes.append(&mut request_send_funds.canonical_serialize())
            }
            FriendTcOp::ResponseSendFunds(response_send_funds) => {
                res_bytes.push(1u8);
                res_bytes.append(&mut response_send_funds.canonical_serialize())
            }
            FriendTcOp::CancelSendFunds(cancel_send_funds) => {
                res_bytes.push(2u8);
                res_bytes.append(&mut cancel_send_funds.canonical_serialize())
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

impl CanonicalSerialize for UpdateFriendCurrency {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.public_key);
        res_bytes.extend_from_slice(&self.currency.canonical_serialize());
        res_bytes
            .write_u128::<BigEndian>(self.recv_capacity)
            .unwrap();
        res_bytes
    }
}

impl CanonicalSerialize for RemoveFriendCurrency {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.public_key);
        res_bytes.extend_from_slice(&self.currency.canonical_serialize());
        res_bytes
    }
}

impl CanonicalSerialize for IndexMutation {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            IndexMutation::UpdateFriendCurrency(update_friend_currency) => {
                res_bytes.push(0);
                res_bytes.extend(update_friend_currency.canonical_serialize());
            }
            IndexMutation::RemoveFriendCurrency(remove_friend_currency) => {
                res_bytes.push(1);
                res_bytes.extend(remove_friend_currency.canonical_serialize());
            }
        };
        res_bytes
    }
}

/*
impl CanonicalSerialize for BalanceInfo {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.balance.canonical_serialize());
        res_bytes.extend_from_slice(&self.local_pending_debt.canonical_serialize());
        res_bytes.extend_from_slice(&self.remote_pending_debt.canonical_serialize());
        res_bytes
    }
}

impl CanonicalSerialize for CurrencyBalanceInfo {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.currency.canonical_serialize());
        res_bytes.extend_from_slice(&self.balance_info.canonical_serialize());
        res_bytes
    }
}
*/

/*
impl CanonicalSerialize for McInfo {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.local_public_key);
        res_bytes.extend_from_slice(&self.remote_public_key);
        res_bytes.extend_from_slice(&self.balances.canonical_serialize());
        res_bytes
    }
}

impl CanonicalSerialize for CountersInfo {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.inconsistency_counter.canonical_serialize());
        res_bytes.extend_from_slice(&self.move_token_counter.canonical_serialize());
        res_bytes
    }
}

*/

impl CanonicalSerialize for McBalance {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.write_i128::<BigEndian>(self.balance).unwrap();
        res_bytes
            .write_u128::<BigEndian>(self.local_pending_debt)
            .unwrap();
        res_bytes
            .write_u128::<BigEndian>(self.remote_pending_debt)
            .unwrap();

        // Write in/out fees as big endian:
        let mut temp_array = [0u8; 16];
        self.in_fees.to_big_endian(&mut temp_array);
        res_bytes.extend_from_slice(&temp_array);
        self.out_fees.to_big_endian(&mut temp_array);
        res_bytes.extend_from_slice(&temp_array);

        res_bytes
    }
}
/*
impl CanonicalSerialize for TokenInfo {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.local_public_key);
        res_bytes.extend_from_slice(&self.remote_public_key);
        res_bytes.extend_from_slice(&self.balances_hash);
        res_bytes
            .write_u128::<BigEndian>(self.move_token_counter)
            .unwrap();
        res_bytes
    }
}
*/
