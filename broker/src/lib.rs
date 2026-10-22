macro_rules! import {
    ($($module:ident),* $(,)?) => {
        $(
            pub mod $module;
            pub use $module::*;
        )*
    };
}

import!(cli, context, error);

pub mod commands;
mod lockfile;
pub mod logging;
pub mod mcp;
mod protocol;
mod server;
mod upstream;
