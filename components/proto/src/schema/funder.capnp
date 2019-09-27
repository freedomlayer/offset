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
        currenciesOperations @0: List(CurrencyOperations);
        # Operations that should be applied to various currencies.
        # For every currency, ordered batched operations are provided.
        # First operation should be applied first.
        optLocalRelays: union {
                empty @1: Void;
                # Nothing has changed
                relays @2: List(RelayAddress);
                # Set this exact list to be the list of relays
        }
        # Set the relays used by the sender of this MoveToken message.
        # (empty means no change happens).
        optAddActiveCurrencies: union {
                empty @3: Void;
                # Nothing has changed
                currency @4: List(Currency);
                # Add the given currencies
        }
        # Add a list of active currencies. (empty means that no change happens)
        # Note that this field only allows to add new currencies.
        infoHash @5: HashResult;
        # Current information about the channel that both sides implicitly agree upon.
        oldToken @6: Signature;
        # Token of the previous move token. This is a proof that we have
        # received the previous message before sending this one.
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

# Set the maximum possible debt for the remote party.
# Note: It is not possible to set a maximum debt smaller than the current debt
# This will cause an inconsistency.
# struct SetRemoteMaxDebtOp {
#         remoteMaxDebt @0: CustomUInt128;
# }

struct FriendsRoute {
        publicKeys @0: List(PublicKey);
        # A list of public keys
}

# A custom type for a rational 128 bit number.
struct Ratio128 {
        union {
                one @0: Void;
                numerator @1: CustomUInt128;
        }
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
        randNonce @2: RandValue;
        signature @3: Signature;
        # Signature{key=destinationKey}(
        #   sha512/256("FUNDS_RESPONSE") ||
        #   sha512/256(requestId || randNonce) ||
        #   srcHashedLock ||
        #   destHashedLock ||
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
                enableRequests @0: Void;
                disableRequests @1: Void;
                setRemoteMaxDebt @2: CustomUInt128;
                requestSendFunds @3: RequestSendFundsOp;
                responseSendFunds @4: ResponseSendFundsOp;
                cancelSendFunds @5: CancelSendFundsOp;
                collectSendFunds @6: CollectSendFundsOp;
        }
}
