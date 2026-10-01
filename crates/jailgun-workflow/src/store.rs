use crate::{database::Database, Error, Result};
use std::path::Path;
use tokio::sync::{mpsc, oneshot};

type Operation = Box<dyn FnOnce(&mut Database) + Send>;
enum Message {
    Call(Operation),
    Close(oneshot::Sender<()>),
}

/// One dedicated database thread, with a bounded queue shared by every interface.
/// Operations return only after commit; callers must broadcast returned events afterwards.
#[derive(Clone)]
pub struct Store {
    sender: mpsc::Sender<Message>,
}

impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::from_database(Database::open(root.as_ref())?)
    }

    /// Open only an existing, matching schema; backups must not implicitly upgrade it.
    pub fn open_for_backup(root: impl AsRef<Path>) -> Result<Self> {
        Self::from_database(Database::open_for_backup(root.as_ref())?)
    }

    pub(crate) fn from_database(mut database: Database) -> Result<Self> {
        let (sender, mut receiver) = mpsc::channel::<Message>(64);
        std::thread::Builder::new()
            .name("jailgun-database".into())
            .spawn(move || {
                while let Some(message) = receiver.blocking_recv() {
                    match message {
                        Message::Call(operation) => operation(&mut database),
                        Message::Close(done) => {
                            drop(database);
                            let _ = done.send(());
                            return;
                        }
                    }
                }
            })?;
        Ok(Self { sender })
    }

    pub(crate) async fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Database) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Message::Call(Box::new(move |db| {
                let _ = sender.send(operation(db));
            })))
            .await
            .map_err(|_| closed())?;
        receiver.await.map_err(|_| closed())?
    }

    /// Stops the shared writer after queued operations; all clones subsequently fail closed.
    pub async fn close(&self) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Message::Close(sender))
            .await
            .map_err(|_| closed())?;
        receiver.await.map_err(|_| closed())
    }

    pub async fn sqlite_version(&self) -> Result<String> {
        self.call(|db| {
            Ok(db
                .connection
                .query_row("SELECT sqlite_version()", [], |r| r.get(0))?)
        })
        .await
    }
}

fn closed() -> Error {
    Error::action(
        "storage-closed",
        "The database writer is unavailable.",
        "Restart the daemon and inspect its local diagnostics.",
    )
}
