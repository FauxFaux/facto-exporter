use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use facto_exporter::debug::elf::{find_function, full_symbol_table, Symbol};
use facto_exporter::debug::inject::inject_mmap;
use facto_exporter::debug::pad_to_word;
use facto_exporter::debug::ptrace::{
    breakpoint, find_executable_map, read_words_var, run_until_stop, wait_for_stop,
    which_breakpoints, write_words_ptr,
};
use nix::libc::pid_t;
use nix::sys::ptrace;
use nix::unistd::Pid;

#[test]
fn smoke() -> Result<()> {
    let victim_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/victim1/victim1");

    let table = full_symbol_table(victim_path)?;
    let mut child = Command::new(victim_path).spawn()?;
    let child_pid = Pid::from_raw(pid_t::try_from(child.id())?);
    // this was some attempt to make sure the application is 'running', but the current test uses
    // breakpoints to make sure it is only stopped in regular code anyway
    thread::sleep(Duration::from_millis(30));
    let res = work(child_pid, &table);
    let _ = child.kill();
    let _ = child.wait();
    res
}

fn work(pid: Pid, table: &HashMap<String, Symbol>) -> Result<()> {
    let (step_named, step, _) = find_function(table, "step")?;
    println!("step found as (mangled): {step_named} at {step:#x}");

    ptrace::attach(pid)?;
    wait_for_stop(pid)?;

    let (from, to, offset) = find_executable_map(pid)?;
    assert_eq!(from % 8, 0);
    // should be huge, ensure that it's not tiny so we blow past the end
    assert!(to - from >= 0x1000);

    let step = from + step - offset;

    breakpoint(pid, [None, None, Some(step), None])?;
    run_until_stop(pid)?;
    assert_eq!([false, false, true, false], which_breakpoints(pid)?);

    let map_addr = inject_mmap(pid, from)?;

    write_words_ptr(
        pid,
        map_addr,
        &pad_to_word(include_bytes!("../shellcode/crafting.bin"), 0xcc),
    )?;

    ptrace::cont(pid, None)?;

    Ok(())
}
