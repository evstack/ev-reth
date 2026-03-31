//! Dynamic config template generation for the `init` command.

/// Parameters for generating the init template.
#[derive(Debug)]
pub struct InitParams {
    /// Target chain ID.
    pub chain_id: u64,
    /// Whether to include Permit2 with its canonical address.
    pub permit2: bool,
    /// Optional AdminProxy owner address.
    pub admin_proxy_owner: Option<String>,
}

/// Generate a TOML config template based on the given parameters.
pub fn generate_template(params: &InitParams) -> String {
    let mut out = String::new();

    // Header
    out.push_str("# EV Deployer configuration\n");
    out.push_str("# See: bin/ev-deployer/README.md\n");
    out.push('\n');

    // Chain
    out.push_str("[chain]\n");
    out.push_str("# The chain ID for the target network.\n");
    out.push_str(&format!("chain_id = {}\n", params.chain_id));

    // Contracts section header
    out.push('\n');
    out.push_str("# ── Contracts ────────────────────────────────────────────\n");
    out.push_str("# Uncomment and configure the contracts you want to deploy.\n");
    out.push_str("# The `address` field is required for `genesis` mode but\n");
    out.push_str("# ignored in `deploy` mode (addresses come from CREATE2).\n");

    // AdminProxy
    out.push('\n');
    out.push_str("# AdminProxy: transparent proxy with owner-based access control.\n");
    out.push_str("# The owner address is stored in slot 0.\n");
    if let Some(ref owner) = params.admin_proxy_owner {
        out.push_str("[contracts.admin_proxy]\n");
        out.push_str("address = \"0x000000000000000000000000000000000000Ad00\"\n");
        out.push_str(&format!("owner = \"{owner}\"\n"));
    } else {
        out.push_str("# [contracts.admin_proxy]\n");
        out.push_str("# address = \"0x000000000000000000000000000000000000Ad00\"\n");
        out.push_str("# owner = \"0x...\"\n");
    }

    // Permit2
    out.push('\n');
    out.push_str("# Permit2: Uniswap canonical token approval manager.\n");
    if params.permit2 {
        out.push_str("[contracts.permit2]\n");
        out.push_str("address = \"0x000000000022D473030F116dDEE9F6B43aC78BA3\"\n");
    } else {
        out.push_str("# [contracts.permit2]\n");
        out.push_str("# address = \"0x000000000022D473030F116dDEE9F6B43aC78BA3\"\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The static template that the old `init` used to emit.
    const LEGACY_TEMPLATE: &str = include_str!("init_template.toml");

    #[test]
    fn default_params_match_legacy_template() {
        let params = InitParams {
            chain_id: 0,
            permit2: false,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert_eq!(output, LEGACY_TEMPLATE);
    }

    #[test]
    fn custom_chain_id() {
        let params = InitParams {
            chain_id: 42170,
            permit2: false,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert!(output.contains("chain_id = 42170"), "{output}");
        assert!(output.contains("# [contracts.permit2]"), "{output}");
        assert!(output.contains("# [contracts.admin_proxy]"), "{output}");
    }

    #[test]
    fn permit2_enabled() {
        let params = InitParams {
            chain_id: 0,
            permit2: true,
            admin_proxy_owner: None,
        };
        let output = generate_template(&params);
        assert!(output.contains("[contracts.permit2]\n"), "{output}");
        assert!(
            output.contains("address = \"0x000000000022D473030F116dDEE9F6B43aC78BA3\""),
            "{output}"
        );
        assert!(output.contains("# [contracts.admin_proxy]"), "{output}");
    }

    #[test]
    fn admin_proxy_with_owner() {
        let params = InitParams {
            chain_id: 0,
            permit2: false,
            admin_proxy_owner: Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_string()),
        };
        let output = generate_template(&params);
        assert!(output.contains("[contracts.admin_proxy]\n"), "{output}");
        assert!(
            output.contains("owner = \"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266\""),
            "{output}"
        );
        assert!(output.contains("# [contracts.permit2]"), "{output}");
    }

    #[test]
    fn all_flags_combined() {
        let params = InitParams {
            chain_id: 1234,
            permit2: true,
            admin_proxy_owner: Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".to_string()),
        };
        let output = generate_template(&params);
        assert!(output.contains("chain_id = 1234"), "{output}");
        assert!(output.contains("[contracts.permit2]\n"), "{output}");
        assert!(output.contains("[contracts.admin_proxy]\n"), "{output}");
        assert!(
            output.contains("owner = \"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266\""),
            "{output}"
        );
    }
}
