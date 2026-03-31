fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("../proto/customer_db.proto")?;
    tonic_build::compile_protos("../proto/product_db.proto")?;
    Ok(())
}
