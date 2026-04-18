fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;

    // Build scripts run single-threaded here, and setting PROTOC only affects this
    // process and child codegen commands.
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    tonic_prost_build::compile_protos("proto/slatestreams/v1/streams.proto")?;
    Ok(())
}
