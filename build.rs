// Build script to set compile-time constants for the transfer program
fn main() {
    // You can adjust the values here or read from environment variables / Cargo.toml if needed
    println!("cargo:rustc-env=TARGET_EXE_NAME=original.exe");
}
