use futures::channel::mpsc;

pub trait Listener {
    type Connection;
    type Config;
    type Arg;

    fn listen(self, arg: Self::Arg) -> (mpsc::Sender<Self::Config>, 
                             mpsc::Receiver<Self::Connection>);
}
