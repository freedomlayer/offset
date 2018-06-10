use std::collections::HashMap;

use slab::Slab;

use crypto::hash::HashResult;
use crypto::identity::PublicKey;

use super::{SessionId, HandshakeSession};

pub struct SessionTable<S> {
    slab: Slab<HandshakeSession<S>>,

    idx_last_hash: HashMap<HashResult, usize>,
    idx_session_id: HashMap<SessionId, usize>,
}

impl<S> Default for SessionTable<S> {
    fn default() -> SessionTable<S> {
        SessionTable {
            slab: Slab::new(),
            idx_last_hash: HashMap::new(),
            idx_session_id: HashMap::new(),
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
    pub fn add_session(&mut self, s: HandshakeSession<S>)
        -> Option<HandshakeSession<S>>
    {
        if self.idx_last_hash.contains_key(&s.last_hash) ||
            self.idx_session_id.contains_key(&s.id) {
            return Some(s);
        }

        let last_hash = s.last_hash.clone();
        let session_id = s.id.clone();

        let idx = self.slab.insert(s);

        self.idx_last_hash.insert(last_hash, idx);
        self.idx_session_id.insert(session_id, idx);

        None
    }

    /// Returns a reference to the `HandshakeSession` corresponding to the hash.
    pub fn get_session(&self, hash: &HashResult)
        -> Option<&HandshakeSession<S>>
    {
        if let Some(idx) = self.idx_last_hash.get(hash) {
            Some(self.slab.get(*idx).expect("session table index broken"))
        } else {
            None
        }
    }

    /// Returns true if a value is associated with the given `SessionId`.
    pub fn contains_session(&self, id: &SessionId) -> bool {
        self.idx_session_id.contains_key(&id)
    }

    /// Removes and returns the `HandshakeSession` relevant to the `HashResult`.
    pub fn remove_by_last_hash(&mut self, hash: &HashResult)
        -> Option<HandshakeSession<S>>
    {
        if let Some(idx) = self.idx_last_hash.remove(hash) {
            let session = self.slab.remove(idx);
            self.idx_session_id.remove(&session.id);
            Some(session)
        } else {
            None
        }
    }

    /// Removes and returns the `HandshakeSession` relevant to the `PublicKey`.
    pub fn remove_by_public_key(&mut self, pk: &PublicKey) {
        // A public key associated with TWO session at most.
        let i_session_id = SessionId::new_initiator(pk.clone());
        let r_session_id = SessionId::new_responder(pk.clone());

        if let Some(idx) = self.idx_session_id.remove(&i_session_id) {
            let session = self.slab.remove(idx);
            self.idx_last_hash.remove(&session.last_hash);
        }

        if let Some(idx) = self.idx_session_id.remove(&r_session_id) {
            let session = self.slab.remove(idx);
            self.idx_last_hash.remove(&session.last_hash);
        }
    }

    /// Apply a time tick over all sessions in the table.
    pub fn time_tick(&mut self) {
        self.slab.iter_mut().for_each(|(_i, session)| {
            if session.timeout_counter > 0 {
                session.timeout_counter -= 1;
            }
        });

        let mut idx_timeout = Vec::new();

        for (idx, session) in &self.slab {
            if session.timeout_counter == 0 {
                idx_timeout.push(idx);
            }
        }

        for idx in idx_timeout {
            let last_hash = self.slab.get(idx)
                .and_then(|s| Some(s.last_hash.clone()))
                .expect("session table index broken");
            self.remove_by_last_hash(&last_hash);
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

        let neighbor_pk = PublicKey::from(&[0u8; PUBLIC_KEY_LEN]);

        let session_id = SessionId::new_responder(neighbor_pk);
        let last_hash = HashResult::from(&[0x01; HASH_RESULT_LEN]);

        let new_session =
            HandshakeSession::new(session_id.clone(), (), last_hash.clone());

        assert!(session_table.add_session(new_session).is_none());

        assert!(session_table.contains_session(&session_id));
        assert!(session_table.remove_by_last_hash(&last_hash).is_some());
        assert!(!session_table.contains_session(&session_id));
    }

    #[test]
    fn time_tick() {
        let mut session_table: SessionTable<()> = SessionTable::new();

        let neighbor_pk = PublicKey::from(&[0x00; PUBLIC_KEY_LEN]);
        let session_id = SessionId::new_responder(neighbor_pk);
        let last_hash = HashResult::from(&[0x01; HASH_RESULT_LEN]);
        let new_session =
            HandshakeSession::new(session_id.clone(), (), last_hash.clone());

        assert!(session_table.add_session(new_session).is_none());

        for _ in 0..HANDSHAKE_SESSION_TIMEOUT - 1 {
            session_table.time_tick();
            assert!(session_table.contains_session(&session_id));
        }
        // Remove the session
        session_table.time_tick();
        assert!(!session_table.contains_session(&session_id));
    }
}
