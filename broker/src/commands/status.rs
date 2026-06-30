use crate::{Error, commands::CommandResult, lockfile::Lockfile};

pub fn status() -> CommandResult {
    match Lockfile::read() {
        Some(lock) => {
            println!(
                "broker running on ws://127.0.0.1:{}/ (pid {})",
                lock.handshake.port, lock.pid
            );
            println!("lockfile: {}", Lockfile::path().display());
            Ok(())
        }
        None => {
            println!("no broker lockfile found");
            Err(Error::LockfileNotFound)
        }
    }
}
