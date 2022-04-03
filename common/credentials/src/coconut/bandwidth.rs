// Copyright 2021 - Nym Technologies SA <contact@nymtech.net>
// SPDX-License-Identifier: Apache-2.0

// for time being assume the bandwidth credential consists of public identity of the requester
// and private (though known... just go along with it) infinite bandwidth value
// right now this has no double-spending protection, spender binding, etc
// it's the simplest possible case

use coconut_interface::{
    hash_to_scalar, prepare_blind_sign, Attribute, BlindSignRequest, Credential, Parameters,
    PrivateAttribute, PublicAttribute, Signature, VerificationKey,
};
use crypto::asymmetric::{encryption, identity};
use network_defaults::BANDWIDTH_VALUE;

use cosmrs::tx::Hash;

use super::utils::prepare_credential_for_spending;
use crate::error::Error;

pub const PUBLIC_ATTRIBUTES: u32 = 2;
pub const PRIVATE_ATTRIBUTES: u32 = 2;
pub const TOTAL_ATTRIBUTES: u32 = PUBLIC_ATTRIBUTES + PRIVATE_ATTRIBUTES;

pub struct BandwidthVoucher {
    // a random secret value generated by the client used for double-spending detection
    serial_number: PrivateAttribute,
    // a random secret value generated by the client used to bind multiple credentials together
    binding_number: PrivateAttribute,
    // the value (e.g., bandwidth) encoded in this voucher
    voucher_value: PublicAttribute,
    // the plain text value (e.g., bandwidth) encoded in this voucher
    voucher_value_plain: String,
    // a field with public information, e.g., type of voucher, interval etc.
    voucher_info: PublicAttribute,
    // the plain text information
    voucher_info_plain: String,
    // the hash of the deposit transaction
    tx_hash: Hash,
    // base58 encoded private key ensuring the depositer requested these attributes
    signing_key: identity::PrivateKey,
    // base58 encoded private key ensuring only this client receives the signature share
    encryption_key: encryption::PrivateKey,
    pedersen_commitments_openings: Vec<Attribute>,
    blind_sign_request: BlindSignRequest,
    use_request: bool,
}

impl BandwidthVoucher {
    pub fn new_with_blind_sign_req(
        private_attributes: [PrivateAttribute; PRIVATE_ATTRIBUTES as usize],
        public_attributes_plain: [&str; PUBLIC_ATTRIBUTES as usize],
        tx_hash: Hash,
        signing_key: identity::PrivateKey,
        encryption_key: encryption::PrivateKey,
        pedersen_commitments_openings: Vec<Attribute>,
        blind_sign_request: BlindSignRequest,
    ) -> Self {
        let voucher_value = public_attributes_plain[0];
        let voucher_info = public_attributes_plain[1];
        let voucher_value_plain = voucher_value.to_string();
        let voucher_info_plain = voucher_info.to_string();
        let voucher_value = hash_to_scalar(voucher_value.as_bytes());
        let voucher_info = hash_to_scalar(voucher_info.as_bytes());

        BandwidthVoucher {
            serial_number: private_attributes[0],
            binding_number: private_attributes[1],
            voucher_value,
            voucher_value_plain,
            voucher_info,
            voucher_info_plain,
            tx_hash,
            signing_key,
            encryption_key,
            pedersen_commitments_openings,
            blind_sign_request,
            use_request: false,
        }
    }
    pub fn new(
        params: &Parameters,
        voucher_value: String,
        voucher_info: String,
        tx_hash: Hash,
        signing_key: identity::PrivateKey,
        encryption_key: encryption::PrivateKey,
    ) -> Self {
        let serial_number = params.random_scalar();
        let binding_number = params.random_scalar();
        let voucher_value_plain = voucher_value.clone();
        let voucher_info_plain = voucher_info.clone();
        let voucher_value = hash_to_scalar(voucher_value.as_bytes());
        let voucher_info = hash_to_scalar(voucher_info.as_bytes());
        let (pedersen_commitments_openings, blind_sign_request) = prepare_blind_sign(
            params,
            &[serial_number, binding_number],
            &[voucher_value, voucher_info],
        )
        .unwrap();
        BandwidthVoucher {
            serial_number,
            binding_number,
            voucher_value,
            voucher_value_plain,
            voucher_info,
            voucher_info_plain,
            tx_hash,
            signing_key,
            encryption_key,
            pedersen_commitments_openings,
            blind_sign_request,
            use_request: true,
        }
    }

    /// Check if the plain values correspond to the PublicAttributes
    pub fn verify_against_plain(values: &[PublicAttribute], plain_values: &[String]) -> bool {
        values.len() == 2
            && plain_values.len() == 2
            && values[0] == hash_to_scalar(&plain_values[0])
            && values[1] == hash_to_scalar(&plain_values[1])
    }

    pub fn tx_hash(&self) -> &Hash {
        &self.tx_hash
    }

    pub fn get_public_attributes(&self) -> Vec<PublicAttribute> {
        vec![self.voucher_value, self.voucher_info]
    }

    pub fn encryption_key(&self) -> &encryption::PrivateKey {
        &self.encryption_key
    }

    pub fn pedersen_commitments_openings(&self) -> &Vec<Attribute> {
        &self.pedersen_commitments_openings
    }

    pub fn blind_sign_request(&self) -> &BlindSignRequest {
        &self.blind_sign_request
    }

    pub fn use_request(&self) -> bool {
        self.use_request
    }

    pub fn get_public_attributes_plain(&self) -> Vec<String> {
        vec![
            self.voucher_value_plain.clone(),
            self.voucher_info_plain.clone(),
        ]
    }

    pub fn get_private_attributes(&self) -> Vec<PrivateAttribute> {
        vec![self.serial_number, self.binding_number]
    }

    pub fn sign(&self, request: &BlindSignRequest) -> identity::Signature {
        let mut message = request.to_bytes();
        message.extend_from_slice(self.tx_hash.as_bytes());
        self.signing_key.sign(&message)
    }
}

pub fn prepare_for_spending(
    raw_identity: &[u8],
    signature: &Signature,
    attributes: &BandwidthVoucher,
    verification_key: &VerificationKey,
) -> Result<Credential, Error> {
    let public_attributes = vec![
        raw_identity.to_vec(),
        BANDWIDTH_VALUE.to_be_bytes().to_vec(),
    ];

    let params = Parameters::new(TOTAL_ATTRIBUTES)?;

    prepare_credential_for_spending(
        &params,
        public_attributes,
        attributes.serial_number,
        attributes.binding_number,
        signature,
        verification_key,
    )
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn voucher_consistency() {
        let params = Parameters::new(4).unwrap();
        let mut rng = OsRng;
        let voucher = BandwidthVoucher::new(
            &params,
            "1234".to_string(),
            "voucher info".to_string(),
            Hash::new([0; 32]),
            identity::PrivateKey::from_base58_string(
                identity::KeyPair::new(&mut rng)
                    .private_key()
                    .to_base58_string(),
            )
            .unwrap(),
            encryption::KeyPair::new(&mut rng).private_key().clone(),
        );
        assert!(!BandwidthVoucher::verify_against_plain(
            &[],
            &voucher.get_public_attributes_plain()
        ));
        assert!(!BandwidthVoucher::verify_against_plain(
            &voucher.get_public_attributes(),
            &[],
        ));
        assert!(!BandwidthVoucher::verify_against_plain(
            &voucher.get_public_attributes(),
            &[
                voucher.get_public_attributes_plain()[0].clone(),
                String::new()
            ]
        ));
        assert!(!BandwidthVoucher::verify_against_plain(
            &voucher.get_public_attributes(),
            &[
                String::new(),
                voucher.get_public_attributes_plain()[1].clone()
            ]
        ));
        assert!(!BandwidthVoucher::verify_against_plain(
            &[voucher.get_public_attributes()[0], Attribute::one()],
            &voucher.get_public_attributes_plain()
        ));
        assert!(!BandwidthVoucher::verify_against_plain(
            &[Attribute::one(), voucher.get_public_attributes()[1]],
            &voucher.get_public_attributes_plain()
        ));
        assert!(BandwidthVoucher::verify_against_plain(
            &voucher.get_public_attributes(),
            &voucher.get_public_attributes_plain()
        ));
    }
}
