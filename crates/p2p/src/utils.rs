use std::{
    fmt::{Debug, Display},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now_ms() -> u64 {
    // Get the current time
    let now = SystemTime::now();

    // Calculate the duration since the UNIX epoch
    let duration = now.duration_since(UNIX_EPOCH).expect("Time went backwards");

    // Return the total milliseconds
    duration.as_millis() as u64
}

pub trait ErrorExt {
    fn print_on_err(&self, prefix: &str);
}

pub trait ErrorExt2 {
    fn print_on_err2(&self, prefix: &str);
}

impl<T, E: Display> ErrorExt for Result<T, E> {
    fn print_on_err(&self, prefix: &str) {
        if let Err(e) = self {
            log::error!("{prefix} got error {e}")
        }
    }
}

impl<T, E: Debug> ErrorExt2 for Result<T, E> {
    fn print_on_err2(&self, prefix: &str) {
        if let Err(e) = self {
            log::error!("{prefix} got error {e:?}")
        }
    }
}

// Define the macro
#[macro_export]
macro_rules! return_on_err {
    ($result:expr, $prefix:expr) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                log::error!("{}: {:?}", $prefix, e);
                return; // Adjust return type as needed (e.g., return None; for Option)
            }
        }
    };
}
