use im::hashmap::HashMap as ImHashMap;
use im::vector::Vector as ImVec;

use common::canonical_serialize::CanonicalSerialize;
use crypto::hash_lock::PlainLock;
use crypto::identity::PublicKey;
use crypto::invoice_id::InvoiceId;
use crypto::payment_id::PaymentId;
use crypto::uid::Uid;

use proto::app_server::messages::NamedRelayAddress;
use proto::funder::messages::{AddFriend, Receipt};

use crate::friend::{FriendMutation, FriendState};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FunderState<B: Clone> {
    /// Public key of this node
    pub local_public_key: PublicKey,
    /// Addresses of relays we are going to connect to.
    pub relays: ImVec<NamedRelayAddress<B>>,
    /// All configured friends and their state
    pub friends: ImHashMap<PublicKey, FriendState<B>>,
    /// Locally issued invoices in progress (For which this node is the seller)
    pub open_invoices: ImHashMap<InvoiceId, OpenInvoice>,
    /// Locally created transaction in progress. (For which this node is the buyer).
    pub open_transactions: ImHashMap<Uid, OpenTransaction>,
    /// Ongoing payments (For which this node is the buyer):
    pub open_payments: ImHashMap<PaymentId, OpenPayment>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum ReceiptStatus {
    /// Haven't got a receipt yet
    Empty,
    /// We Have a pending receipt, waiting to be handed to the user.
    Pending(Receipt),
    /// A receipt was already received and handed over to user.
    Taken,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OpenPayment {
    /// Is in the process of being closed?
    /// Can only be removed if (num_transactions == 0) && receipt_status != ReceiptStatus::Pending(Receipt)
    pub is_closing: bool,
    /// Remote invoice id being paid
    pub invoice_id: InvoiceId,
    /// The amount of ongoing transactions for this payment.
    pub num_transactions: u64,
    /// Total amount of credits we want to pay to seller.
    pub total_dest_payment: u128,
    /// Seller's public key:
    pub dest_public_key: PublicKey,
    /// The first receipt that was received for this payment. (Generated after a CollectSendFundsOp
    /// message was received successfuly).
    pub receipt_status: ReceiptStatus,
}

// TODO: If a receipt is requested and OpenPayment.num_transactions == 0, it should reported that no receipt
// exists and the payment should be removed.

/// A local invoice in progress
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OpenInvoice {
    /// Total payment required to fulfill this invoice:
    pub total_dest_payment: u128,
    /// Destination plain locks for all requests related to a single open invoice that was
    /// originated for this node.
    /// Multiple requests are possible for a single invoice in case of a multi-route payment.
    pub dest_plain_locks: ImHashMap<Uid, PlainLock>,
}

impl OpenInvoice {
    pub fn new(total_dest_payment: u128) -> Self {
        OpenInvoice {
            total_dest_payment,
            dest_plain_locks: ImHashMap::new(),
        }
    }
}

/// A local request (Originated from this node) in progress
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OpenTransaction {
    pub payment_id: PaymentId,
    /// The plain part of a hash lock for the generated transaction.
    pub src_plain_lock: PlainLock,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunderMutation<B: Clone> {
    FriendMutation((PublicKey, FriendMutation<B>)),
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
    AddFriend(AddFriend<B>),
    RemoveFriend(PublicKey),
    AddInvoice((InvoiceId, u128)), // (InvoiceId, total_dest_payment)
    AddDestPlainLock((InvoiceId, Uid, PlainLock)), // InvoiceId, RequestId, dest_plain_lock
    RemoveInvoice(InvoiceId),
    AddTransaction((Uid, PaymentId, PlainLock)), // (transaction_id, payment_id,src_plain_lock)
    RemoveTransaction(Uid),                      // transaction_id
    AddPayment((PaymentId, InvoiceId, u128, PublicKey)), // (payment_id, invoice_id, total_dest_payment, destination)
    SetPaymentReceipt((PaymentId, Receipt)),
    TakePaymentReceipt(PaymentId),
    SetPaymentClosing(PaymentId),
    SetPaymentNumTransactions((PaymentId, u64)), // (payment_id, num_transactions)
    RemovePayment(PaymentId),
}

impl<B> FunderState<B>
where
    B: Clone + CanonicalSerialize,
{
    pub fn new(local_public_key: PublicKey, relays: Vec<NamedRelayAddress<B>>) -> Self {
        // Convert relays into a map:
        let relays = relays.into_iter().collect();

        FunderState {
            local_public_key,
            relays,
            friends: ImHashMap::new(),
            open_invoices: ImHashMap::new(),
            open_transactions: ImHashMap::new(),
            open_payments: ImHashMap::new(),
        }
    }

    // TODO: Use MutableState trait instead:
    pub fn mutate(&mut self, funder_mutation: &FunderMutation<B>) {
        match funder_mutation {
            FunderMutation::FriendMutation((public_key, friend_mutation)) => {
                let friend = self.friends.get_mut(&public_key).unwrap();
                friend.mutate(friend_mutation);
            }
            FunderMutation::AddRelay(named_relay_address) => {
                // Check for duplicates:
                self.relays.retain(|cur_named_relay_address| {
                    cur_named_relay_address.public_key != named_relay_address.public_key
                });
                self.relays.push_back(named_relay_address.clone());
                // TODO: Should check here if we have more than a constant amount of relays
            }
            FunderMutation::RemoveRelay(public_key) => {
                self.relays.retain(|cur_named_relay_address| {
                    &cur_named_relay_address.public_key != public_key
                });
            }
            FunderMutation::AddFriend(add_friend) => {
                let friend = FriendState::new(
                    &self.local_public_key,
                    &add_friend.friend_public_key,
                    add_friend.relays.clone(),
                    add_friend.name.clone(),
                    add_friend.balance,
                );
                // Insert friend, but also make sure that we didn't override an existing friend
                // with the same public key:
                let res = self
                    .friends
                    .insert(add_friend.friend_public_key.clone(), friend);
                assert!(res.is_none());
            }
            FunderMutation::RemoveFriend(public_key) => {
                let _ = self.friends.remove(&public_key);
            }
            FunderMutation::AddInvoice((invoice_id, total_dest_payment)) => {
                self.open_invoices
                    .insert(invoice_id.clone(), OpenInvoice::new(*total_dest_payment));
            }
            FunderMutation::AddDestPlainLock((invoice_id, request_id, plain_lock)) => {
                let open_invoice = self.open_invoices.get_mut(invoice_id).unwrap();
                open_invoice
                    .dest_plain_locks
                    .insert(request_id.clone(), plain_lock.clone());
            }
            FunderMutation::RemoveInvoice(invoice_id) => {
                let _ = self.open_invoices.remove(invoice_id);
            }
            FunderMutation::AddTransaction((transaction_id, payment_id, src_plain_lock)) => {
                let open_transaction = OpenTransaction {
                    payment_id: payment_id.clone(),
                    src_plain_lock: src_plain_lock.clone(),
                };
                let _ = self
                    .open_transactions
                    .insert(transaction_id.clone(), open_transaction);
            }
            FunderMutation::RemoveTransaction(transaction_id) => {
                let _ = self.open_transactions.remove(transaction_id);
            }
            FunderMutation::AddPayment((
                payment_id,
                invoice_id,
                total_dest_payment,
                dest_public_key,
            )) => {
                let open_payment = OpenPayment {
                    is_closing: false,
                    invoice_id: invoice_id.clone(),
                    num_transactions: 0,
                    total_dest_payment: *total_dest_payment,
                    dest_public_key: dest_public_key.clone(),
                    receipt_status: ReceiptStatus::Empty,
                };
                let _ = self.open_payments.insert(payment_id.clone(), open_payment);
            }
            FunderMutation::SetPaymentReceipt((payment_id, receipt)) => {
                let open_payment = self.open_payments.get_mut(payment_id).unwrap();
                if let ReceiptStatus::Empty = open_payment.receipt_status {
                    open_payment.receipt_status = ReceiptStatus::Pending(receipt.clone());
                } else {
                    unreachable!();
                }
            }
            FunderMutation::TakePaymentReceipt(payment_id) => {
                let open_payment = self.open_payments.get_mut(payment_id).unwrap();
                if let ReceiptStatus::Pending(_) = &open_payment.receipt_status {
                    open_payment.receipt_status = ReceiptStatus::Taken;
                } else {
                    unreachable!();
                }
            }
            FunderMutation::SetPaymentClosing(payment_id) => {
                self.open_payments.get_mut(payment_id).unwrap().is_closing = true;
            }
            FunderMutation::SetPaymentNumTransactions((payment_id, num_transactions)) => {
                self.open_payments
                    .get_mut(payment_id)
                    .unwrap()
                    .num_transactions = *num_transactions;
            }
            FunderMutation::RemovePayment(payment_id) => {
                self.open_payments.remove(payment_id);
            }
        }
    }
}
