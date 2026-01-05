use alloy_consensus::crypto::{secp256k1, RecoveryError};
use alloy_eips::Typed2718;
use alloy_primitives::{keccak256, Address, B256, ChainId, Signature};
use alloy_rlp::{BufMut, Decodable, Encodable};
use serde::{Deserialize, Serialize};

/// Sponsor transaction type byte (0x76).
pub const SPONSOR_TX_TYPE_ID: u8 = 0x76;

/// Magic byte for fee payer signature hashing.
pub const FEE_PAYER_SIGNATURE_MAGIC_BYTE: u8 = 0x78;

/// Sponsor transaction with fee payer commitment.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SponsorTransaction {
    /// EIP-155 replay protection.
    pub chain_id: ChainId,
    /// Token used to pay fees.
    pub fee_token: Address,
    /// Fee payer signature over the sponsorship payload.
    pub fee_payer_signature: Signature,
}

impl SponsorTransaction {
    /// Returns the transaction type byte.
    pub const fn tx_type() -> u8 {
        SPONSOR_TX_TYPE_ID
    }

    /// Hash signed by the fee payer to sponsor this transaction.
    pub fn fee_payer_signature_hash(&self) -> B256 {
        let payload_length = self.chain_id.length() + self.fee_token.length();
        let mut buf = Vec::with_capacity(1 + rlp_header(payload_length).length_with_payload());

        buf.put_u8(FEE_PAYER_SIGNATURE_MAGIC_BYTE);
        rlp_header(payload_length).encode(&mut buf);
        self.chain_id.encode(&mut buf);
        self.fee_token.encode(&mut buf);

        keccak256(&buf)
    }

    /// Recovers the fee payer address from the signature.
    pub fn recover_fee_payer(&self) -> Result<Address, RecoveryError> {
        secp256k1::recover_signer(&self.fee_payer_signature, self.fee_payer_signature_hash())
    }

    fn rlp_encoded_fields_length(&self) -> usize {
        self.chain_id.length()
            + self.fee_token.length()
            + {
                let payload_length =
                    self.fee_payer_signature.rlp_rs_len() + self.fee_payer_signature.v().length();
                rlp_header(payload_length).length_with_payload()
            }
    }

    fn rlp_encode_fields(&self, out: &mut dyn BufMut) {
        self.chain_id.encode(out);
        self.fee_token.encode(out);

        let payload_length =
            self.fee_payer_signature.rlp_rs_len() + self.fee_payer_signature.v().length();
        rlp_header(payload_length).encode(out);
        self.fee_payer_signature
            .write_rlp_vrs(out, self.fee_payer_signature.v());
    }
}

impl Typed2718 for SponsorTransaction {
    fn ty(&self) -> u8 {
        Self::tx_type()
    }
}

impl Encodable for SponsorTransaction {
    fn encode(&self, out: &mut dyn BufMut) {
        let payload_length = self.rlp_encoded_fields_length();
        rlp_header(payload_length).encode(out);
        self.rlp_encode_fields(out);
    }

    fn length(&self) -> usize {
        let payload_length = self.rlp_encoded_fields_length();
        rlp_header(payload_length).length_with_payload()
    }
}

impl Decodable for SponsorTransaction {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = alloy_rlp::Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let remaining = buf.len();
        if header.payload_length > remaining {
            return Err(alloy_rlp::Error::InputTooShort);
        }

        let chain_id = Decodable::decode(buf)?;
        let fee_token = Decodable::decode(buf)?;

        let signature_header = alloy_rlp::Header::decode(buf)?;
        if buf.len() < signature_header.payload_length {
            return Err(alloy_rlp::Error::InputTooShort);
        }
        if !signature_header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let fee_payer_signature = Signature::decode_rlp_vrs(buf, bool::decode)?;

        if buf.len() + header.payload_length != remaining {
            return Err(alloy_rlp::Error::UnexpectedLength);
        }

        Ok(Self {
            chain_id,
            fee_token,
            fee_payer_signature,
        })
    }
}

#[inline]
fn rlp_header(payload_length: usize) -> alloy_rlp::Header {
    alloy_rlp::Header {
        list: true,
        payload_length,
    }
}
