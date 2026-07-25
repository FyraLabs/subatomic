use rpm::signature::Signing;

#[derive(Debug)]
pub struct Mgr {
    private: pgp::composed::SignedSecretKey,
}

impl Mgr {
    pub fn public(&self) -> pgp::composed::SignedPublicKey {
        self.private.to_public_key()
    }

    /// Sign an rpm header. This is equivalent to [`rpm::Package::sign`], except that only the inner
    /// [`rpm::PackageMetadata`] is needed.
    ///
    /// # Errors
    /// Header parsing errors and signing errors are returned.
    pub fn sign_rpm(&self, rpm: &mut rpm::PackageMetadata) -> Result<(), rpm::Error> {
        // TODO: we should send the signature to the cli for gh attest.
        let signer = rpm::signature::pgp::Signer::new(self.private.primary_key.clone())?;
        let sig = signer.sign(rpm.header_bytes()?.as_slice(), rpm::Timestamp::now())?;
        rpm.signature = rpm::SignatureHeaderBuilder::from_existing(&rpm.signature)?
            .add_openpgp_signature(sig)
            .build()?;
        Ok(())
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
