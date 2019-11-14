use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{future, SinkExt, StreamExt};

use common::conn::{BoxFuture, ConnPairVec, FutTransform};

/// Prefix a communication session (Of Vec<u8>) with each side declaring his version.
/// If the local version does not match the stated remote version, the connection is closed.
#[derive(Clone)]
pub struct VersionPrefix<S> {
    local_version: u32,
    spawner: S,
}

impl<S> VersionPrefix<S>
where
    S: Spawn,
{
    pub fn new(local_version: u32, spawner: S) -> Self {
        VersionPrefix {
            local_version,
            spawner,
        }
    }

    pub fn spawn_prefix(&mut self, conn_pair: ConnPairVec) -> ConnPairVec {
        let (mut sender, mut receiver) = conn_pair.split();

        let (user_sender, from_user_sender) = mpsc::channel(0);
        let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

        let local_version = self.local_version;
        let sender_fut = async move {
            // First send our protocol version to the remote side:
            let mut version_data = Vec::new();
            version_data.write_u32::<BigEndian>(local_version).unwrap();
            if sender.send(version_data).await.is_err() {
                warn!("Failed to send version information");
                return;
            }
            // Next send any other message from the user:
            let mut from_user_sender = from_user_sender.map(Ok);
            let _ = sender.send_all(&mut from_user_sender).await;
        };
        // If spawning fails, the user will find out when he tries to send
        // through user_sender.
        let _ = self.spawner.spawn(sender_fut);

        let receiver_fut = async move {
            // Expect version to be the first sent data:
            let version_data = match receiver.next().await {
                Some(version_data) => version_data,
                _ => {
                    warn!("Failed to receive version information");
                    return;
                }
            };

            if version_data.len() != 4 {
                warn!("Invalid version_data length");
                return;
            }

            let remote_version = BigEndian::read_u32(&version_data);
            if remote_version != local_version {
                warn!("Invalid remote version: {}", remote_version);
                return;
            }

            let mut receiver = receiver.map(Ok);
            let _ = to_user_receiver.send_all(&mut receiver).await;
        };
        // If spawning fails, the user will find out when he tries to read
        // from user_receiver.
        if let Err(e) = self.spawner.spawn(receiver_fut) {
            error!("VersionPrefix::spawn_prefix(): spawn() failed: {:?}", e);
        }

        ConnPairVec::from_raw(user_sender, user_receiver)
    }
}

impl<S> FutTransform for VersionPrefix<S>
where
    S: Spawn + Send,
{
    type Input = ConnPairVec;
    type Output = ConnPairVec;

    fn transform(&mut self, input: Self::Input) -> BoxFuture<'_, Self::Output> {
        Box::pin(future::ready(self.spawn_prefix(input)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::{ThreadPool, LocalPool};

    async fn task_version_prefix_match<S>(spawner: S)
    where
        S: Spawn,
    {
        let (a_sender, b_receiver) = mpsc::channel(0);
        let (b_sender, a_receiver) = mpsc::channel(0);

        // Both A and B use version 3:
        let mut version_prefix_3 = VersionPrefix::new(3u32, spawner);

        let (mut a_sender, mut a_receiver) = version_prefix_3.spawn_prefix(ConnPairVec::from_raw(a_sender, a_receiver)).split();
        let (mut b_sender, mut b_receiver) = version_prefix_3.spawn_prefix(ConnPairVec::from_raw(b_sender, b_receiver)).split();

        // We expect the connection to work correctly, as the versions match:
        a_sender.send(vec![1, 2, 3]).await.unwrap();
        assert_eq!(b_receiver.next().await.unwrap(), vec![1, 2, 3]);

        b_sender.send(vec![3, 2, 1]).await.unwrap();
        assert_eq!(a_receiver.next().await.unwrap(), vec![3, 2, 1]);
    }

    #[test]
    fn test_version_prefix_match() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_version_prefix_match(thread_pool.clone()));
    }

    async fn task_version_prefix_mismatch<S>(spawner: S)
    where
        S: Spawn + Clone,
    {
        let (a_sender, b_receiver) = mpsc::channel(0);
        let (b_sender, a_receiver) = mpsc::channel(0);

        // Version mismatch between A and B:
        let mut version_prefix_3 = VersionPrefix::new(3u32, spawner.clone());
        let mut version_prefix_4 = VersionPrefix::new(4u32, spawner);


        let (mut a_sender, _a_receiver) = version_prefix_3.spawn_prefix(ConnPairVec::from_raw(a_sender, a_receiver)).split();
        let (_b_sender, mut b_receiver) = version_prefix_4.spawn_prefix(ConnPairVec::from_raw(b_sender, b_receiver)).split();

        // We expect the connection to be closed because of version mismatch:
        let _ = a_sender.send(vec![1, 2, 3]).await;
        assert!(b_receiver.next().await.is_none());

        // b_sender.send(vec![3, 2, 1]).await.unwrap();
        // assert!(a_receiver.next().await.is_none());
    }

    #[test]
    fn test_version_prefix_mismatch() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_version_prefix_mismatch(thread_pool.clone()));
    }
}
