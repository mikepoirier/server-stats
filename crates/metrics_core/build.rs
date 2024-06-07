fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pwd = std::env::current_dir()?;
    println!(">>>> PWD: {pwd:?}");
    tonic_build::compile_protos("proto/metrics.proto")?;
    Ok(())
}
