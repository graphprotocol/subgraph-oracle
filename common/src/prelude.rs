#[macro_export]
macro_rules! concat_bytes {
    ($count:expr, [$($slices:expr,)*]) => {
        {
            let mut bytes = [0u8; $count];
            let mut cursor = ::std::io::Cursor::new(&mut bytes[..]);

            $(
                ::std::io::Write::write(&mut cursor, $slices).unwrap();
            )*

            assert!(cursor.position() == $count);

            bytes
        }
    }
}

pub use crate::concat_bytes;

pub use crate::metrics::{Metrics, WithErrMetric as _};
pub use anyhow::{anyhow, ensure, Context as _, Error, Result};
pub use futures::{
    self, Future, FutureExt as _, StreamExt as _, TryFutureExt as _, TryStreamExt as _,
};
pub use never::Never;
pub use prometheus::Counter;
pub use slog::{error, info, trace, warn, Logger};
pub use std::convert::{TryFrom, TryInto};
pub type Bytes32 = [u8; 32];
pub use lazy_static::lazy_static;

pub use crate::impl_slog_value;
pub use async_trait::async_trait;

pub fn blake3(salt: &[u8], data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(salt).update(data);
    hasher.finalize().into()
}

#[macro_export]
macro_rules! _bytes_wrapper {
    ($vis:vis $id:ident, $len:expr) => {
        #[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        $vis struct $id(pub [u8; $len]);

        impl ::std::ops::Deref for $id {
            type Target = [u8; $len];
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl ::std::fmt::Debug for $id {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "0x{}", hex::encode(self.0))
            }
        }
    };
}

pub use crate::_bytes_wrapper as bytes_wrapper;
