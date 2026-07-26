pub use itertools::Itertools;
pub use rayon::prelude::*;
pub use serde::{Deserialize, Serialize};
pub use smartstring::alias::String;
pub use std::ffi::{OsStr, OsString};
pub use std::io::prelude::*;
pub use std::os::unix::prelude::*;
pub use std::path::{Path, PathBuf};

#[macro_export]
macro_rules! epoch {
    ($f:expr) => {{
        #[allow(clippy::unwrap_in_result)]
        $f.duration_since(::std::time::UNIX_EPOCH).expect("time overflow").as_secs()
    }};
}
pub use crate::epoch;
pub use crate::err::Res;
