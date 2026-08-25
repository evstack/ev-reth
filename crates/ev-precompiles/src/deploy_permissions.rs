//! Stateful deployment-permissions precompile.

use alloy::{
    sol,
    sol_types::{SolInterface, SolValue},
};
use alloy_evm::{
    precompiles::{Precompile, PrecompileInput},
    revm::precompile::{PrecompileError, PrecompileId, PrecompileResult},
    EvmInternals, EvmInternalsError,
};
use alloy_primitives::{address, keccak256, Address, Bytes, U256};
use revm::{
    bytecode::Bytecode,
    precompile::{PrecompileHalt, PrecompileOutput},
};
use std::sync::{Arc, OnceLock};

sol! {
    interface IDeployPermissions {
        function addDeployer(address account) external;
        function removeDeployer(address account) external;
        function setEnabled(bool enabled) external;
        function isDeployerAllowed(address account) external view returns (bool);
        function isEnabled() external view returns (bool);
        function deployerCount() external view returns (uint256);
        function admin() external view returns (address);
    }
}

/// Address of the deployment-permissions precompile.
pub const DEPLOY_PERMISSIONS_PRECOMPILE_ADDR: Address =
    address!("0x000000000000000000000000000000000000F102");

/// Maximum number of active deployers.
pub const MAX_DEPLOYERS: usize = 1024;

const ENTRY_ALLOWED: U256 = U256::from_limbs([1, 0, 0, 0]);
const ENTRY_DENIED: U256 = U256::from_limbs([2, 0, 0, 0]);

fn domain_slot(domain: &'static [u8]) -> U256 {
    U256::from_be_bytes(keccak256(domain).0)
}

/// Storage slot recording whether enforcement is disabled.
pub fn disabled_slot() -> U256 {
    static SLOT: OnceLock<U256> = OnceLock::new();
    *SLOT.get_or_init(|| domain_slot(b"ev-reth.deploy-permissions.disabled.v1"))
}

/// Storage slot containing the encoded active-deployer count.
pub fn deployer_count_slot() -> U256 {
    static SLOT: OnceLock<U256> = OnceLock::new();
    *SLOT.get_or_init(|| domain_slot(b"ev-reth.deploy-permissions.count.v1"))
}

/// Returns the domain-separated storage slot for an address override.
pub fn deployer_override_slot(account: Address) -> U256 {
    static DOMAIN: OnceLock<[u8; 32]> = OnceLock::new();
    let domain = DOMAIN.get_or_init(|| keccak256(b"ev-reth.deploy-permissions.member.v1").0);
    let mut input = [0u8; 64];
    input[..32].copy_from_slice(domain);
    input[32..].copy_from_slice(account.into_word().as_slice());
    U256::from_be_bytes(keccak256(input).0)
}

/// Decodes a stored override against genesis baseline membership.
pub fn resolve_deployer_override(value: U256, baseline_member: bool) -> bool {
    if value == ENTRY_ALLOWED {
        true
    } else if value == ENTRY_DENIED {
        false
    } else {
        baseline_member
    }
}

/// A precompile that manages state-backed top-level deployment permissions.
#[derive(Clone, Debug, Default)]
pub struct DeployPermissionsPrecompile {
    admin: Address,
    baseline: Arc<[Address]>,
}

#[derive(Debug)]
enum DeployPermissionsError {
    Fatal(PrecompileError),
    Halt(PrecompileHalt),
}

type DeployPermissionsResult<T> = Result<T, DeployPermissionsError>;

impl DeployPermissionsError {
    fn fatal(err: EvmInternalsError) -> Self {
        Self::Fatal(PrecompileError::Fatal(err.to_string()))
    }

    const fn halt(reason: &'static str) -> Self {
        Self::Halt(PrecompileHalt::other_static(reason))
    }
}

impl DeployPermissionsPrecompile {
    /// Returns the stable custom precompile identifier.
    pub fn id() -> &'static PrecompileId {
        static ID: OnceLock<PrecompileId> = OnceLock::new();
        ID.get_or_init(|| PrecompileId::custom("deploy_permissions"))
    }

    fn bytecode() -> &'static Bytecode {
        static BYTECODE: OnceLock<Bytecode> = OnceLock::new();
        BYTECODE.get_or_init(|| Bytecode::new_raw(Bytes::from_static(&[0xFE])))
    }

    /// Creates a precompile using the fixed admin and genesis baseline.
    /// Sorted unique inputs are stored without copying.
    pub fn new(admin: Address, baseline: impl Into<Arc<[Address]>>) -> Self {
        let baseline = baseline.into();
        if baseline.windows(2).all(|pair| pair[0] < pair[1]) {
            return Self { admin, baseline };
        }
        let mut owned = Vec::from(baseline.as_ref());
        owned.sort_unstable();
        owned.dedup();
        Self {
            admin,
            baseline: Arc::from(owned),
        }
    }

    fn map_internals_error(err: EvmInternalsError) -> DeployPermissionsError {
        DeployPermissionsError::fatal(err)
    }

    fn is_baseline_member(&self, account: Address) -> bool {
        self.baseline.binary_search(&account).is_ok()
    }

    fn ensure_admin(&self, caller: Address) -> DeployPermissionsResult<()> {
        if caller == self.admin {
            Ok(())
        } else {
            Err(DeployPermissionsError::halt("unauthorized caller"))
        }
    }

    fn ensure_account_created(internals: &mut EvmInternals<'_>) -> DeployPermissionsResult<()> {
        let account = internals
            .load_account(DEPLOY_PERMISSIONS_PRECOMPILE_ADDR)
            .map_err(Self::map_internals_error)?;
        let needs_code = account.info.code_hash == alloy_primitives::KECCAK256_EMPTY;
        let needs_nonce = account.info.nonce == 0;
        if needs_code {
            internals
                .set_code(DEPLOY_PERMISSIONS_PRECOMPILE_ADDR, Self::bytecode().clone())
                .map_err(Self::map_internals_error)?;
        }
        if needs_nonce {
            internals
                .load_account_mut(DEPLOY_PERMISSIONS_PRECOMPILE_ADDR)
                .map_err(Self::map_internals_error)?
                .set_nonce(1);
        }
        if needs_code || needs_nonce {
            internals
                .touch_account(DEPLOY_PERMISSIONS_PRECOMPILE_ADDR)
                .map_err(Self::map_internals_error)?;
        }
        Ok(())
    }

    fn read_slot(internals: &mut EvmInternals<'_>, slot: U256) -> DeployPermissionsResult<U256> {
        let value = internals
            .sload(DEPLOY_PERMISSIONS_PRECOMPILE_ADDR, slot)
            .map_err(Self::map_internals_error)?;
        Ok(*value)
    }

    fn write_slot(
        internals: &mut EvmInternals<'_>,
        slot: U256,
        value: U256,
    ) -> DeployPermissionsResult<()> {
        Self::ensure_account_created(internals)?;
        internals
            .sstore(DEPLOY_PERMISSIONS_PRECOMPILE_ADDR, slot, value)
            .map_err(Self::map_internals_error)?;
        internals
            .touch_account(DEPLOY_PERMISSIONS_PRECOMPILE_ADDR)
            .map_err(Self::map_internals_error)?;
        Ok(())
    }

    fn is_enabled(internals: &mut EvmInternals<'_>) -> DeployPermissionsResult<bool> {
        Ok(Self::read_slot(internals, disabled_slot())?.is_zero())
    }

    fn deployer_count(&self, internals: &mut EvmInternals<'_>) -> DeployPermissionsResult<usize> {
        let encoded = Self::read_slot(internals, deployer_count_slot())?;
        if encoded.is_zero() {
            return Ok(self.baseline.len());
        }
        let count = usize::try_from(encoded - U256::from(1))
            .map_err(|_| DeployPermissionsError::halt("invalid deployment-permissions state"))?;
        if count > MAX_DEPLOYERS {
            return Err(DeployPermissionsError::halt(
                "invalid deployment-permissions state",
            ));
        }
        Ok(count)
    }

    fn set_deployer_count(
        internals: &mut EvmInternals<'_>,
        count: usize,
    ) -> DeployPermissionsResult<()> {
        let encoded = U256::from(count) + U256::from(1);
        Self::write_slot(internals, deployer_count_slot(), encoded)
    }

    fn is_deployer_allowed(
        &self,
        internals: &mut EvmInternals<'_>,
        account: Address,
    ) -> DeployPermissionsResult<bool> {
        if account.is_zero() {
            return Ok(false);
        }
        let value = Self::read_slot(internals, deployer_override_slot(account))?;
        Ok(resolve_deployer_override(
            value,
            self.is_baseline_member(account),
        ))
    }

    fn add_deployer(
        &self,
        internals: &mut EvmInternals<'_>,
        account: Address,
    ) -> DeployPermissionsResult<()> {
        if account.is_zero() {
            return Err(DeployPermissionsError::halt("deployer cannot be zero"));
        }
        if self.is_deployer_allowed(internals, account)? {
            return Ok(());
        }
        let count = self.deployer_count(internals)?;
        if count >= MAX_DEPLOYERS {
            return Err(DeployPermissionsError::halt("deployer limit reached"));
        }
        let value = if self.is_baseline_member(account) {
            U256::ZERO
        } else {
            ENTRY_ALLOWED
        };
        Self::write_slot(internals, deployer_override_slot(account), value)?;
        Self::set_deployer_count(internals, count + 1)
    }

    fn remove_deployer(
        &self,
        internals: &mut EvmInternals<'_>,
        account: Address,
    ) -> DeployPermissionsResult<()> {
        if account.is_zero() {
            return Err(DeployPermissionsError::halt("deployer cannot be zero"));
        }
        if !self.is_deployer_allowed(internals, account)? {
            return Ok(());
        }
        let count = self.deployer_count(internals)?;
        let value = if self.is_baseline_member(account) {
            ENTRY_DENIED
        } else {
            U256::ZERO
        };
        let next_count = count
            .checked_sub(1)
            .ok_or_else(|| DeployPermissionsError::halt("invalid deployment-permissions state"))?;
        Self::write_slot(internals, deployer_override_slot(account), value)?;
        Self::set_deployer_count(internals, next_count)
    }

    fn set_enabled(internals: &mut EvmInternals<'_>, enabled: bool) -> DeployPermissionsResult<()> {
        let currently_enabled = Self::is_enabled(internals)?;
        if currently_enabled == enabled {
            return Ok(());
        }
        let disabled = if enabled { U256::ZERO } else { U256::from(1) };
        Self::write_slot(internals, disabled_slot(), disabled)
    }
}

impl Precompile for DeployPermissionsPrecompile {
    fn precompile_id(&self) -> &PrecompileId {
        Self::id()
    }

    fn call(&self, mut input: PrecompileInput<'_>) -> PrecompileResult {
        let caller = input.caller;
        let reservoir = input.reservoir;
        let is_static = input.is_static;
        let decoded = match IDeployPermissions::IDeployPermissionsCalls::abi_decode(input.data) {
            Ok(value) => value,
            Err(err) => {
                return Ok(PrecompileOutput::halt(
                    PrecompileHalt::other(err.to_string()),
                    reservoir,
                ))
            }
        };
        let internals = input.internals_mut();

        let result = (|| -> DeployPermissionsResult<Bytes> {
            match decoded {
                IDeployPermissions::IDeployPermissionsCalls::addDeployer(call) => {
                    if is_static {
                        return Err(DeployPermissionsError::halt(
                            "state change during static call",
                        ));
                    }
                    self.ensure_admin(caller)?;
                    self.add_deployer(internals, call.account)?;
                    Ok(Bytes::new())
                }
                IDeployPermissions::IDeployPermissionsCalls::removeDeployer(call) => {
                    if is_static {
                        return Err(DeployPermissionsError::halt(
                            "state change during static call",
                        ));
                    }
                    self.ensure_admin(caller)?;
                    self.remove_deployer(internals, call.account)?;
                    Ok(Bytes::new())
                }
                IDeployPermissions::IDeployPermissionsCalls::setEnabled(call) => {
                    if is_static {
                        return Err(DeployPermissionsError::halt(
                            "state change during static call",
                        ));
                    }
                    self.ensure_admin(caller)?;
                    Self::set_enabled(internals, call.enabled)?;
                    Ok(Bytes::new())
                }
                IDeployPermissions::IDeployPermissionsCalls::isDeployerAllowed(call) => Ok(self
                    .is_deployer_allowed(internals, call.account)?
                    .abi_encode()
                    .into()),
                IDeployPermissions::IDeployPermissionsCalls::isEnabled(_) => {
                    Ok(Self::is_enabled(internals)?.abi_encode().into())
                }
                IDeployPermissions::IDeployPermissionsCalls::deployerCount(_) => {
                    Ok(U256::from(self.deployer_count(internals)?)
                        .abi_encode()
                        .into())
                }
                IDeployPermissions::IDeployPermissionsCalls::admin(_) => {
                    Ok(self.admin.abi_encode().into())
                }
            }
        })();

        match result {
            Ok(bytes) => Ok(PrecompileOutput::new(0, bytes, reservoir)),
            Err(DeployPermissionsError::Halt(reason)) => {
                Ok(PrecompileOutput::halt(reason, reservoir))
            }
            Err(DeployPermissionsError::Fatal(err)) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::SolCall;
    use alloy_primitives::address;
    use revm::{
        context::{
            journal::{Journal, JournalInner},
            BlockEnv, CfgEnv, TxEnv,
        },
        context_interface::JournalTr,
        database::{CacheDB, EmptyDB},
        primitives::hardfork::SpecId,
    };

    type TestJournal = Journal<CacheDB<EmptyDB>>;
    const GAS_LIMIT: u64 = 1_000_000;

    fn setup_context() -> (TestJournal, BlockEnv, CfgEnv, TxEnv) {
        let mut journal = Journal::new_with_inner(CacheDB::default(), JournalInner::new());
        journal.inner.set_spec_id(SpecId::PRAGUE);
        (
            journal,
            BlockEnv::default(),
            CfgEnv::default(),
            TxEnv::default(),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "test helper mirrors the complete stateful precompile call context"
    )]
    fn run_call<'a>(
        journal: &'a mut TestJournal,
        block_env: &'a BlockEnv,
        cfg_env: &'a CfgEnv,
        tx_env: &'a TxEnv,
        precompile: &DeployPermissionsPrecompile,
        caller: Address,
        data: &'a [u8],
        is_static: bool,
    ) -> PrecompileResult {
        precompile.call(PrecompileInput {
            data,
            gas: GAS_LIMIT,
            reservoir: 0,
            caller,
            value: U256::ZERO,
            target_address: DEPLOY_PERMISSIONS_PRECOMPILE_ADDR,
            is_static,
            bytecode_address: DEPLOY_PERMISSIONS_PRECOMPILE_ADDR,
            internals: EvmInternals::new(journal, block_env, cfg_env, tx_env),
        })
    }

    fn output_bytes(result: PrecompileResult) -> Bytes {
        match result {
            Ok(output) if !output.is_halt() => output.bytes.clone(),
            Ok(output) => panic!("expected success, got halt {output:?}"),
            Err(err) => panic!("expected success, got fatal error {err:?}"),
        }
    }

    fn assert_halt(result: PrecompileResult, expected: &str) {
        match result {
            Ok(output) => match output.halt_reason() {
                Some(PrecompileHalt::Other(message)) => assert_eq!(message.as_ref(), expected),
                other => panic!("expected custom halt, got {other:?}"),
            },
            Err(err) => panic!("expected halt, got fatal error {err:?}"),
        }
    }

    fn call_bool(
        journal: &mut TestJournal,
        block_env: &BlockEnv,
        cfg_env: &CfgEnv,
        tx_env: &TxEnv,
        precompile: &DeployPermissionsPrecompile,
        data: &[u8],
    ) -> bool {
        bool::abi_decode(&output_bytes(run_call(
            journal,
            block_env,
            cfg_env,
            tx_env,
            precompile,
            Address::ZERO,
            data,
            true,
        )))
        .expect("bool output decodes")
    }

    fn count(
        journal: &mut TestJournal,
        block_env: &BlockEnv,
        cfg_env: &CfgEnv,
        tx_env: &TxEnv,
        precompile: &DeployPermissionsPrecompile,
    ) -> U256 {
        U256::abi_decode(&output_bytes(run_call(
            journal,
            block_env,
            cfg_env,
            tx_env,
            precompile,
            Address::ZERO,
            &IDeployPermissions::deployerCountCall {}.abi_encode(),
            true,
        )))
        .expect("count output decodes")
    }

    #[test]
    fn configured_policy_is_enabled_by_default_and_uses_baseline() {
        let admin = address!("0x00000000000000000000000000000000000000aa");
        let baseline = address!("0x00000000000000000000000000000000000000bb");
        let other = address!("0x00000000000000000000000000000000000000cc");
        let precompile = DeployPermissionsPrecompile::new(admin, vec![baseline]);
        let (mut journal, block, cfg, tx) = setup_context();

        assert!(call_bool(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            &IDeployPermissions::isEnabledCall {}.abi_encode(),
        ));
        assert!(call_bool(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            &IDeployPermissions::isDeployerAllowedCall { account: baseline }.abi_encode(),
        ));
        assert!(!call_bool(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            &IDeployPermissions::isDeployerAllowedCall { account: other }.abi_encode(),
        ));
        assert_eq!(
            count(&mut journal, &block, &cfg, &tx, &precompile),
            U256::from(1)
        );
    }

    #[test]
    fn admin_can_disable_edit_policy_and_reenable() {
        let admin = address!("0x00000000000000000000000000000000000000aa");
        let baseline = address!("0x00000000000000000000000000000000000000bb");
        let added = address!("0x00000000000000000000000000000000000000cc");
        let precompile = DeployPermissionsPrecompile::new(admin, vec![baseline]);
        let (mut journal, block, cfg, tx) = setup_context();

        for data in [
            IDeployPermissions::setEnabledCall { enabled: false }.abi_encode(),
            IDeployPermissions::removeDeployerCall { account: baseline }.abi_encode(),
            IDeployPermissions::addDeployerCall { account: added }.abi_encode(),
        ] {
            output_bytes(run_call(
                &mut journal,
                &block,
                &cfg,
                &tx,
                &precompile,
                admin,
                &data,
                false,
            ));
        }

        assert!(!call_bool(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            &IDeployPermissions::isEnabledCall {}.abi_encode(),
        ));
        assert!(!call_bool(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            &IDeployPermissions::isDeployerAllowedCall { account: baseline }.abi_encode(),
        ));
        assert!(call_bool(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            &IDeployPermissions::isDeployerAllowedCall { account: added }.abi_encode(),
        ));
        assert_eq!(
            count(&mut journal, &block, &cfg, &tx, &precompile),
            U256::from(1)
        );

        let enable = IDeployPermissions::setEnabledCall { enabled: true }.abi_encode();
        output_bytes(run_call(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            admin,
            &enable,
            false,
        ));
        assert!(call_bool(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            &IDeployPermissions::isEnabledCall {}.abi_encode(),
        ));
    }

    #[test]
    fn mutations_are_authorized_idempotent_and_reject_zero() {
        let admin = address!("0x00000000000000000000000000000000000000aa");
        let caller = address!("0x00000000000000000000000000000000000000bb");
        let account = address!("0x00000000000000000000000000000000000000cc");
        let precompile = DeployPermissionsPrecompile::new(admin, Vec::new());
        let (mut journal, block, cfg, tx) = setup_context();
        let add = IDeployPermissions::addDeployerCall { account }.abi_encode();

        assert_halt(
            run_call(
                &mut journal,
                &block,
                &cfg,
                &tx,
                &precompile,
                caller,
                &add,
                false,
            ),
            "unauthorized caller",
        );
        assert_halt(
            run_call(
                &mut journal,
                &block,
                &cfg,
                &tx,
                &precompile,
                admin,
                &add,
                true,
            ),
            "state change during static call",
        );
        for _ in 0..2 {
            output_bytes(run_call(
                &mut journal,
                &block,
                &cfg,
                &tx,
                &precompile,
                admin,
                &add,
                false,
            ));
        }
        assert_eq!(
            count(&mut journal, &block, &cfg, &tx, &precompile),
            U256::from(1)
        );

        let remove = IDeployPermissions::removeDeployerCall { account }.abi_encode();
        for _ in 0..2 {
            output_bytes(run_call(
                &mut journal,
                &block,
                &cfg,
                &tx,
                &precompile,
                admin,
                &remove,
                false,
            ));
        }
        assert_eq!(
            count(&mut journal, &block, &cfg, &tx, &precompile),
            U256::ZERO
        );

        let zero = IDeployPermissions::addDeployerCall {
            account: Address::ZERO,
        }
        .abi_encode();
        assert_halt(
            run_call(
                &mut journal,
                &block,
                &cfg,
                &tx,
                &precompile,
                admin,
                &zero,
                false,
            ),
            "deployer cannot be zero",
        );
    }

    #[test]
    fn removals_clear_dynamic_entries_and_only_baseline_removals_leave_tombstones() {
        let admin = address!("0x00000000000000000000000000000000000000aa");
        let baseline = address!("0x00000000000000000000000000000000000000bb");
        let dynamic = address!("0x00000000000000000000000000000000000000cc");
        let precompile = DeployPermissionsPrecompile::new(admin, vec![baseline]);
        let (mut journal, block, cfg, tx) = setup_context();

        for data in [
            IDeployPermissions::removeDeployerCall { account: baseline }.abi_encode(),
            IDeployPermissions::addDeployerCall { account: dynamic }.abi_encode(),
            IDeployPermissions::removeDeployerCall { account: dynamic }.abi_encode(),
        ] {
            output_bytes(run_call(
                &mut journal,
                &block,
                &cfg,
                &tx,
                &precompile,
                admin,
                &data,
                false,
            ));
        }

        let account = journal
            .inner
            .state
            .get(&DEPLOY_PERMISSIONS_PRECOMPILE_ADDR)
            .expect("precompile account exists");
        assert_eq!(account.info.nonce, 1);
        assert_ne!(account.info.code_hash, alloy_primitives::KECCAK256_EMPTY);
        assert_eq!(
            account.storage[&deployer_override_slot(baseline)].present_value,
            ENTRY_DENIED
        );
        assert_eq!(
            account.storage[&deployer_override_slot(dynamic)].present_value,
            U256::ZERO,
            "removed non-baseline entries must be cleared"
        );

        let readd = IDeployPermissions::addDeployerCall { account: baseline }.abi_encode();
        output_bytes(run_call(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            admin,
            &readd,
            false,
        ));
        let account = journal
            .inner
            .state
            .get(&DEPLOY_PERMISSIONS_PRECOMPILE_ADDR)
            .expect("precompile account exists");
        assert_eq!(
            account.storage[&deployer_override_slot(baseline)].present_value,
            U256::ZERO,
            "re-adding a baseline member must clear its tombstone"
        );
    }

    #[test]
    fn reverted_control_call_restores_policy_and_enabled_flag() {
        let admin = address!("0x00000000000000000000000000000000000000aa");
        let account = address!("0x00000000000000000000000000000000000000bb");
        let precompile = DeployPermissionsPrecompile::new(admin, Vec::new());
        let (mut journal, block, cfg, tx) = setup_context();
        let checkpoint = journal.checkpoint();

        for data in [
            IDeployPermissions::addDeployerCall { account }.abi_encode(),
            IDeployPermissions::setEnabledCall { enabled: false }.abi_encode(),
        ] {
            output_bytes(run_call(
                &mut journal,
                &block,
                &cfg,
                &tx,
                &precompile,
                admin,
                &data,
                false,
            ));
        }
        journal.checkpoint_revert(checkpoint);

        assert!(call_bool(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            &IDeployPermissions::isEnabledCall {}.abi_encode(),
        ));
        assert!(!call_bool(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            &IDeployPermissions::isDeployerAllowedCall { account }.abi_encode(),
        ));
        assert_eq!(
            count(&mut journal, &block, &cfg, &tx, &precompile),
            U256::ZERO
        );
    }

    #[test]
    fn exposes_fixed_admin_and_rejects_malformed_calldata() {
        let admin = address!("0x00000000000000000000000000000000000000aa");
        let precompile = DeployPermissionsPrecompile::new(admin, Vec::new());
        let (mut journal, block, cfg, tx) = setup_context();
        let bytes = output_bytes(run_call(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            Address::ZERO,
            &IDeployPermissions::adminCall {}.abi_encode(),
            true,
        ));
        assert_eq!(
            Address::abi_decode(&bytes).expect("admin output decodes"),
            admin
        );

        let malformed = run_call(
            &mut journal,
            &block,
            &cfg,
            &tx,
            &precompile,
            admin,
            &[0xde, 0xad, 0xbe, 0xef],
            false,
        )
        .expect("malformed calldata produces a halt output");
        assert!(malformed.is_halt());
    }

    #[test]
    fn rejects_addition_beyond_cap() {
        let admin = address!("0x00000000000000000000000000000000000000aa");
        let baseline: Vec<_> = (1..=MAX_DEPLOYERS)
            .map(|value| Address::from_word(U256::from(value).into()))
            .collect();
        let precompile = DeployPermissionsPrecompile::new(admin, baseline);
        let extra = address!("0x000000000000000000000000000000000000ffff");
        let (mut journal, block, cfg, tx) = setup_context();
        let data = IDeployPermissions::addDeployerCall { account: extra }.abi_encode();

        assert_halt(
            run_call(
                &mut journal,
                &block,
                &cfg,
                &tx,
                &precompile,
                admin,
                &data,
                false,
            ),
            "deployer limit reached",
        );
    }
}
