//! Dynamic config template generation for the `init` command.

/// Whether the config is for genesis injection or live CREATE2 deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitMode {
    /// Config for genesis injection.
    Genesis,
    /// Config for live CREATE2 deployment.
    Deploy,
}

/// Parameters for generating the init template.
#[derive(Debug)]
pub struct InitParams {
    /// Genesis or deploy mode.
    pub mode: InitMode,
    /// Target chain ID (written to `[chain]` section).
    pub chain_id: u64,
    /// Whether to enable the `Permit2` contract section.
    pub permit2: bool,
    /// Whether to include the deterministic deployer (Nick's factory).
    pub deterministic_deployer: bool,
    /// If set, enables `AdminProxy` with this owner address.
    pub admin_proxy_owner: Option<String>,
}

/// Generate a TOML config template based on the given parameters.
pub fn generate_template(params: &InitParams) -> String {
    let mut out = String::new();

    let is_genesis = params.mode == InitMode::Genesis;

    // In deploy mode, the deterministic deployer must already exist on-chain
    // (verified by the pipeline) — it cannot be deployed via CREATE2 itself.
    let deterministic_deployer = params.deterministic_deployer && is_genesis;

    // Header
    let mode_label = if is_genesis { "genesis" } else { "deploy" };
    out.push_str(&format!("# EV Deployer configuration ({mode_label} mode)\n"));
    out.push_str("# See: bin/ev-deployer/README.md\n");
    out.push('\n');

    // Chain
    out.push_str("[chain]\n");
    out.push_str("# The chain ID for the target network.\n");
    out.push_str(&format!("chain_id = {}\n", params.chain_id));

    // Contracts section header
    out.push('\n');
    out.push_str("# ── Contracts ────────────────────────────────────────────\n");
    if is_genesis {
        out.push_str("# Uncomment and configure the contracts you want in genesis.\n");
        out.push_str("# The `address` field is required for genesis mode.\n");
    } else {
        out.push_str("# Uncomment the contracts you want to deploy via CREATE2.\n");
        out.push_str("# Addresses are computed deterministically; no `address` field needed.\n");
    }

    // AdminProxy
    out.push('\n');
    out.push_str("# AdminProxy: transparent proxy with owner-based access control.\n");
    out.push_str("# The owner address is stored in slot 0.\n");
    if let Some(ref owner) = params.admin_proxy_owner {
        out.push_str("[contracts.admin_proxy]\n");
        if is_genesis {
            out.push_str("address = \"0x000000000000000000000000000000000000Ad00\"\n");
        }
        out.push_str(&format!("owner = \"{owner}\"\n"));
    } else {
        out.push_str("# [contracts.admin_proxy]\n");
        if is_genesis {
            out.push_str("# address = \"0x000000000000000000000000000000000000Ad00\"\n");
        }
        out.push_str("# owner = \"0x...\"\n");
    }

    // Permit2
    out.push('\n');
    out.push_str("# Permit2: Uniswap canonical token approval manager.\n");
    if params.permit2 {
        out.push_str("[contracts.permit2]\n");
        if is_genesis {
            out.push_str("address = \"0x000000000022D473030F116dDEE9F6B43aC78BA3\"\n");
        }
    } else {
        out.push_str("# [contracts.permit2]\n");
        if is_genesis {
            out.push_str("# address = \"0x000000000022D473030F116dDEE9F6B43aC78BA3\"\n");
        }
    }

    // Deterministic deployer (only relevant for genesis mode — in deploy mode
    // the pipeline verifies it exists on-chain, it cannot be deployed via CREATE2).
    if is_genesis {
        out.push('\n');
        out.push_str(
            "# Deterministic deployer (Nick's factory): CREATE2 factory for deploy mode.\n",
        );
        out.push_str(
            "# Required in genesis for post-merge chains where the keyless tx cannot land.\n",
        );
        if deterministic_deployer {
            out.push_str("[contracts.deterministic_deployer]\n");
            out.push_str("address = \"0x4e59b44847b379578588920cA78FbF26c0B4956C\"\n");
        } else {
            out.push_str("# [contracts.deterministic_deployer]\n");
            out.push_str("# address = \"0x4e59b44847b379578588920cA78FbF26c0B4956C\"\n");
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY_GENESIS_TEMPLATE: &str = include_str!("init_template.toml");

    #[test]
    fn genesis_default_matches_legacy_template() {
        let params = InitParams {
            mode: InitMode::Genesis,
            chain_id: 0,
            permit2: false,
            deterministic_deployer: false,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert_eq!(output, LEGACY_GENESIS_TEMPLATE);
    }

    #[test]
    fn genesis_custom_chain_id() {
        let params = InitParams {
            mode: InitMode::Genesis,
            chain_id: 42170,
            permit2: false,
            deterministic_deployer: false,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert!(output.contains("chain_id = 42170"), "{output}");
        assert!(output.contains("# [contracts.permit2]"), "{output}");
        assert!(output.contains("# [contracts.admin_proxy]"), "{output}");
    }

    #[test]
    fn genesis_permit2_includes_address() {
        let params = InitParams {
            mode: InitMode::Genesis,
            chain_id: 0,
            permit2: true,
            deterministic_deployer: false,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert!(output.contains("[contracts.permit2]\n"), "{output}");
        assert!(
            output.contains("address = \"0x000000000022D473030F116dDEE9F6B43aC78BA3\""),
            "{output}"
        );
    }

    #[test]
    fn genesis_admin_proxy_with_owner() {
        let params = InitParams {
            mode: InitMode::Genesis,
            chain_id: 0,
            permit2: false,
            deterministic_deployer: false,
            admin_proxy_owner: Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_string()),
        };
        let output = generate_template(&params);
        assert!(output.contains("[contracts.admin_proxy]\n"), "{output}");
        assert!(
            output.contains("address = \"0x000000000000000000000000000000000000Ad00\""),
            "{output}"
        );
        assert!(
            output.contains("owner = \"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266\""),
            "{output}"
        );
    }

    #[test]
    fn genesis_all_flags() {
        let params = InitParams {
            mode: InitMode::Genesis,
            chain_id: 1234,
            permit2: true,
            deterministic_deployer: true,
            admin_proxy_owner: Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_string()),
        };
        let output = generate_template(&params);
        assert!(output.contains("(genesis mode)"), "{output}");
        assert!(output.contains("chain_id = 1234"), "{output}");
        assert!(output.contains("[contracts.permit2]\n"), "{output}");
        assert!(output.contains("[contracts.admin_proxy]\n"), "{output}");
        assert!(
            output.contains("[contracts.deterministic_deployer]\n"),
            "{output}"
        );
    }

    #[test]
    fn genesis_deterministic_deployer_includes_address() {
        let params = InitParams {
            mode: InitMode::Genesis,
            chain_id: 0,
            permit2: false,
            deterministic_deployer: true,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert!(
            output.contains("[contracts.deterministic_deployer]\n"),
            "{output}"
        );
        assert!(
            output.contains("address = \"0x4e59b44847b379578588920cA78FbF26c0B4956C\""),
            "{output}"
        );
    }

    #[test]
    fn genesis_deterministic_deployer_disabled() {
        let params = InitParams {
            mode: InitMode::Genesis,
            chain_id: 0,
            permit2: false,
            deterministic_deployer: false,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert!(
            output.contains("# [contracts.deterministic_deployer]"),
            "{output}"
        );
    }

    // ── Deploy mode tests ──

    #[test]
    fn deploy_header() {
        let params = InitParams {
            mode: InitMode::Deploy,
            chain_id: 1234,
            permit2: false,
            deterministic_deployer: false,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert!(output.contains("(deploy mode)"), "{output}");
        assert!(
            output.contains("Addresses are computed deterministically"),
            "{output}"
        );
    }

    #[test]
    fn deploy_permit2_no_address() {
        let params = InitParams {
            mode: InitMode::Deploy,
            chain_id: 1234,
            permit2: true,
            deterministic_deployer: false,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert!(output.contains("[contracts.permit2]\n"), "{output}");
        assert!(
            !output.contains("address = \"0x000000000022D473030F116dDEE9F6B43aC78BA3\""),
            "deploy mode should not include address for permit2\n{output}"
        );
    }

    #[test]
    fn deploy_excludes_deterministic_deployer() {
        let params = InitParams {
            mode: InitMode::Deploy,
            chain_id: 1234,
            permit2: true,
            deterministic_deployer: false,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert!(
            !output.contains("deterministic_deployer"),
            "deploy mode should not include deterministic deployer section\n{output}"
        );
    }

    #[test]
    fn deploy_admin_proxy_no_address() {
        let params = InitParams {
            mode: InitMode::Deploy,
            chain_id: 1234,
            permit2: false,
            deterministic_deployer: false,
            admin_proxy_owner: Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_string()),
        };
        let output = generate_template(&params);
        assert!(output.contains("[contracts.admin_proxy]\n"), "{output}");
        assert!(
            !output.contains("address = \"0x000000000000000000000000000000000000Ad00\""),
            "deploy mode should not include address for admin_proxy\n{output}"
        );
        assert!(
            output.contains("owner = \"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266\""),
            "{output}"
        );
    }
}
