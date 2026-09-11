fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Hermetic builds: prefer a vendored protoc so neither CI runners nor
    // fresh dev machines need protobuf installed system-wide.
    if std::env::var_os("PROTOC").is_none() {
        if let Ok(path) = protoc_bin_vendored::protoc_bin_path() {
            std::env::set_var("PROTOC", path);
        }
    }
    tonic_build::compile_protos("../../proto/cc.proto")?;
    Ok(())
}
