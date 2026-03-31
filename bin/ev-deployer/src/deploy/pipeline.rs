//! Deploy pipeline: orchestrates the full deployment flow.

use crate::{
    config::DeployConfig,
    contracts,
    deploy::{
        create2::{compute_address, DETERMINISTIC_DEPLOYER},
        deployer::ChainDeployer,
        state::{ContractState, ContractStatus, DeployState},
    },
};
use alloy_primitives::{Address, B256};
use std::path::{Path, PathBuf};

/// Configuration for the deploy pipeline.
#[derive(Debug)]
pub struct PipelineConfig {
    /// Parsed deploy configuration (chain + contracts).
    pub config: DeployConfig,
    /// Path to the JSON state file for idempotent deploys.
    pub state_path: PathBuf,
    /// Optional path to write the final address manifest.
    pub addresses_out: Option<PathBuf>,
}

/// Run the full deploy pipeline.
pub async fn run(pipeline_cfg: &PipelineConfig, deployer: &dyn ChainDeployer) -> eyre::Result<()> {
    // ── Step 1: Init ──
    eprintln!("[1/4] Connecting to RPC...");
    let chain_id = deployer.chain_id().await?;
    eprintln!("       chain_id={chain_id}");

    eyre::ensure!(
        chain_id == pipeline_cfg.config.chain.chain_id,
        "chain_id mismatch: config says {}, RPC reports {}",
        pipeline_cfg.config.chain.chain_id,
        chain_id
    );

    eprintln!("[2/4] Verifying deterministic deployer...");
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

    // ── Step 2: Deploy Permit2 ──
    if let Some(ref p2_config) = pipeline_cfg.config.contracts.permit2 {
        eprintln!("[3/4] Deploying Permit2...");

        if p2_config.address.is_some() {
            eprintln!("       WARN: contracts.permit2.address is ignored in deploy mode");
        }

        let initcode = contracts::permit2::PERMIT2_INITCODE.to_vec();
        let address = compute_address(salt, &initcode);

        let expected_runtime = contracts::permit2::expected_runtime_bytecode(chain_id, address);

        deploy_contract(
            deployer,
            &mut state,
            &DeployContractParams {
                name: "permit2",
                address,
                salt,
                initcode: &initcode,
                expected_runtime: &expected_runtime,
                state_path: &pipeline_cfg.state_path,
            },
        )
        .await?;
    } else {
        eprintln!("[3/4] Permit2 not configured, skipping");
    }

    // ── Step 3: Verify ──
    eprintln!("[4/4] Verifying bytecodes...");
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

/// Parameters for deploying a single contract.
struct DeployContractParams<'a> {
    name: &'a str,
    address: Address,
    salt: B256,
    initcode: &'a [u8],
    expected_runtime: &'a [u8],
    state_path: &'a Path,
}

/// Deploy a single contract via CREATE2 with idempotency.
async fn deploy_contract(
    deployer: &dyn ChainDeployer,
    state: &mut DeployState,
    params: &DeployContractParams<'_>,
) -> eyre::Result<()> {
    let DeployContractParams {
        name,
        address,
        salt,
        initcode,
        expected_runtime,
        state_path,
    } = params;
    // Check if already deployed or verified in state
    let current_status = get_contract_status(state, name);
    if current_status >= Some(ContractStatus::Deployed) {
        eprintln!("       already deployed at {address}, skipping");
        return Ok(());
    }

    // Idempotency: check if code already exists on-chain
    let existing_code = deployer.get_code(*address).await?;
    if !existing_code.is_empty() {
        if existing_code.as_ref() == *expected_runtime {
            eprintln!("       found matching bytecode at {address}, marking as deployed");
            set_contract_state(
                state,
                name,
                ContractState {
                    status: ContractStatus::Deployed,
                    address: *address,
                    deploy_tx: None,
                },
            );
            state.save(state_path)?;
            return Ok(());
        }
        eyre::bail!(
            "unexpected bytecode at {address}: expected {} bytes, found {} bytes",
            expected_runtime.len(),
            existing_code.len()
        );
    }

    // Deploy
    let receipt = deployer.deploy_create2(*salt, initcode).await?;
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
            address: *address,
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
    if let Some(ref cs) = state.contracts.permit2 {
        if cs.status == ContractStatus::Deployed {
            let on_chain = deployer.get_code(cs.address).await?;
            let expected = contracts::permit2::expected_runtime_bytecode(chain_id, cs.address);
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
    if name == "permit2" {
        state.contracts.permit2.as_ref().map(|c| c.status)
    } else {
        None
    }
}

fn set_contract_state(state: &mut DeployState, name: &str, cs: ContractState) {
    if name == "permit2" {
        state.contracts.permit2 = Some(cs);
    }
}

fn build_deploy_manifest(state: &DeployState) -> serde_json::Value {
    let mut manifest = serde_json::Map::new();
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
    use crate::{config::*, deploy::deployer::TxReceipt};
    use alloy_primitives::Bytes;
    use async_trait::async_trait;
    use std::{collections::HashMap, sync::Mutex};

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

        async fn deploy_create2(&self, salt: B256, initcode: &[u8]) -> eyre::Result<TxReceipt> {
            self.deploys.lock().unwrap().push((salt, initcode.to_vec()));

            // Simulate: place the expected runtime bytecode at the computed address
            let address = compute_address(salt, initcode);

            let runtime = contracts::permit2::expected_runtime_bytecode(self.chain_id, address);
            self.code
                .lock()
                .unwrap()
                .insert(address, Bytes::from(runtime));

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
                admin_proxy: None,
                permit2: Some(Permit2Config { address: None }),
            },
        }
    }

    #[tokio::test]
    async fn pipeline_deploys_permit2() {
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
            state.contracts.permit2.as_ref().unwrap().status,
            ContractStatus::Verified
        );
        assert_eq!(mock.deploys.lock().unwrap().len(), 1);
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
        assert_eq!(mock.deploys.lock().unwrap().len(), 1);

        // Second run — should skip
        run(&cfg, &mock).await.unwrap();
        assert_eq!(mock.deploys.lock().unwrap().len(), 1); // no new deploys
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
