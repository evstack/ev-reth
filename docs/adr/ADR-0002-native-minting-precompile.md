# ADR 0002: Native Token Minting Precompile

## Changelog

* 2025-10-20: Initial draft
* 2025-10-20: Added implementation details and test coverage

## Status

PROPOSED - Implemented

## Abstract

Evolve requires a mechanism for privileged accounts to mint and burn the native token (ETH equivalent). This ADR describes a custom precompile for native token supply management. The precompile is installed at a dedicated address and exposes `mint` and `burn` functions, callable only by a designated admin account or addresses on a dynamic allowlist. This approach provides a secure and flexible way to manage the native token supply without altering the core EVM logic.

## Context

In order to bootstrap the network and provide liquidity, Evolve requires a native token minting and burning functionality. This functionality must be restricted to authorized accounts to prevent malicious supply inflation. The system must be flexible enough to allow for different authorization models, such as multisig wallets or governance contracts.

Traditional Ethereum execution clients don't support native token minting post-genesis. Evolve's unique architecture, where transactions are submitted directly through the Engine API, creates new requirements for token supply management. The solution needs to:
- Enable controlled minting and burning of native tokens
- Support flexible authorization mechanisms
- Maintain compatibility with existing EVM tooling
- Provide a secure and auditable approach to supply management

## Alternatives

### 1. Minting via a Smart Contract

A standard smart contract could be used to manage the native token supply. This contract would hold a large amount of tokens at genesis and distribute them based on its internal logic.

*   **Pros:** No changes to the EVM are required. The logic is transparent and upgradeable.
*   **Cons:** The entire token supply must be created at genesis, which can be inflexible. The contract would be a single point of failure.

### 2. Modifying the EVM Core

The EVM core could be modified to include a new opcode for minting tokens.

*   **Pros:** Potentially more performant than a precompile.
*   **Cons:** Highly invasive and complex to implement. It would create a significant divergence from upstream Reth and break compatibility with existing tools.

## Decision

We will implement a custom precompile for native token minting. The precompile will be located at address `0x000000000000000000000000000000000000f100` and expose the following interface:

```solidity
interface INativeToken {
    function mint(address to, uint256 amount);
    function burn(address from, uint256 amount);
    function addToAllowList(address account);
    function removeFromAllowList(address account);
}
```

Authorization is managed through a combination of a genesis-configured `mintAdmin` address and a dynamic `allowlist` stored in the precompile's state.

*   The `mintAdmin` is the only account that can add or remove addresses from the `allowlist`.
*   Both the `mintAdmin` and the addresses on the `allowlist` can call `mint` and `burn`.

This design provides a flexible and secure way to manage the native token supply. The `mintAdmin` can be a simple EOA for testing or a complex smart contract (e.g., a multisig wallet) for production environments.

### Implementation Details

The precompile implementation includes:

1. **Account Management**: Automatic account creation when minting to non-existent addresses, with proper state tracking and journaling
2. **Balance Operations**: Direct state manipulation using `EvmInternals` API with overflow/underflow protection
3. **Authorization Storage**: Allowlist stored in the precompile's own state using address-derived storage keys
4. **Gas Efficiency**: Minimal gas consumption with unused gas returned to caller
5. **State Safety**: All accounts properly marked as touched and created for EVM state consistency

## Consequences

### Backwards Compatibility

This precompile introduces a new feature that doesn't exist in standard Ethereum. However, it maintains backwards compatibility by:
- Using a reserved precompile address that won't conflict with existing contracts
- Following standard Solidity ABI encoding for function calls
- Not modifying any existing EVM opcodes or behavior
- Being fully optional - networks can run without configuring a mint admin

### Positive

*   **Flexibility:** The allowlist mechanism allows for dynamic and granular control over minting and burning permissions.
*   **Security:** The precompile is a simple and well-tested component, reducing the risk of bugs. Authorization is enforced at the EVM level.
*   **Compatibility:** The precompile is a self-contained unit that does not affect other parts of the EVM. Standard tools can interact with it.
*   **Upgradability:** The mint admin can be a upgradeable smart contract, allowing for governance evolution without hard forks.
*   **Auditability:** All mint and burn operations are recorded on-chain as regular transactions.

### Negative

*   **Gas Costs:** While the precompile itself is efficient, any authorization logic implemented in a `mintAdmin` smart contract will incur gas costs.
*   **Centralization Risk:** The mint admin represents a centralized control point, though this can be mitigated through multisig or DAO governance.

### Neutral

*   The precompile introduces a new, non-standard feature. While it is self-contained, it is a deviation from the standard Ethereum protocol.
*   Networks must carefully consider the security implications of enabling native token minting.

## Test Cases

The implementation includes comprehensive test coverage for all functionality:

### Unit Tests (`crates/ev-precompiles/src/mint.rs`)
- **Authorization**: Validates that only admin can modify allowlist and only authorized addresses can mint/burn
- **Minting**: Tests balance increases, account creation for non-existent addresses, and overflow protection
- **Burning**: Tests balance decreases and underflow protection
- **Allowlist Management**: Tests adding/removing addresses and immediate permission changes
- **State Management**: Validates proper account touching and creation flags

### Integration Tests (`crates/tests/src/e2e_tests.rs`)
- **End-to-end minting**: Tests minting through full transaction flow via RPC
- **Authorization scenarios**: Tests various permission configurations
- **Error conditions**: Validates proper error handling for unauthorized access and invalid operations

All test cases verify that state changes are properly journaled and can be reverted if needed.

### Security Considerations

- The mint admin private key or smart contract security is critical - compromise would allow unlimited minting
- Networks should consider implementing monitoring and alerting for unusual mint/burn patterns
- For production use, the mint admin should ideally be a well-audited multisig or governance contract

## References

*   `crates/ev-precompiles/src/mint.rs`
*   `crates/ev-precompiles/README.md`
