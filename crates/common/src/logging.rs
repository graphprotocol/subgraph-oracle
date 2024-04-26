use crate::prelude::*;
use slog::{o, Drain};
use slog_async::Async;
use slog_term::{FullFormat, TermDecorator};

pub fn create_logger() -> Logger {
    let decorator = TermDecorator::new().build();
    let drain = FullFormat::new(decorator).build().fuse();
    let drain = Async::new(drain).build().fuse();

    slog::Logger::root(drain, o!())
}

#[macro_export]
macro_rules! impl_slog_value {
    ($T:ty) => {
        impl_slog_value!($T, "{}");
    };
    ($T:ty, $fmt:expr) => {
        impl ::slog::Value for $T {
            fn serialize(
                &self,
                record: &::slog::Record,
                key: ::slog::Key,
                serializer: &mut dyn ::slog::Serializer,
            ) -> ::slog::Result {
                format!($fmt, self).serialize(record, key, serializer)
            }
        }
    };
}
