use anyhow::{ensure, Result};
use facto_exporter::debug::elf::full_symbol_table;
use facto_exporter::debug::ptrace::wait_for_stop;
use nix::libc::pid_t;
use nix::sys::ptrace;
use nix::unistd::Pid;
use std::process::Command;

#[test]
fn smoke() -> Result<()> {
    let victim_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/victim1/victim1");

    let table = full_symbol_table(victim_path)?;
    println!("{:?}", table.get("main"));

    let mut child = Command::new(victim_path).spawn()?;
    let child_pid = Pid::from_raw(pid_t::try_from(child.id())?);
    let res = work(child_pid);
    let _ = child.kill();
    let _ = child.wait();
    res
}

fn work(pid: Pid) -> Result<()> {
    ptrace::attach(pid)?;
    ensure!(wait_for_stop(pid)?.is_some());

    todo!("doesn't actually test anything");

    Ok(())
}
