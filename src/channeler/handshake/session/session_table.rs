use std::collections::HashMap;

use slab::Slab;

use crypto::hash::HashResult;
use crypto::identity::PublicKey;

use super::{SessionId, HandshakeSession};

pub struct SessionTable<S> {
    slab: Slab<HandshakeSession<S>>,

    hash_to_key: HashMap<HashResult, usize>,
    session_id_to_key: HashMap<SessionId, usize>,
}

impl<S> Default for SessionTable<S> {
    fn default() -> SessionTable<S> {
        SessionTable {
            slab: Slab::new(),
            hash_to_key: HashMap::new(),
            session_id_to_key: HashMap::new(),
        }
    }
}

impl<S> SessionTable<S> {
    /// Creates a empty `SessionTable`.
    pub fn new() -> SessionTable<S> {
        SessionTable::default()
    }

    /// Add a new `HandshakeSession` into the table.
    ///
    /// - If the table didn't have the given `SessionId` AND `HashResult`,
    /// the new session will be add into the table and returns `None`.
    ///
    /// - If the table did have the given `SessionId` OR `HashResult`,
    /// this method do nothing and returns `Some(HandshakeSession)`.
    pub fn add_session(&mut self, session: HandshakeSession<S>) -> Option<HandshakeSession<S>> {
        if self.hash_to_key.contains_key(&session.last_hash) ||
            self.session_id_to_key.contains_key(session.session_id()) {
            return Some(session);
        }

        let last_hash = session.last_hash.clone();
        let session_id = session.session_id.clone();

        let key = self.slab.insert(session);

        self.hash_to_key.insert(last_hash, key);
        self.session_id_to_key.insert(session_id, key);

        None
    }

    /// Returns a reference to the `HandshakeSession` corresponding to the hash.
    pub fn get_session(&self, hash: &HashResult) -> Option<&HandshakeSession<S>> {
        if let Some(idx) = self.hash_to_key.get(hash) {
            Some(self.slab.get(*idx).expect("the hash to key mapping broken"))
        } else {
            None
        }
    }

    /// Returns true if a value is associated with the given `SessionId`.
    pub fn contains_session(&self, id: &SessionId) -> bool {
        self.session_id_to_key.contains_key(&id)
    }

    /// Removes and returns the `HandshakeSession` relevant to the `HashResult`.
    pub fn remove_session_by_hash(&mut self, hash: &HashResult) -> Option<HandshakeSession<S>> {
        if let Some(key) = self.hash_to_key.remove(hash) {
            let session = self.slab.remove(key);
            self.session_id_to_key.remove(&session.session_id);
            Some(session)
        } else {
            None
        }
    }

    /// Removes `HandshakeSession`s relevant to the `PublicKey`.
    pub fn remove_session_by_public_key(&mut self, public_key: &PublicKey) {
        // A public key associated with TWO sessions at most.
        let initiator_session_id = SessionId::new_initiator(public_key.clone());
        let responder_session_id = SessionId::new_responder(public_key.clone());

        if let Some(key) = self.session_id_to_key.remove(&initiator_session_id) {
            let session = self.slab.remove(key);
            self.hash_to_key.remove(&session.last_hash);
        }

        if let Some(key) = self.session_id_to_key.remove(&responder_session_id) {
            let session = self.slab.remove(key);
            self.hash_to_key.remove(&session.last_hash);
        }
    }

    /// Apply a time tick over all sessions in the table.
    pub fn time_tick(&mut self) {
        self.slab.iter_mut().for_each(|(_i, session)| {
            if session.session_timeout > 0 {
                session.session_timeout -= 1;
            }
        });

        let mut expired_session_keys = Vec::new();

        for (key, session) in &self.slab {
            if session.session_timeout == 0 {
                expired_session_keys.push(key);
            }
        }

        for key in expired_session_keys {
            // NOTICE: The following line will cause panic
            // when the key doesn't relevant to a session.
            let session = self.slab.remove(key);

            self.hash_to_key.remove(&session.last_hash);
            self.session_id_to_key.remove(session.session_id());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use channeler::handshake::SessionId;
    use channeler::config::HANDSHAKE_SESSION_TIMEOUT;


    use crypto::hash::HASH_RESULT_LEN;
    use crypto::identity::PUBLIC_KEY_LEN;

    #[test]
    fn add_remove() {
        let mut session_table: SessionTable<()> = SessionTable::new();

        let neighbor_public_key = PublicKey::from(&[0u8; PUBLIC_KEY_LEN]);
        let session_id = SessionId::new_responder(neighbor_public_key);
        let last_hash = HashResult::from(&[0x01; HASH_RESULT_LEN]);

        let new_session = HandshakeSession::new(session_id.clone(), (), last_hash.clone());

        assert!(session_table.add_session(new_session).is_none());

        assert!(session_table.contains_session(&session_id));
        assert!(session_table.remove_session_by_hash(&last_hash).is_some());
        assert!(!session_table.contains_session(&session_id));
    }

    #[test]
    fn time_tick() {
        let mut session_table: SessionTable<()> = SessionTable::new();

        let neighbor_public_key = PublicKey::from(&[0x00; PUBLIC_KEY_LEN]);
        let session_id = SessionId::new_responder(neighbor_public_key);
        let last_hash = HashResult::from(&[0x01; HASH_RESULT_LEN]);

        let new_session = HandshakeSession::new(session_id.clone(), (), last_hash.clone());

        assert!(session_table.add_session(new_session).is_none());

        for _ in 0..HANDSHAKE_SESSION_TIMEOUT - 1 {
            session_table.time_tick();
            assert!(session_table.contains_session(&session_id));
        }

        // The session will expired after applying this tick.
        session_table.time_tick();
        assert!(!session_table.contains_session(&session_id));
    }
}
