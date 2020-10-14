@0xe7603b9ac00e2251;

using import "common.capnp".Signature;
using import "common.capnp".PublicKey;
using import "common.capnp".InvoiceId;
using import "common.capnp".Uid;
using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomInt128;
using import "common.capnp".RelayAddress;
using import "common.capnp".HashedLock;
using import "common.capnp".PlainLock;
using import "common.capnp".HashResult;
using import "common.capnp".HmacResult;
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
        removeRelays @2: List(PublicKey);
        # A list of relays to remove
        addRelays @3: List(RelayAddress);
        # A list of relays to add
        # Should have no intersection with the removeRelays list.
        # TODO: We might be able to have a similar xor based diff here if we use
        # a map, mapping PublicKey to NetAddress.
        currenciesDiff @4: List(Currency);
        # Exclusive-Or difference of previous list of currencies and new list of currencies.
        # Should be empty if nothing has changed.
        infoHash @5: HashResult;
        # Current information about the channel that both sides implicitly agree upon.
        newToken @6 : Signature;
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
        invoiceHash @5: HashResult;
        # A hash of the contents of the invoice (Including text etc)
        # Possibly a hash over a random invoiceId, and text hash
        hmac @6: HmacResult;
        # An HMAC signature over the whole message, not including "leftFees"
        # and "route" (as they change when the message is passed).
        # The shared secret for the HMAC algorithm was received through
        # "direct" relay communication with the seller.
        leftFees @7: CustomUInt128;
        # Amount of fees left to give to mediators
        # Every mediator takes the amount of fees he wants and subtracts this
        # value accordingly.
}

struct ResponseSendFundsOp {
        requestId @0: Uid;
        srcPlainLock @1: PlainLock;
        serialNum @2: CustomUInt128;
        # Serial number used for this collection of invoice money.
        # This should be a u128 counter, increased by 1 for every collected
        # invoice.
        signature @3: Signature;
        # Signature{key=destinationKey}(
        #   sha512/256("FUNDS_RESPONSE") ||
        #   sha512/256(requestId || hmac || srcPlainLock || destPayment)
        #   serialNum ||
        #   totalDestPayment ||
        #   invoiceHash ||
        #   currency [Implicitly known by the mutual credit]
        # )
}

struct CancelSendFundsOp {
        requestId @0: Uid;
}


struct FriendTcOp {
        union {
                requestSendFunds @0: RequestSendFundsOp;
                responseSendFunds @1: ResponseSendFundsOp;
                cancelSendFunds @2: CancelSendFundsOp;
        }
}
