@0x8bc829b5200f3c7f;

using import "common.capnp".PublicKey;
using import "common.capnp".Hash;
using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomInt128;
using import "common.capnp".Signature;
using import "common.capnp".RandNonce;
using import "common.capnp".Rate;

using import "common.capnp".RelayAddress;
using import "common.capnp".NamedRelayAddress;
using import "common.capnp".NamedIndexServerAddress;
using import "common.capnp".NetAddress;

## Report related structs
#########################

struct MoveTokenHashedReport {
        prefixHash @0: Hash;
        localPublicKey @1: PublicKey;
        remotePublicKey @2: PublicKey;
        inconsistencyCounter @3: UInt64;
        moveTokenCounter @4: CustomUInt128;
        balance @5: CustomInt128;
        localPendingDebt @6: CustomUInt128;
        remotePendingDebt @7: CustomUInt128;
        randNonce @8: RandNonce;
        newToken @9: Signature;
}


struct FriendStatusReport {
        union {
                disabled @0: Void;
                enabled @1: Void;
        }
}

struct RequestsStatusReport {
        union {
                closed @0: Void;
                open @1: Void;
        }
}

struct FriendLivenessReport {
        union {
                offline @0: Void;
                online @1: Void;
        }
}

struct DirectionReport {
        union {
                incoming @0: Void;
                outgoing @1: Void;
        }
}

struct McRequestsStatusReport {
        local @0: RequestsStatusReport;
        remote @1: RequestsStatusReport;
}

struct McBalanceReport {
    balance @0: CustomInt128;
    # Amount of credits this side has against the remote side.
    # The other side keeps the negation of this value.
    localMaxDebt @2: CustomUInt128;
    # Maximum possible local debt
    remoteMaxDebt @1: CustomUInt128;
    # Maximum possible remote debt
    localPendingDebt @3: CustomUInt128;
    # Frozen credits by our side
    remotePendingDebt @4: CustomUInt128;
    # Frozen credits by the remote side
}

struct TcReport {
        direction @0: DirectionReport;
        balance @1: McBalanceReport;
        requestsStatus @2: McRequestsStatusReport;
        numLocalPendingRequests @3: UInt64;
        numRemotePendingRequests @4: UInt64;
}

struct ResetTermsReport {
        resetToken @0: Signature;
        balanceForReset @1: CustomInt128;
}

struct ChannelInconsistentReport {
        localResetTermsBalance @0: CustomInt128;
        optRemoteResetTerms: union {
                remoteResetTerms @1: ResetTermsReport;
                empty @2: Void;
        }
}


struct ChannelStatusReport {
        union {
                inconsistent @0: ChannelInconsistentReport;
                consistent @1: TcReport;
        }
}

struct OptLastIncomingMoveToken {
        union {
                moveTokenHashed @0: MoveTokenHashedReport;
                empty @1: Void;
        }
}

struct RelaysTransition {
        lastSent @0: List(NamedRelayAddress);
        beforeLastSent @1: List(NamedRelayAddress);
}

struct SentLocalRelaysReport {
        union {
                neverSent @0: Void;
                transition @1: RelaysTransition;
                lastSent @2: List(NamedRelayAddress);
        }
}

struct FriendReport {
        name @0: Text;
        rate @1: Rate;
        remoteRelays @2: List(RelayAddress);
        sentLocalRelays @3: SentLocalRelaysReport;
        optLastIncomingMoveToken @4: OptLastIncomingMoveToken;
        liveness @5: FriendLivenessReport;
        channelStatus @6: ChannelStatusReport;
        wantedRemoteMaxDebt @7: CustomUInt128;
        wantedLocalRequestsStatus @8: RequestsStatusReport;
        numPendingRequests @9: UInt64;
        numPendingBackwardsOps @10: UInt64;
        status @11: FriendStatusReport;
        numPendingUserRequests @12: UInt64;
}

struct PkFriendReport {
        friendPublicKey @0: PublicKey;
        friendReport @1: FriendReport;
}

# A full Funder report.
struct FunderReport {
        localPublicKey @0: PublicKey;
        relays @1: List(NamedRelayAddress);
        friends @2: List(PkFriendReport);
        numOpenInvoices @3: UInt64;
        numPayments @4: UInt64;
        numOpenTransactions @5: UInt64;
}


############################################################################
############################################################################

struct AddFriendReport {
        friendPublicKey @0: PublicKey;
        name @2: Text;
        relays @1: List(RelayAddress);
        balance @3: CustomInt128;
        optLastIncomingMoveToken @4: OptLastIncomingMoveToken;
        channelStatus @5: ChannelStatusReport;
}

struct FriendReportMutation {
        union {
                setRemoteRelays @0: List(RelayAddress);
                setName @1: Text;
                setRate @2: Rate;
                setSentLocalRelays @3: SentLocalRelaysReport;
                setChannelStatus @4: ChannelStatusReport;
                setWantedRemoteMaxDebt @5: CustomUInt128;
                setWantedLocalRequestsStatus @6: RequestsStatusReport;
                setNumPendingRequests @7: UInt64;
                setNumPendingBackwardsOps @8: UInt64;
                setStatus @9: FriendStatusReport;
                setNumPendingUserRequests @10: UInt64;
                setOptLastIncomingMoveToken @11: OptLastIncomingMoveToken;
                setLiveness @12: FriendLivenessReport;
        }
}

struct PkFriendReportMutation {
        friendPublicKey @0: PublicKey;
        friendReportMutation @1: FriendReportMutation;
}

# A FunderReportMutation. Could be applied over a FunderReport to make small changes.
struct FunderReportMutation {
        union {
                addRelay @0: NamedRelayAddress;
                removeRelay @1: PublicKey;
                addFriend @2: AddFriendReport;
                removeFriend @3: PublicKey;
                pkFriendReportMutation @4: PkFriendReportMutation;
                setNumOpenInvoices @5: UInt64;
                setNumPayments @6: UInt64;
                setNumOpenTransactions @7: UInt64;
        }
}


############################################################################
##### IndexClient report
############################################################################

struct IndexClientReport {
        indexServers @0: List(NamedIndexServerAddress);
        optConnectedServer: union {
                publicKey @1: PublicKey;
                empty @2: Void;
        }
}

struct IndexClientReportMutation {
        union {
                addIndexServer @0: NamedIndexServerAddress;
                removeIndexServer @1: PublicKey;
                setConnectedServer: union {
                        publicKey @2: PublicKey;
                        empty @3: Void;
                }
        }
}


############################################################################
##### Node report
############################################################################

struct NodeReport {
        funderReport @0: FunderReport;
        indexClientReport @1: IndexClientReport;
}

struct NodeReportMutation {
        union {
                funder @0: FunderReportMutation;
                indexClient @1: IndexClientReportMutation;
        }
}
