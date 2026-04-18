pub mod basin;
pub mod error;
pub mod grpc;
pub mod key;
pub mod record;
pub mod storage;
pub mod stream;
pub mod stream_controller;

pub mod pb {
    pub mod slatestreams {
        pub mod v1 {
            tonic::include_proto!("slatestreams.v1");
        }
    }
}
