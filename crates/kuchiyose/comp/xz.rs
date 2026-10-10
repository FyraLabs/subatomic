use tokio::io::AsyncWrite;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Cfg {
    pub level: i32,
    pub multi: u32,
}

impl crate::Sealed for Cfg {}

impl super::CompConfigure for Cfg {
    fn to_async_write<'a, W: AsyncWrite + 'a>(&'a self, inner: W) -> impl AsyncWrite {
        let level = if self.level == 0 {
            async_compression::core::Level::Default
        } else {
            async_compression::core::Level::Precise(self.level)
        };

        match std::num::NonZero::new(self.multi) {
            Some(threads) => {
                async_compression::tokio::write::XzEncoder::parallel(inner, level, threads)
            }
            None => async_compression::tokio::write::XzEncoder::with_quality(inner, level),
        }
    }

    fn ext(&self) -> &'static str {
        "xz"
    }
}
