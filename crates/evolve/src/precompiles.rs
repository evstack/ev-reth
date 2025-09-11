use reth_ethereum::evm::revm::precompile::Precompiles;
use std::sync::OnceLock;

pub(crate) fn custom_prague_precompiles() -> &'static Precompiles {
    static INSTANCE: OnceLock<Precompiles> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let precompiles = Precompiles::prague().clone();

        // TODO: Add RIP-7212.

        precompiles
    })
}
