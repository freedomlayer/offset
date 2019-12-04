use cluFlock::err::FlockError;
use cluFlock::{FlockLock, ToFlock};
use std::fs::File;
use std::path::Path;

#[derive(Debug)]
pub enum LockFileError {
    IoError(std::io::Error),
    FLockError(FlockError<File>),
}

#[derive(Debug)]
pub struct LockFileHandle {
    flock_lock: FlockLock<File>,
}

/// Attempt to lock file
/// Returns a handle to the locked file. When the handle is dropped, the file is automatically unlocked.
pub fn try_lock_file(lock_file_path: &Path) -> Result<LockFileHandle, LockFileError> {
    let file = File::create(&lock_file_path).map_err(LockFileError::IoError)?;
    let flock_lock = file
        .try_exclusive_lock()
        .map_err(LockFileError::FLockError)?;
    Ok(LockFileHandle { flock_lock })
}

#[allow(unused)]
fn wait_lock_file(lock_file_path: &Path) -> Result<LockFileHandle, LockFileError> {
    let file = File::create(&lock_file_path).map_err(LockFileError::IoError)?;
    let flock_lock = file
        .wait_exclusive_lock()
        .map_err(LockFileError::FLockError)?;
    Ok(LockFileHandle { flock_lock })
}

#[cfg(test)]
mod test {
    use super::*;

    use std::path::PathBuf;
    use std::sync::mpsc::channel;
    use std::thread;
    use std::time::Duration;

    use tempfile::tempdir;

    fn spawn_lockfile_wait_thread(
        lock_file_path: PathBuf,
        delay: u64,
        sender: std::sync::mpsc::Sender<i32>,
    ) {
        thread::spawn(move || {
            for _ in 0..5 {
                let _lock = wait_lock_file(&lock_file_path).unwrap();

                sender.send(1).unwrap();
                thread::sleep(Duration::from_millis(delay));
                sender.send(-1).unwrap();
            }
        });
    }

    #[test]
    fn test_lockfile_wait() {
        let dir = tempdir().unwrap();
        let lock_file_path = dir.path().join("test_lock_file");
        let (sender, receiver) = channel::<i32>();

        spawn_lockfile_wait_thread(lock_file_path.clone(), 17, sender.clone());
        spawn_lockfile_wait_thread(lock_file_path.clone(), 32, sender.clone());
        spawn_lockfile_wait_thread(lock_file_path.clone(), 19, sender.clone());

        drop(sender);

        let mut sum = 0;
        while let Ok(num) = receiver.recv() {
            sum += num;
            assert!(sum >= 0);
            assert!(sum <= 1);
        }
    }

    fn spawn_lockfile_try_thread(
        lock_file_path: PathBuf,
        delay: u64,
        sender: std::sync::mpsc::Sender<i32>,
    ) {
        thread::spawn(move || {
            for _ in 0..6 {
                let _lock = loop {
                    if let Ok(lock) = try_lock_file(&lock_file_path) {
                        break lock;
                    } else {
                        thread::sleep(Duration::from_millis(50));
                    }
                };

                sender.send(1).unwrap();
                thread::sleep(Duration::from_millis(delay));
                sender.send(-1).unwrap();
            }
        });
    }

    #[test]
    fn test_lockfile_try() {
        let dir = tempdir().unwrap();
        let lock_file_path = dir.path().join("test_lock_file");
        let (sender, receiver) = channel::<i32>();

        spawn_lockfile_try_thread(lock_file_path.clone(), 17, sender.clone());
        spawn_lockfile_try_thread(lock_file_path.clone(), 32, sender.clone());
        spawn_lockfile_try_thread(lock_file_path.clone(), 19, sender.clone());

        drop(sender);

        let mut sum = 0;
        while let Ok(num) = receiver.recv() {
            sum += num;
            assert!(sum >= 0);
            assert!(sum <= 1);
        }
    }
}
