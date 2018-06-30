//! State machine of `CSwitch` handshake protocol.
//!
//! In `CSwitch`, the handshake state can be divided into two stages roughly:
//!
//! ## Request Nonce
//!
//! At this state, initiator sent `RequestNonce` message to responder, upon
//! receiving a `RequestNonce` message, responder generate a `ResponseNonce`
//! as response. **For initiator**, this stage is staleless.
//!
//! ## Perform Handshake
//!
//! This stage is a standard Diffie–Hellman key exchange process. At this stage,
//! initiator and responder complete key exchange and derive the keys for later.
//!
//! ```notrust
//!    Initiator                                        Responder
//!        | ----------------ExchangeActive---------------> |
//!        |                                                |
//!        | <---------------ExchangePassive--------------- |
//!        |                                                |
//!        | -----------------ChannelReady----------------> |
//! ```
//!
//! ### Handshake Phases
//!
//! 1. Upon **initiator** receiving a `ResponseNonce` message, it checks whether
//!    it sent the `request_rand_nonce`, if so, it creates a `HandshakeSession`.
//!    Then sent a `ExchangeActive` message to responder.
//! 2. Upon **responder** receiving a `ExchangeActive` message, it checks if the
//!    `responder_rand_nonce` is included in its constant nonce list. (TODO)

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

        local_dh_private_key: DhPrivateKey,
    },

    /// After responder received `ExchangeActive` and sent `ExchangePassive`, it
    /// creates a new handshake session with this state.
    AfterResponderExchangePassive {
        sent_rand_nonce: RandValue,
        recv_rand_nonce: RandValue,

        sent_key_salt: Salt,
        recv_key_salt: Salt,

        remote_dh_public_key: DhPublicKey,
        local_dh_private_key: DhPrivateKey,
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
