use tokio::io::AsyncWrite;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Cfg {
    pub level: i32,
    pub multi: u32,
}

impl super::CompConfigure for Cfg {
    fn to_async_write<'a, W: AsyncWrite + 'a>(&'a self, inner: W) -> impl AsyncWrite {
        let level = if self.level == 0 {
            async_compression::core::Level::Default
        } else {
            async_compression::core::Level::Precise(self.level)
        };
        let params = [async_compression::zstd::CParameter::nb_workers(self.multi)];
        async_compression::tokio::write::ZstdEncoder::with_quality_and_params(inner, level, &params)
    }
}
