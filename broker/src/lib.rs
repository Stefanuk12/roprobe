macro_rules! import {
    ($($module:ident),* $(,)?) => {
        $(
            pub mod $module;
            pub use $module::*;
        )*
    };
}

import!(cli, error);

pub mod commands;
mod lockfile;
pub mod logging;
mod protocol;
mod server;
