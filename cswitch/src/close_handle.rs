use futures::sync::oneshot;

pub struct CloseHandle {
    handle_close_sender: oneshot::Sender<()>,       // Signal to close
    handle_close_receiver: oneshot::Receiver<()>,   // Closing is complete
}


/// This handle is used to send a closing notification to a Future,
/// even after it was consumed.
impl CloseHandle {
    /// Send a close message to remote Future
    /// Returns a future that resolves when the closing is complete.
    pub fn close(self) -> Result<oneshot::Receiver<()>,()> {
        match self.handle_close_sender.send(()) {
            Ok(()) => Ok(self.handle_close_receiver),
            Err(_) => Err(())
        }
    }
}


/// Create a close handle.
/// Returns a CloseHandle structure, together with a sender and a receiver oneshot
/// channels to be used by a remote future.
pub fn create_close_handle() -> (CloseHandle, (oneshot::Sender<()>, oneshot::Receiver<()>)) {
    let (future_close_sender, handle_close_receiver) = oneshot::channel();
    let (handle_close_sender, future_close_receiver) = oneshot::channel();

    let close_handle = CloseHandle {
        handle_close_sender,
        handle_close_receiver,
    };

    (close_handle, (future_close_sender, future_close_receiver))

}
