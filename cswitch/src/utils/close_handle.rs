use futures::sync::oneshot;

/// This handle is used to send a closing notification to a Future,
/// even after it was consumed.
pub struct CloseHandle {
    /// Request remote to close.
    tx: oneshot::Sender<()>,
    /// Closing completed.
    rx: oneshot::Receiver<()>,
}

impl CloseHandle {
    /// Creates a close handle.
    ///
    /// Returns a `CloseHandle`, along with the `oneshot::Sender<()>`
    /// and `oneshot::Receiver<()>` to be used by a remote future.
    pub fn new() -> (CloseHandle, (oneshot::Sender<()>, oneshot::Receiver<()>)) {
        let (remote_tx, handle_rx) = oneshot::channel();
        let (handle_tx, remote_rx) = oneshot::channel();

        let close_handle = CloseHandle {
            tx: handle_tx,
            rx: handle_rx,
        };

        (close_handle, (remote_tx, remote_rx))
    }

    /// Request remote to close.
    ///
    /// Returns a future that resolves when the closing is completed.
    pub fn close(self) -> Result<oneshot::Receiver<()>, ()> {
        match self.tx.send(()) {
            Ok(()) => Ok(self.rx),
            Err(_) => Err(())
        }
    }
}
