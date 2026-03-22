use std::sync::Arc;

use bytes::Bytes;
use slatedb::{Db, object_store::ObjectStore};
use tokio::sync::{mpsc, oneshot};

use crate::record::Record;

struct Streamer {
    next_seq: u64,
    db: Db,
}

impl Streamer {
    pub async fn new(db: Db) -> Self {
        let s = Self {
            next_seq: 1,
            db, 
        };
    }

    

    async fn run(&mut self, mut rx: mpsc::UnboundedReceiver<Request>) {
        while let Some(msg) = rx.recv().await {
            match msg {
                Request::Append { data, reply_tx } => {
                    let record = Record {
                        seq_num: todo!(),
                        data,
                    }
                },
                Request::Read { seq_num, reply_tx } => todo!(),
            }
        }
    }
}

enum Request {
    Append {
        data: Bytes, // content to be appended to stream
        reply_tx: oneshot::Sender<Record>, // response channel
    },
    Read {
        seq_num: u64,
        reply_tx: oneshot::Sender<Option<Record>>,
    }
}


