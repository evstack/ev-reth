use reth_ethereum::evm::revm::precompile::{secp256r1, Precompiles};
use std::sync::OnceLock;

pub(crate) fn custom_prague_precompiles() -> &'static Precompiles {
    static INSTANCE: OnceLock<Precompiles> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = Precompiles::prague().clone();

        // https://github.com/ethereum/RIPs/blob/master/RIPS/rip-7212.md
        precompiles.extend(secp256r1::precompiles());

        precompiles
    })
}
