use std::panic::Location;

#[macro_export]
macro_rules! check {
    ( $condition:expr, $error:expr ) => {
        if !$condition {
            return Err($error);
        }
    };
}

#[macro_export]
macro_rules! debug_panic {
    ( $($fmt_arg:tt)* ) => {
        if cfg!(debug_assertions) {
            panic!( $($fmt_arg)* );
        } else {
            let backtrace = std::backtrace::Backtrace::capture();
            log::error!("{}\n{:?}", format_args!($($fmt_arg)*), backtrace);
        }
    };
}

pub trait ResultExt<E> {
    type Ok;

    fn log_err(self) -> Option<Self::Ok>;

    /// Assert that this result should never be an error in development or tests.
    #[allow(dead_code)]
    fn debug_assert_ok(self, reason: &str) -> Self;

    #[allow(dead_code)]
    fn warn_on_err(self) -> Option<Self::Ok>;

    fn log_with_level(self, level: log::Level) -> Option<Self::Ok>;
    // fn anyhow(self) -> anyhow::Result<Self::Ok>
    // where
    //     E: Into<anyhow::Error>;
}

impl<T, E> ResultExt<E> for Result<T, E>
where
    E: std::fmt::Debug,
{
    type Ok = T;

    #[track_caller]
    fn log_err(self) -> Option<T> {
        self.log_with_level(log::Level::Error)
    }

    #[track_caller]
    fn debug_assert_ok(self, reason: &str) -> Self {
        if let Err(error) = &self {
            debug_panic!("{reason} - {error:?}");
        }
        self
    }

    #[track_caller]
    fn warn_on_err(self) -> Option<T> {
        self.log_with_level(log::Level::Warn)
    }

    #[track_caller]
    fn log_with_level(self, level: log::Level) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(error) => {
                log_error_with_caller(*Location::caller(), error, level);
                None
            }
        }
    }

    // fn anyhow(self) -> anyhow::Result<T>
    // where
    //     E: Into<anyhow::Error>,
    // {
    //     self.map_err(Into::into)
    // }
}

fn log_error_with_caller<E>(caller: core::panic::Location<'_>, error: E, level: log::Level)
where
    E: std::fmt::Debug,
{
    #[cfg(not(target_os = "windows"))]
    let file = caller.file();
    #[cfg(target_os = "windows")]
    let file = caller.file().replace('\\', "/");
    // In this codebase, the first segment of the file path is
    // the 'crates' folder, followed by the crate name.
    let target = file.split('/').nth(1);

    log::logger().log(
        &log::Record::builder()
            .target(target.unwrap_or(""))
            .module_path(target)
            .args(format_args!("{:?}", error))
            .file(Some(caller.file()))
            .line(Some(caller.line()))
            .level(level)
            .build(),
    );
}

// pub fn log_err<E: std::fmt::Debug>(error: &E) {
//     log_error_with_caller(*Location::caller(), error, log::Level::Warn);
// }
