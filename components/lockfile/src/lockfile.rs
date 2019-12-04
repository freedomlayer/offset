use std::convert::TryFrom;
use std::path::{Path, PathBuf};

use std::time::{Duration};

use chrono::Utc;

use async_std::stream::interval;
use async_std::fs::{File, read};
use async_std::io::prelude::WriteExt;

use futures::StreamExt;
use futures::future::RemoteHandle;
use futures::task::{Spawn, SpawnExt};

use byteorder::{BigEndian, ByteOrder, WriteBytesExt};


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
    handle: RemoteHandle<()>,
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
    let vec = read(&path).await.map_err(LockFileError::AsyncIoError)?;
    if vec.len() != 128 / 8 {
        return Err(LockFileError::InvalidLockFile);
    }
    let timestamp = BigEndian::read_u128(&vec[..]);
    let cur_time = get_time();

    let diff = cur_time.checked_sub(timestamp).ok_or(LockFileError::TimestampFromFuture)?;

    Ok(diff < 2*duration.as_millis())
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
    let mut interval = interval(duration);
    let handle = spawner.spawn_with_handle(async move {
        while let Some(()) = interval.next().await {
            if let Err(e) = write_timestamp(path_buf.as_path()).await {
                warn!("lock_file: write_timestamp() failed: {:?}", e);
            }
        }
    }).map_err(|_| LockFileError::SpawnError)?;

    Ok(LockFileHandle {handle})
}

#[cfg(test)]
mod test {

    #[test]
    fn test() {
    }
}
