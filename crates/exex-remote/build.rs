//! Build script for the shared remote `ExEx` transport crate.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=proto/remote_exex.proto");

    if std::env::var_os("PROTOC").is_none() {
        if let Ok(output) = Command::new("which").arg("protoc").output() {
            if output.status.success() {
                if let Ok(path) = String::from_utf8(output.stdout) {
                    let path = path.trim();
                    if !path.is_empty() {
                        std::env::set_var("PROTOC", path);
                    }
                }
            }
        }
    }

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/remote_exex.proto"], &["proto"])
        .expect("failed to compile remote exex proto");
}
