use std::convert::TryFrom;
use std::path::{Path, PathBuf};

use std::time::{Duration};

use chrono::Utc;

use async_std::stream::interval;
use async_std::fs::{File, read, remove_file};
use async_std::io::prelude::WriteExt;
use async_std::io::ErrorKind;

use futures::{future, stream, StreamExt, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::{oneshot, mpsc};

use byteorder::{BigEndian, ByteOrder, WriteBytesExt};

use common::select_streams::select_streams;
use common::conn::BoxStream;


#[derive(Debug)]
pub enum LockFileError {
    AlreadyLocked,
    InvalidLockFile,
    SpawnError,
    AsyncIoError(async_std::io::Error),
    TimestampFromFuture,
}

/// If dropped, after a while (2*duration) the lock can be acquired by another entity.
#[derive(Debug)]
pub struct LockFileHandle {
    close_sender: oneshot::Sender<()>,
}

/// Get amount of milliseconds that passed since the epoch.
fn get_time() -> u128 {
    u128::try_from(Utc::now().timestamp_millis()).expect("Invalid time")
}

/// Write a new timestamp into a file
async fn write_timestamp(path: &Path) -> Result<(), LockFileError> {
    let mut file = File::create(path).await.map_err(LockFileError::AsyncIoError)?;

    // TODO: Is there a way to do this on-the-fly, without creating an intermediate vector?
    let mut ser_cur_time = Vec::new();
    ser_cur_time.write_u128::<BigEndian>(get_time()).unwrap();
    file.write_all(&ser_cur_time).await.map_err(LockFileError::AsyncIoError)?;
    Ok(())
}

/// Check if a file contains a recent timestamp
async fn is_timestamp_recent(path: &Path, duration: &Duration) -> Result<bool, LockFileError> {
    let vec = match read(&path).await {
        Ok(vec) => vec,
        Err(e) => {
            if e.kind() == ErrorKind::NotFound {
                return Ok(false);
            } else {
                return Err(LockFileError::AsyncIoError(e));
            }
        }
    };

    if vec.is_empty() {
        // TODO: Does this mean something special?
        return Ok(false);
    }

    if vec.len() != 128 / 8 {
        return Err(LockFileError::InvalidLockFile);
    }
    let timestamp = BigEndian::read_u128(&vec[..]);
    let cur_time = get_time();

    let diff = cur_time.checked_sub(timestamp).ok_or(LockFileError::TimestampFromFuture)?;

    Ok(diff < 2*duration.as_millis())
}

#[derive(Debug)]
enum LockFileEvent {
    Release,
    IntervalTick,
    IntervalClosed,
}

// TODO: This implementation is very hacky! 
// Maybe in the future a better implementation (with support from the operating system) will be possible.
/// Lock a lockfile. This signals other entities that some resource is in use.
/// On success, returns a LockFileHandle. When the handle is dropped, the resource is considered
/// available for other entities.
///
/// Note that a lock is only an advisory, and therefore should not be used for security purposes. 
/// Should only be used for synchronization between trusting actors.
///
/// `duration` is implementation related: 
/// - The period of time we wait between updating the timestamp. 
/// - After the file is freed, other users will have to wait `2*duration` before being
/// able to use it.
pub async fn lock_file<S>(path_buf: PathBuf, duration: Duration, spawner: &S) -> Result<LockFileHandle, LockFileError>
where
    S: Spawn,
{

    // Check if the lock file contains old timestamp.
    // If so, it means that we can lock the file:
    if is_timestamp_recent(path_buf.as_path(), &duration).await? { 
        return Err(LockFileError::AlreadyLocked);
    }; 

    // Acquire the lock by writing a timestamp into the file for the first time:
    write_timestamp(path_buf.as_path()).await?;

    // Keep locking the file by writing a new timestamp in constant in intervals:
    let interval = interval(duration);

    let interval = interval.map(|_| LockFileEvent::IntervalTick)
        .chain(stream::once(future::ready(LockFileEvent::IntervalClosed)));

    let (close_sender, close_receiver) = oneshot::channel();
    let (mut release_sender, release_receiver) = mpsc::channel(0);

    spawner.spawn(async move {
        let _ = close_receiver.await;
        let _ = release_sender.send(()).await;
    }).map_err(|_| LockFileError::SpawnError)?;
    
    let release_receiver = release_receiver.map(|_| LockFileEvent::Release);

    let mut incoming_events = select_streams![
        interval,
        release_receiver
    ];

    spawner.spawn(async move {
        while let Some(event) = incoming_events.next().await {
            match event {
                LockFileEvent::Release => {
                    break;
                },
                LockFileEvent::IntervalTick => {
                    if let Err(e) = write_timestamp(path_buf.as_path()).await {
                        warn!("lock_file: write_timestamp() failed: {:?}", e);
                    }
                },
                LockFileEvent::IntervalClosed => {
                    warn!("lock_file: IntervalClosed");
                    break;
                },
            };
        }
        // Erase lock file:
        if let Err(e) = remove_file(&path_buf).await {
            warn!("lock_file: Error in remove_file(): {:?}", e);
        }
    }).map_err(|_| LockFileError::SpawnError)?;

    Ok(LockFileHandle {close_sender})
}


#[cfg(test)]
mod test {
    use super::*;

    use futures::executor::{ThreadPool, block_on};
    use futures::channel::mpsc;
    use futures::SinkExt;

    use async_std::task;
    use tempfile::tempdir;

    #[test]
    fn test_read_empty_file() {
        let dir = tempdir().unwrap();
        let nonexistent_file = dir.path().join("nonexistent_file"); 

        let my_fut = async move {
            assert!(read(&nonexistent_file).await.is_err());
        };

        block_on(my_fut);
    }

    const LOCK_DURATION: Duration = Duration::from_millis(50);

    async fn lockfile_check<S>(spawner: S) 
        where S: Spawn + Send + Sync + Clone + 'static,
    {
        let dir = tempdir().unwrap();
        let lock_file_path = dir.path().join("test_lock_file");
        let (sender, mut receiver) = mpsc::channel::<i32>(0x100);

        let mut c_sender = sender.clone();
        let c_spawner = spawner.clone();
        let c_lock_file_path = lock_file_path.clone();
        spawner.spawn(async move {
            let lock_res = lock_file(PathBuf::from(&c_lock_file_path), LOCK_DURATION, &c_spawner).await;
            let _lock_file_handle = match lock_res {
                Ok(lock_file_handle) => lock_file_handle,
                _ => unreachable!(),
            };

            // Entering:
            c_sender.send(1).await.unwrap();

            // Wait some time
            task::sleep(Duration::from_millis(200)).await;

            // Leaving:
            c_sender.send(-1).await.unwrap();

        }).unwrap();

        let mut c_sender = sender.clone();
        let c_spawner = spawner.clone();
        let c_lock_file_path = lock_file_path.clone();
        spawner.spawn(async move {
            // Wait some time. If we don't have this wait we will probably run into a race.
            task::sleep(Duration::from_millis(100)).await;
            let _lock_file_handle = loop {
                let lock_res = lock_file(PathBuf::from(&c_lock_file_path), LOCK_DURATION, &c_spawner).await;
                let _lock_file_handle = match lock_res {
                    Ok(lock_file_handle) => break lock_file_handle,
                    Err(LockFileError::AlreadyLocked) => {
                        task::sleep(Duration::from_millis(50)).await;
                        continue;
                    },
                    _ => unreachable!(),
                };
            };

            // Entering:
            c_sender.send(1).await.unwrap();

            // Wait some time
            task::sleep(Duration::from_millis(200)).await;

            // Leaving:
            c_sender.send(-1).await.unwrap();

            // Wait some time
            task::sleep(Duration::from_millis(100)).await;

        }).unwrap();

        drop(sender);


        // Make sure that at every stage at most one entity was inside.
        let mut sum = 0;
        while let Some(num) = receiver.next().await {
            sum += num;
            assert!(sum >= 0);
            assert!(sum <= 1);
        }
    }

    #[test]
    fn test_lockfile_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(lockfile_check(thread_pool));
    }
}
