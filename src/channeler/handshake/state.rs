//! The handshake state machine of `CSwitch` handshake protocol.
//!
//! In `CSwitch`, the handshake state can be divided into two stages roughly:
//!
//! ## PreHandshake
//!
//! This stage include: Initiator sent `RequestNonce` to the responder, and the
//! responder handle that message and generate a `ResponseNonce` as response.
//!
//! ## Handshaking
//!
//! This stage is a standard Diffie–Hellman key exchange process. At this stage,
//! initiator and responder complete key exchange and derive the keys for later.

use crypto::rand_values::RandValue;
use crypto::dh::{DhPrivateKey, DhPublicKey, Salt};

/// The handshake state machine of `CSwitch` handshake protocol.
pub enum HandshakeState {
    /// After initiator received `ResponseNonce` and sent `ExchangeActive`, the
    /// state transfer from `AfterInitiatorRequestNonce` to this state.
    AfterInitiatorExchangeActive {
        recv_rand_nonce: RandValue,
        sent_rand_nonce: RandValue,

        sent_key_salt: Salt,

        my_private_key: DhPrivateKey,
    },

    /// After responder received `ExchangeActive` and sent `ExchangePassive`, it
    /// creates a new handshake session with this state.
    AfterResponderExchangePassive {
        sent_rand_nonce: RandValue,
        recv_rand_nonce: RandValue,

        sent_key_salt: Salt,
        recv_key_salt: Salt,

        remote_dh_public_key: DhPublicKey,

        my_private_key: DhPrivateKey,
    },
}

impl HandshakeState {
    #[inline]
    pub(crate) fn is_after_initiator_exchange_active(&self) -> bool {
        match *self {
            HandshakeState::AfterInitiatorExchangeActive { .. } => true,
            _ => false
        }
    }

    #[inline]
    pub(crate) fn is_after_responder_exchange_passive(&self) -> bool {
        match *self {
            HandshakeState::AfterResponderExchangePassive { .. } => true,
            _ => false,
        }
    }
}
