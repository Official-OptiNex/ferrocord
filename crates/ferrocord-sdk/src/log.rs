//! Logging macros that send lines to the host: [`error!`], [`warn!`],
//! [`info!`], [`debug!`] and [`trace!`].

/// Logs a formatted message at the given [`crate::LogLevel`].
#[macro_export]
macro_rules! log_at {
    ($level:expr, $($arg:tt)*) => {
        $crate::raw::log($level as i32, &::std::format!($($arg)*))
    };
}

#[macro_export]
macro_rules! error { ($($arg:tt)*) => { $crate::log_at!($crate::LogLevel::Error, $($arg)*) }; }
#[macro_export]
macro_rules! warn { ($($arg:tt)*) => { $crate::log_at!($crate::LogLevel::Warn, $($arg)*) }; }
#[macro_export]
macro_rules! info { ($($arg:tt)*) => { $crate::log_at!($crate::LogLevel::Info, $($arg)*) }; }
#[macro_export]
macro_rules! debug { ($($arg:tt)*) => { $crate::log_at!($crate::LogLevel::Debug, $($arg)*) }; }
#[macro_export]
macro_rules! trace { ($($arg:tt)*) => { $crate::log_at!($crate::LogLevel::Trace, $($arg)*) }; }

// Re-export so `log::info!(...)` reads naturally after `use ferrocord_sdk::log;`.
#[doc(inline)]
pub use crate::{debug, error, info, trace, warn};
