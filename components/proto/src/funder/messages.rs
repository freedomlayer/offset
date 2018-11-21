
pub enum FunderToChanneler<A> {
    /// Send a message to a friend
    Message((PublicKey, Vec<u8>)) // (friend_public_key, message)
    /// Request to add a new friend
    AddFriend((PublicKey, A)) // (friend_public_key, address)
    /// Request to remove a friend
    RemoveFriend(PublicKey) // friend_public_key
}

pub enum ChannelerToFunder<A> {
    /// A friend is now online
    Online(PublicKey),
    /// A friend is now offline
    Offline(PublicKey),
    /// Incoming message from a remote friend
    Message((PublicKey, Vec<u8>)), // (friend_public_key, message)
}

