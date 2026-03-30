//! Deploy pipeline: orchestrates the full deployment flow.

use crate::config::DeployConfig;
use crate::contracts;
use crate::deploy::create2::{compute_address, DETERMINISTIC_DEPLOYER};
use crate::deploy::deployer::ChainDeployer;
use crate::deploy::state::{ContractState, ContractStatus, DeployState};
use alloy_primitives::{Address, B256};
use std::path::{Path, PathBuf};

/// Configuration for the deploy pipeline.
pub(crate) struct PipelineConfig {
    pub config: DeployConfig,
    pub state_path: PathBuf,
    pub addresses_out: Option<PathBuf>,
}

/// Run the full deploy pipeline.
pub(crate) async fn run(
    pipeline_cfg: &PipelineConfig,
    deployer: &dyn ChainDeployer,
) -> eyre::Result<()> {
    // ── Step 1: Init ──
    eprintln!("[1/5] Connecting to RPC...");
    let chain_id = deployer.chain_id().await?;
    eprintln!("       chain_id={chain_id}");

    eyre::ensure!(
        chain_id == pipeline_cfg.config.chain.chain_id,
        "chain_id mismatch: config says {}, RPC reports {}",
        pipeline_cfg.config.chain.chain_id,
        chain_id
    );

    eprintln!("[2/5] Verifying deterministic deployer...");
    let deployer_code = deployer.get_code(DETERMINISTIC_DEPLOYER).await?;
    eyre::ensure!(
        !deployer_code.is_empty(),
        "deterministic deployer not found at {} -- deploy it before running ev-deployer deploy",
        DETERMINISTIC_DEPLOYER
    );
    eprintln!("       OK");

    // Load or create state
    let mut state = if pipeline_cfg.state_path.exists() {
        let state = DeployState::load(&pipeline_cfg.state_path)?;
        state.validate_immutability(&pipeline_cfg.config)?;
        state
    } else {
        let state = DeployState::new(&pipeline_cfg.config);
        state.save(&pipeline_cfg.state_path)?;
        state
    };

    let salt = state.create2_salt;

    // ── Step 2: Deploy AdminProxy ──
    if let Some(ref ap_config) = pipeline_cfg.config.contracts.admin_proxy {
        eprintln!("[3/5] Deploying AdminProxy...");

        if ap_config.address.is_some() {
            eprintln!("       WARN: contracts.admin_proxy.address is ignored in deploy mode");
        }

        let initcode = build_admin_proxy_initcode(ap_config.owner);
        let address = compute_address(salt, &initcode);

        deploy_contract(
            deployer,
            &mut state,
            "admin_proxy",
            address,
            salt,
            &initcode,
            contracts::admin_proxy::ADMIN_PROXY_BYTECODE,
            &pipeline_cfg.state_path,
        )
        .await?;
    } else {
        eprintln!("[3/5] AdminProxy not configured, skipping");
    }

    // ── Step 3: Deploy Permit2 ──
    if pipeline_cfg.config.contracts.permit2.is_some() {
        eprintln!("[4/5] Deploying Permit2...");

        if pipeline_cfg
            .config
            .contracts
            .permit2
            .as_ref()
            .unwrap()
            .address
            .is_some()
        {
            eprintln!("       WARN: contracts.permit2.address is ignored in deploy mode");
        }

        let initcode = contracts::permit2::PERMIT2_INITCODE.to_vec();
        let address = compute_address(salt, &initcode);

        let expected_runtime =
            contracts::permit2::expected_runtime_bytecode(chain_id, address);

        deploy_contract(
            deployer,
            &mut state,
            "permit2",
            address,
            salt,
            &initcode,
            &expected_runtime,
            &pipeline_cfg.state_path,
        )
        .await?;
    } else {
        eprintln!("[4/5] Permit2 not configured, skipping");
    }

    // ── Step 4: Verify ──
    eprintln!("[5/5] Verifying bytecodes...");
    verify_all(deployer, &mut state, &pipeline_cfg.config, chain_id).await?;
    state.save(&pipeline_cfg.state_path)?;
    eprintln!("       OK");

    // ── Step 5: Output ──
    eprintln!();
    eprintln!(
        "Deploy complete. State saved to {}",
        pipeline_cfg.state_path.display()
    );

    if let Some(ref addr_path) = pipeline_cfg.addresses_out {
        let manifest = build_deploy_manifest(&state);
        let json = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(addr_path, &json)?;
        eprintln!("Wrote address manifest to {}", addr_path.display());
    }

    Ok(())
}

/// Build AdminProxy initcode with constructor argument.
fn build_admin_proxy_initcode(owner: Address) -> Vec<u8> {
    let mut initcode = contracts::admin_proxy::ADMIN_PROXY_INITCODE.to_vec();
    // ABI-encode the owner address as a 32-byte word and append
    initcode.extend_from_slice(owner.into_word().as_slice());
    initcode
}

/// Deploy a single contract via CREATE2 with idempotency.
async fn deploy_contract(
    deployer: &dyn ChainDeployer,
    state: &mut DeployState,
    name: &str,
    address: Address,
    salt: B256,
    initcode: &[u8],
    expected_runtime: &[u8],
    state_path: &Path,
) -> eyre::Result<()> {
    // Check if already deployed or verified in state
    let current_status = get_contract_status(state, name);
    if current_status >= Some(ContractStatus::Deployed) {
        eprintln!("       already deployed at {address}, skipping");
        return Ok(());
    }

    // Idempotency: check if code already exists on-chain
    let existing_code = deployer.get_code(address).await?;
    if !existing_code.is_empty() {
        if existing_code.as_ref() == expected_runtime {
            eprintln!("       found matching bytecode at {address}, marking as deployed");
            set_contract_state(
                state,
                name,
                ContractState {
                    status: ContractStatus::Deployed,
                    address,
                    deploy_tx: None,
                },
            );
            state.save(state_path)?;
            return Ok(());
        } else {
            eyre::bail!(
                "unexpected bytecode at {address}: expected {} bytes, found {} bytes",
                expected_runtime.len(),
                existing_code.len()
            );
        }
    }

    // Deploy
    let receipt = deployer.deploy_create2(salt, initcode).await?;
    eyre::ensure!(
        receipt.success,
        "CREATE2 deploy tx reverted for {name}: tx={}",
        receipt.tx_hash
    );

    eprintln!("       tx={} address={address}", receipt.tx_hash);

    set_contract_state(
        state,
        name,
        ContractState {
            status: ContractStatus::Deployed,
            address,
            deploy_tx: Some(receipt.tx_hash),
        },
    );
    state.save(state_path)?;

    Ok(())
}

/// Verify all deployed contracts have matching on-chain bytecode.
async fn verify_all(
    deployer: &dyn ChainDeployer,
    state: &mut DeployState,
    _config: &DeployConfig,
    chain_id: u64,
) -> eyre::Result<()> {
    if let Some(ref cs) = state.contracts.admin_proxy {
        if cs.status == ContractStatus::Deployed {
            let on_chain = deployer.get_code(cs.address).await?;
            let expected = contracts::admin_proxy::ADMIN_PROXY_BYTECODE;
            eyre::ensure!(
                on_chain.as_ref() == expected,
                "bytecode mismatch at {}: expected {} bytes, got {} bytes",
                cs.address,
                expected.len(),
                on_chain.len()
            );
            let mut updated = cs.clone();
            updated.status = ContractStatus::Verified;
            state.contracts.admin_proxy = Some(updated);
        }
    }

    if let Some(ref cs) = state.contracts.permit2 {
        if cs.status == ContractStatus::Deployed {
            let on_chain = deployer.get_code(cs.address).await?;
            let expected =
                contracts::permit2::expected_runtime_bytecode(chain_id, cs.address);
            eyre::ensure!(
                on_chain.as_ref() == expected.as_slice(),
                "bytecode mismatch at {}: expected {} bytes, got {} bytes",
                cs.address,
                expected.len(),
                on_chain.len()
            );
            let mut updated = cs.clone();
            updated.status = ContractStatus::Verified;
            state.contracts.permit2 = Some(updated);
        }
    }

    Ok(())
}

fn get_contract_status(state: &DeployState, name: &str) -> Option<ContractStatus> {
    match name {
        "admin_proxy" => state.contracts.admin_proxy.as_ref().map(|c| c.status),
        "permit2" => state.contracts.permit2.as_ref().map(|c| c.status),
        _ => None,
    }
}

fn set_contract_state(state: &mut DeployState, name: &str, cs: ContractState) {
    match name {
        "admin_proxy" => state.contracts.admin_proxy = Some(cs),
        "permit2" => state.contracts.permit2 = Some(cs),
        _ => {}
    }
}

fn build_deploy_manifest(state: &DeployState) -> serde_json::Value {
    let mut manifest = serde_json::Map::new();
    if let Some(ref cs) = state.contracts.admin_proxy {
        manifest.insert(
            "admin_proxy".to_string(),
            serde_json::Value::String(format!("{}", cs.address)),
        );
    }
    if let Some(ref cs) = state.contracts.permit2 {
        manifest.insert(
            "permit2".to_string(),
            serde_json::Value::String(format!("{}", cs.address)),
        );
    }
    serde_json::Value::Object(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use crate::deploy::deployer::TxReceipt;
    use alloy_primitives::{address, Bytes};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock deployer for testing the pipeline without a live chain.
    struct MockDeployer {
        chain_id: u64,
        code: Mutex<HashMap<Address, Bytes>>,
        deploys: Mutex<Vec<(B256, Vec<u8>)>>,
    }

    impl MockDeployer {
        fn new(chain_id: u64) -> Self {
            let mut code = HashMap::new();
            code.insert(DETERMINISTIC_DEPLOYER, Bytes::from_static(&[0x01]));
            Self {
                chain_id,
                code: Mutex::new(code),
                deploys: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ChainDeployer for MockDeployer {
        async fn chain_id(&self) -> eyre::Result<u64> {
            Ok(self.chain_id)
        }

        async fn get_code(&self, address: Address) -> eyre::Result<Bytes> {
            Ok(self
                .code
                .lock()
                .unwrap()
                .get(&address)
                .cloned()
                .unwrap_or_default())
        }

        async fn deploy_create2(
            &self,
            salt: B256,
            initcode: &[u8],
        ) -> eyre::Result<TxReceipt> {
            self.deploys
                .lock()
                .unwrap()
                .push((salt, initcode.to_vec()));

            // Simulate: place the expected runtime bytecode at the computed address
            let address = compute_address(salt, initcode);

            // Determine which contract this is based on initcode
            let runtime =
                if initcode.len() > contracts::admin_proxy::ADMIN_PROXY_INITCODE.len()
                    && initcode[..contracts::admin_proxy::ADMIN_PROXY_INITCODE.len()]
                        == *contracts::admin_proxy::ADMIN_PROXY_INITCODE
                {
                    Bytes::from_static(contracts::admin_proxy::ADMIN_PROXY_BYTECODE)
                } else {
                    let runtime = contracts::permit2::expected_runtime_bytecode(
                        self.chain_id,
                        address,
                    );
                    Bytes::from(runtime)
                };

            self.code.lock().unwrap().insert(address, runtime);

            Ok(TxReceipt {
                tx_hash: B256::with_last_byte(0x01),
                success: true,
            })
        }
    }

    fn test_config() -> DeployConfig {
        DeployConfig {
            chain: ChainConfig { chain_id: 1234 },
            contracts: ContractsConfig {
                admin_proxy: Some(AdminProxyConfig {
                    address: None,
                    owner: address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                }),
                permit2: Some(Permit2Config { address: None }),
            },
        }
    }

    #[tokio::test]
    async fn pipeline_deploys_both_contracts() {
        let mock = MockDeployer::new(1234);
        let tmp_state = tempfile::NamedTempFile::new().unwrap();
        std::fs::remove_file(tmp_state.path()).unwrap();

        let cfg = PipelineConfig {
            config: test_config(),
            state_path: tmp_state.path().to_path_buf(),
            addresses_out: None,
        };

        run(&cfg, &mock).await.unwrap();

        let state = DeployState::load(tmp_state.path()).unwrap();
        assert_eq!(
            state.contracts.admin_proxy.as_ref().unwrap().status,
            ContractStatus::Verified
        );
        assert_eq!(
            state.contracts.permit2.as_ref().unwrap().status,
            ContractStatus::Verified
        );
        assert_eq!(mock.deploys.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn pipeline_skips_already_deployed() {
        let mock = MockDeployer::new(1234);
        let tmp_state = tempfile::NamedTempFile::new().unwrap();
        std::fs::remove_file(tmp_state.path()).unwrap();

        let cfg = PipelineConfig {
            config: test_config(),
            state_path: tmp_state.path().to_path_buf(),
            addresses_out: None,
        };

        // First run
        run(&cfg, &mock).await.unwrap();
        assert_eq!(mock.deploys.lock().unwrap().len(), 2);

        // Second run — should skip both
        run(&cfg, &mock).await.unwrap();
        assert_eq!(mock.deploys.lock().unwrap().len(), 2); // no new deploys
    }

    #[tokio::test]
    async fn pipeline_rejects_chain_id_mismatch() {
        let mock = MockDeployer::new(9999);
        let tmp_state = tempfile::NamedTempFile::new().unwrap();
        std::fs::remove_file(tmp_state.path()).unwrap();

        let cfg = PipelineConfig {
            config: test_config(), // chain_id = 1234
            state_path: tmp_state.path().to_path_buf(),
            addresses_out: None,
        };

        let err = run(&cfg, &mock).await.unwrap_err().to_string();
        assert!(err.contains("chain_id mismatch"), "{err}");
    }

    #[tokio::test]
    async fn pipeline_rejects_missing_deployer() {
        let mock = MockDeployer::new(1234);
        mock.code.lock().unwrap().remove(&DETERMINISTIC_DEPLOYER);

        let tmp_state = tempfile::NamedTempFile::new().unwrap();
        std::fs::remove_file(tmp_state.path()).unwrap();

        let cfg = PipelineConfig {
            config: test_config(),
            state_path: tmp_state.path().to_path_buf(),
            addresses_out: None,
        };

        let err = run(&cfg, &mock).await.unwrap_err().to_string();
        assert!(err.contains("deterministic deployer not found"), "{err}");
    }
}
