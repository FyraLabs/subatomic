pub use itertools::Itertools;
pub use rayon::prelude::*;
pub use serde::Serialize;
pub use smartstring::alias::String;

#[macro_export]
macro_rules! epoch {
    ($f:expr) => {{
        #[allow(clippy::unwrap_in_result)]
        $f.duration_since(::std::time::UNIX_EPOCH).expect("time overflow").as_secs()
    }};
}
pub use crate::epoch;
