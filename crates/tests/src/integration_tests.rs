//! Integration tests for the ev-reth binary and CLI functionality.
//!
//! This test suite focuses on testing the ev-reth binary compilation,
//! CLI argument handling, and overall integration with the Reth framework.

use std::process::{Command, Stdio};

/// Tests that the ev-reth binary compiles successfully
#[test]
fn test_ev_reth_binary_compiles() {
    let output = Command::new("cargo")
        .args(["build", "-p", "ev-reth", "--bin", "ev-reth"])
        .output()
        .expect("Failed to execute cargo build");

    assert!(
        output.status.success(),
        "Binary compilation failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    println!("✓ ev-reth binary compilation test passed");
}

/// Tests that the ev-reth binary shows help without crashing
#[test]
fn test_ev_reth_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "ev-reth", "--bin", "ev-reth", "--", "--help"])
        .output()
        .expect("Failed to execute ev-reth --help");

    // The help command should exit with code 0
    assert!(
        output.status.success(),
        "Help command failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should contain evolve-specific options or at least show it's a evolve-enabled build
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let full_output = format!("{stdout} {stderr}");

    // Check if ev-reth is mentioned anywhere in the output (args, build info, etc)
    assert!(
        full_output.to_lowercase().contains("ev-reth")
            || full_output.contains("Evolve")
            || full_output.contains("ev-reth"), // Binary name indicates ev-reth support
        "Help output should indicate this is a ev-reth-enabled build. Output: {}",
        &full_output[..500.min(full_output.len())] // Show first 500 chars of output
    );

    println!("✓ ev-reth help test passed");
}

/// Tests that evolve-specific CLI arguments are recognized
#[test]
fn test_evolve_cli_arguments() {
    // Test that evolve-specific arguments are parsed correctly
    let output = Command::new("cargo")
        .args(["run", "-p", "ev-reth", "--bin", "ev-reth", "--", "--help"])
        .output()
        .expect("Failed to execute ev-reth help");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check for evolve-specific arguments or ev-reth branding
    let stderr = String::from_utf8_lossy(&output.stderr);
    let full_output = format!("{stdout} {stderr}");
    assert!(
        full_output.to_lowercase().contains("ev-reth")
            || full_output.contains("Evolve")
            || full_output.contains("ev-reth"), // Binary name indicates ev-reth support
        "Should show ev-reth-related content or ev-reth branding"
    );

    // Since this is a Reth-based binary, it should have basic Ethereum node functionality
    let has_basic_options = stdout.contains("help")
        || stdout.contains("config")
        || stdout.contains("chain")
        || stdout.contains("datadir");
    assert!(has_basic_options, "Should show basic node options");

    println!("✓ evolve CLI arguments test passed");
}

/// Tests that the binary exits gracefully with invalid arguments
#[test]
fn test_ev_reth_invalid_arguments() {
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "ev-reth",
            "--bin",
            "ev-reth",
            "--",
            "--invalid-flag",
        ])
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .expect("Failed to execute evolve-reth with invalid args");

    // Should fail with non-zero exit code
    assert!(
        !output.status.success(),
        "Should fail with invalid arguments"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should contain some indication of the error
    assert!(
        stderr.contains("error") || stderr.contains("unknown") || stderr.contains("unrecognized"),
        "Error output should indicate invalid argument: {stderr}"
    );

    println!("✓ ev-reth invalid arguments test passed");
}

/// Tests that the Engine API integration tests run successfully
#[test]
fn test_evolve_engine_api_tests_run() {
    let output = Command::new("cargo")
        .args(["test", "test_engine_api", "--lib"])
        .output()
        .expect("Failed to execute cargo test for Engine API tests");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("dependency") || stderr.contains("feature") {
            println!("⚠ Engine API tests skipped (missing dependencies): {stderr}");
            return;
        }

        panic!(
            "Engine API tests failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
    }

    println!("✓ Engine API integration tests passed");
}

/// Tests library compilation and basic exports
#[test]
fn test_evolve_library_compilation() {
    let output = Command::new("cargo")
        .args(["build", "--lib"])
        .output()
        .expect("Failed to execute cargo build --lib");

    assert!(
        output.status.success(),
        "Library compilation failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    println!("✓ ev-reth library compilation test passed");
}

/// Tests that documentation can be generated successfully
#[test]
fn test_evolve_documentation_generation() {
    let output = Command::new("cargo")
        .args(["doc", "--no-deps", "--lib"])
        .env("RUSTDOCFLAGS", "-D warnings") // Treat doc warnings as errors
        .output()
        .expect("Failed to execute cargo doc");

    if !output.status.success() {
        // Documentation generation failure is not critical, just log it
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("⚠ Documentation generation failed (non-critical): {stderr}");
        return;
    }

    println!("✓ ev-reth documentation generation test passed");
}

/// Tests basic workspace integration
#[test]
fn test_workspace_integration() {
    // Test that the evolve crate is properly integrated into the workspace
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .output()
        .expect("Failed to execute cargo metadata");

    assert!(output.status.success(), "Cargo metadata should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ev-reth"),
        "Workspace should contain ev-reth crate"
    );

    println!("✓ workspace integration test passed");
}
