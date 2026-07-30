use pgp::{
    composed::Deserializable,
    crypto::{hash::HashAlgorithm, sym::SymmetricKeyAlgorithm},
    ser::Serialize,
    types::CompressionAlgorithm,
};
use rpm::signature::Signing;
use smallvec::smallvec;

#[derive(Clone)]
pub struct Mgr {
    private: pgp::composed::SignedSecretKey,
}

impl std::fmt::Debug for Mgr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mgr").field("private", &"<private key redacted>").finish()
    }
}

impl Mgr {
    pub fn new(userid: std::string::String) -> Self {
        Self {
            private: pgp::composed::SecretKeyParamsBuilder::default()
                .key_type(pgp::composed::KeyType::Rsa(4096))
                .can_certify(false)
                .can_sign(true)
                .primary_user_id(userid)
                .preferred_symmetric_algorithms(smallvec![SymmetricKeyAlgorithm::AES256])
                .preferred_hash_algorithms(smallvec![HashAlgorithm::Sha256])
                .preferred_compression_algorithms(smallvec![CompressionAlgorithm::ZLIB])
                .build()
                .expect("cannot build prikey params")
                .generate(rand::thread_rng())
                .expect("cannot generate prikey"),
        }
    }

    pub fn parse(bytes: &[u8]) -> pgp::errors::Result<Self> {
        Ok(Self {
            private: pgp::composed::SignedSecretKey::from_bytes(std::io::BufReader::new(bytes))?,
        })
    }

    pub fn write(&self) -> Vec<u8> {
        self.private.to_bytes().expect("cannot convert to bytes")
    }

    pub fn public(&self) -> pgp::composed::SignedPublicKey {
        self.private.to_public_key()
    }

    pub fn public_armor(&self) -> pgp::errors::Result<std::string::String> {
        self.public().to_armored_string(pgp::composed::ArmorOptions::default())
    }

    /// Sign an rpm header. This is equivalent to [`rpm::Package::sign`], except that only the inner
    /// [`rpm::PackageMetadata`] is needed.
    ///
    /// # Errors
    /// Header parsing errors and signing errors are returned.
    pub fn sign_rpm(&self, rpm: &rpm::PackageMetadata) -> Result<Vec<u8>, rpm::Error> {
        // TODO: we should send the signature to the cli for gh attest.
        let signer = rpm::signature::pgp::Signer::new(self.private.primary_key.clone())?;
        let sig = signer.sign(rpm.header_bytes()?.as_slice(), rpm::Timestamp::now())?;
        // rpm.signature = rpm::SignatureHeaderBuilder::from_existing(&rpm.signature)?
        //     .add_openpgp_signature(sig)
        //     .build()?;
        Ok(sig)
    }

    /// Sign some data.
    ///
    /// # Errors
    /// Propagated from [`pgp::composed::DetachedSignature::sign_text_data`].
    pub fn sign(
        &self,
        data: &[u8],
    ) -> Result<pgp::composed::DetachedSignature, pgp::errors::Error> {
        pgp::composed::DetachedSignature::sign_text_data(
            rand::thread_rng(),
            &self.private.primary_key,
            &pgp::types::Password::empty(),
            pgp::crypto::hash::HashAlgorithm::Sha256,
            data,
        )
    }
}
