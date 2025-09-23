use fee_handlers::{compute::*, types::*};

#[test]
fn base_fee_math() {
    // 30,000,000 gas * 1 gwei
    let wei = compute_base_fee_wei(1_000_000_000, 30_000_000);
    assert_eq!(wei, 30_000_000u128 * 1_000_000_000u128);
}

#[test]
fn l1_fee_v1_celestia_math() {
    let params = L1FeeParams::V1 {
        v1: V1Params {
            share_size: 512,
            overhead_shares: 1,
            blob_price_scalar: 1_000_000,
            decimals: 6,
        },
    };
    // 120_000 bytes -> ceil(120000/512)=235 shares; +1 overhead = 236 shares
    let txb = TxBytesAcc {
        total_size: 120_000,
        ..Default::default()
    };
    let l1_blob_base_fee = 2_000_000_000u128; // 2 gwei per share in L2 wei units
    let fee = compute_l1_fee_wei(&params, &txb, 0, l1_blob_base_fee);
    // 236 * 2e9 * 1e6 / 1e6 = 236 * 2e9
    let expected = 236u128 * 2_000_000_000u128;
    assert_eq!(fee, expected);
}
