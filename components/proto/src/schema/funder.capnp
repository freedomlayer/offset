@0xe7603b9ac00e2251;

using import "common.capnp".Signature;
using import "common.capnp".PublicKey;
using import "common.capnp".RandValue;
using import "common.capnp".InvoiceId;
using import "common.capnp".Uid;
using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomInt128;
using import "common.capnp".RelayAddress;
using import "common.capnp".HashedLock;
using import "common.capnp".PlainLock;
using import "common.capnp".HashResult;
using import "common.capnp".Currency;


# Token channel messages
# ----------------------

# Operations to be applied to a specific currency
struct CurrencyOperations {
        currency @0: Currency;
        # Name of the currency
        operations @1: List(FriendTcOp);
        # Operations to be applied for a specific currency
        # The operations must be applied in the given order.
}

struct MoveToken {
        oldToken @0: Signature;
        # Token of the previous move token. This is a proof that we have
        # received the previous message before sending this one.
        currenciesOperations @1: List(CurrencyOperations);
        # Operations that should be applied to various currencies.
        # For every currency, ordered batched operations are provided.
        # First operation should be applied first.
        optLocalRelays: union {
                empty @2: Void;
                # Nothing has changed
                relays @3: List(RelayAddress);
                # Set this exact list to be the list of relays
        }
        # Set the relays used by the sender of this MoveToken message.
        # (Empty means no change happens).
        optActiveCurrencies: union {
                empty @4: Void;
                # Nothing has changed
                currencies @5: List(Currency);
                # Set this exact list to be the list of currencies
        }
        # Add a list of active currencies. (empty means that no change happens)
        # Note that this field only allows to add new currencies.
        infoHash @6: HashResult;
        # Current information about the channel that both sides implicitly agree upon.
        randNonce @7: RandValue;
        # A random nonce, generated by the sender. We have it because the
        # sender is signing over this message, and we don't want him to be
        # tricked into signing over something strange.
        newToken @8 : Signature;
        # A signature over all the previous fields.
}

struct MoveTokenRequest {
        moveToken @0: MoveToken;
        tokenWanted @1: Bool;
}

# A pair of currency and balance
struct CurrencyBalance {
        currency @0: Currency;
        balance @1: CustomInt128;
}

struct ResetTerms {
        resetToken @0: Signature;
        inconsistencyCounter @1: UInt64;
        balanceForReset @2: List(CurrencyBalance);
        # List of expected balance for each currency
}


# A message sent between friends.
struct FriendMessage {
        union {
                moveTokenRequest @0: MoveTokenRequest;
                inconsistencyError @1: ResetTerms;
        }
}




# Token Operations
# ------------------

struct FriendsRoute {
        publicKeys @0: List(PublicKey);
        # A list of public keys
}

struct RequestSendFundsOp {
        requestId @0: Uid;
        # Id number of this request. Used to identify the whole transaction
        # over this route.
        srcHashedLock @1: HashedLock;
        # A hash lock created by the originator of this request
        route @2: FriendsRoute;
        destPayment @3: CustomUInt128;
        totalDestPayment @4: CustomUInt128;
        invoiceId @5: InvoiceId;
        # Id number of the invoice we are attempting to pay
        leftFees @6: CustomUInt128;
        # Amount of fees left to give to mediators
        # Every mediator takes the amount of fees he wants and subtracts this
        # value accordingly.
}

struct ResponseSendFundsOp {
        requestId @0: Uid;
        destHashedLock @1: HashedLock;
        isComplete @2: Bool;
        # Has the destination received all the funds he asked for at the invoice?
        # Mostly meaningful in the case of multi-path payments.
        # isComplete == True means that no more requests should be sent.
        # The isComplete field is crucial for the construction of a Commit message.
        randNonce @3: RandValue;
        signature @4: Signature;
        # Signature{key=destinationKey}(
        #   sha512/256("FUNDS_RESPONSE") ||
        #   sha512/256(requestId || randNonce) ||
        #   srcHashedLock ||
        #   destHashedLock ||
        #   isComplete ||
        #   destPayment ||
        #   totalDestPayment ||
        #   invoiceId ||
        #   currency [Implicitly known by the mutual credit]
        # )
}

struct CancelSendFundsOp {
        requestId @0: Uid;
}

struct CollectSendFundsOp {
        requestId @0: Uid;
        srcPlainLock @1: PlainLock;
        destPlainLock @2: PlainLock;
}


struct FriendTcOp {
        union {
                requestSendFunds @0: RequestSendFundsOp;
                responseSendFunds @1: ResponseSendFundsOp;
                cancelSendFunds @2: CancelSendFundsOp;
                collectSendFunds @3: CollectSendFundsOp;
        }
}
