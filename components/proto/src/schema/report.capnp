@0x8bc829b5200f3c7f;

using import "common.capnp".PublicKey;
using import "common.capnp".HashResult;
using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomInt128;
using import "common.capnp".Signature;
using import "common.capnp".RandValue;
using import "common.capnp".Rate;
using import "common.capnp".Currency;
using import "common.capnp".RelayAddress;
using import "common.capnp".NamedRelayAddress;
using import "common.capnp".NamedIndexServerAddress;

using import "funder.capnp".CurrencyBalance;

## Report related structs
#########################

struct CountersInfo {
        inconsistencyCounter @0: UInt64;
        moveTokenCounter @1: CustomUInt128;
}

struct BalanceInfo {
        balance @0: CustomInt128;
        localPendingDebt @1: CustomUInt128;
        remotePendingDebt @2: CustomUInt128;
}

struct CurrencyBalanceInfo {
        currency @0: Currency;
        balanceInfo @1: BalanceInfo;
}

struct McInfo {
        localPublicKey @0: PublicKey;
        remotePublicKey @1: PublicKey;
        balances @2: List(CurrencyBalanceInfo);
}


struct TokenInfo {
        mc @0: McInfo;
        counters @1: CountersInfo;
}

struct MoveTokenHashedReport {
        prefixHash @0: HashResult;
        tokenInfo @1: TokenInfo;
        randNonce @2: RandValue;
        newToken @3: Signature;
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

struct McBalanceReport {
    balance @0: CustomInt128;
    # Maximum possible remote debt
    localPendingDebt @1: CustomUInt128;
    # Frozen credits by our side
    remotePendingDebt @2: CustomUInt128;
    # Frozen credits by the remote side
}

struct CurrencyReport {
        currency @0: Currency;
        balance @1: McBalanceReport;
}

struct ResetTermsReport {
        resetToken @0: Signature;
        # TODO: Possibly do not expose resetToken to the user? Instead, have
        # here some random binary blob or a u64 number?
        balanceForReset @1: List(CurrencyBalance);
        # List of expected balance for each currency
}

struct ChannelInconsistentReport {
        localResetTerms @0: List(CurrencyBalance);
        optRemoteResetTerms: union {
                remoteResetTerms @1: ResetTermsReport;
                empty @2: Void;
        }
}

struct ChannelConsistentReport {
        currencyReports @0: List(CurrencyReport);
}


struct ChannelStatusReport {
        union {
                inconsistent @0: ChannelInconsistentReport;
                consistent @1: ChannelConsistentReport;
        }
}

struct OptLastIncomingMoveToken {
        union {
                moveTokenHashed @0: MoveTokenHashedReport;
                empty @1: Void;
        }
}

struct CurrencyRate {
        currency @0: Currency;
        rate @1: Rate;
}

struct CurrencyConfigReport {
        currency @0: Currency;
        rate @1: Rate;
        remoteMaxDebt @2: CustomUInt128;
        isOpen @3: Bool;
}

struct FriendReport {
        name @0: Text;
        remoteRelays @1: List(RelayAddress);
        # TODO: Not sure if we should keep this field.
        # Is the user actually going to use remoteRelays? Maybe to export a friend?
        currencyConfigs @2: List(CurrencyConfigReport);
        optLastIncomingMoveToken @3: OptLastIncomingMoveToken;
        liveness @4: FriendLivenessReport;
        channelStatus @5: ChannelStatusReport;
        status @6: FriendStatusReport;
}

struct PkFriendReport {
        friendPublicKey @0: PublicKey;
        friendReport @1: FriendReport;
}

struct PkFriendReportList {
        list @0: List(PkFriendReport);
}

# A full Funder report.
struct FunderReport {
        localPublicKey @0: PublicKey;
        relays @1: List(NamedRelayAddress);
        friends @2: PkFriendReportList;
}


############################################################################
############################################################################

struct AddFriendReport {
        friendPublicKey @0: PublicKey;
        name @1: Text;
        relays @2: List(RelayAddress);
        optLastIncomingMoveToken @3: OptLastIncomingMoveToken;
        channelStatus @4: ChannelStatusReport;
}

struct FriendReportMutation {
        union {
                setRemoteRelays @0: List(RelayAddress);
                setName @1: Text;
                updateCurrencyConfig @2: CurrencyConfigReport;
                removeCurrencyConfig @3: Currency;
                setChannelStatus @4: ChannelStatusReport;
                setStatus @5: FriendStatusReport;
                setOptLastIncomingMoveToken @6: OptLastIncomingMoveToken;
                setLiveness @7: FriendLivenessReport;
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

# TODO: Possibly add reports for the liveness of our relays (From the
# Channeler?)

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
