use pgp::composed::Deserializable;
use rpm::signature::Signing;

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
    /// Create a new private key.
    ///
    /// # Panics
    /// We expect this function to always work. [`pgp`] did not exactly document when this will fail.
    ///
    /// # Examples
    /// ```
    /// const USERID: &str = "RPM Fission <nuclearfission-buildsys@example.com>";
    /// let mgr = libsubatomic::sig::Mgr::new(String::from(USERID));
    /// println!("{}", mgr.to_armor());
    /// ```
    #[must_use]
    pub fn new(userid: std::string::String) -> Self {
        // const KEYTYPE: pgp::composed::KeyType = pgp::composed::KeyType::Rsa(4096);
        const KEYTYPE: pgp::composed::KeyType = pgp::composed::KeyType::Ed25519;
        let mut signkey = pgp::composed::SubkeyParamsBuilder::default();
        signkey
            .key_type(KEYTYPE)
            .can_sign(true)
            .can_encrypt(pgp::composed::EncryptionCaps::None)
            .can_authenticate(false);
        Self {
            private: pgp::composed::SecretKeyParamsBuilder::default()
                .key_type(KEYTYPE)
                .can_certify(true)
                .can_sign(false)
                .can_encrypt(pgp::composed::EncryptionCaps::None)
                .primary_user_id(userid)
                .subkey(signkey.build().expect("can't build signkey"))
                .build()
                .expect("cannot build prikey params")
                .generate(rand::thread_rng())
                .expect("cannot generate prikey"),
        }
    }

    pub fn from_armor(armor: &str) -> pgp::errors::Result<Self> {
        let r = std::io::BufReader::new(armor.as_bytes());
        Ok(Self { private: pgp::composed::SignedSecretKey::from_armor_single(r)?.0 })
    }

    pub fn to_armor(&self) -> String {
        self.private.to_armored_string(Default::default()).expect("cannot convert to bytes")
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
    pub fn sign_rpm(&self, rpm: &mut rpm::PackageMetadata) -> Result<Vec<u8>, rpm::Error> {
        // TODO: we should send the signature to the cli for gh attest.
        let signer = rpm::signature::pgp::Signer::new(self.private.primary_key.clone())?;
        let sig = signer.sign(rpm.header_bytes()?.as_slice(), rpm::Timestamp::now())?;
        rpm.signature = rpm::SignatureHeaderBuilder::from_existing(&rpm.signature)?
            .add_openpgp_signature(sig.clone())
            .build()?;
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

#[cfg(test)]
mod test {
    #[test]
    fn generate() {
        const USERID: &str = "RPM Fission <nuclearfission-buildsys@example.com>";
        let mgr = super::Mgr::new(String::from(USERID));
        println!("{}", mgr.to_armor());
    }
}
