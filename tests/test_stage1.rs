use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use std::{fs, thread};

use anyhow::{anyhow, bail, ensure, Result};
use facto_exporter::debug::elf::{full_symbol_table, Symbol};
use facto_exporter::debug::pad_to_word;
use facto_exporter::debug::ptrace::{
    read_words_var, run_until_stop, wait_for_stop, write_words_ptr,
};
use nix::libc::pid_t;
use nix::sys::ptrace;
use nix::unistd::Pid;

#[test]
fn smoke() -> Result<()> {
    let victim_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/victim1/victim1");

    let mut child = Command::new(victim_path).spawn()?;
    let child_pid = Pid::from_raw(pid_t::try_from(child.id())?);
    thread::sleep(Duration::from_millis(30));
    let res = work(child_pid);
    let _ = child.kill();
    let _ = child.wait();
    res
}

fn find_executable_map(pid: Pid) -> Result<(u64, u64, u64)> {
    let maps = std::fs::read_to_string(format!("/proc/{}/maps", pid))?;
    for line in maps.lines() {
        let mut it = line.split_whitespace();

        let addrs = it.next().ok_or_else(|| anyhow!("no addrs"))?;
        let perms = it.next().ok_or_else(|| anyhow!("no perms"))?;
        if !perms.contains('x') {
            continue;
        }
        let offset = it.next().ok_or_else(|| anyhow!("no offset"))?;
        let _dev = it.next().ok_or_else(|| anyhow!("no dev"))?;
        let _inode = it.next().ok_or_else(|| anyhow!("no inode"))?;
        // path is inaccurate due to split_whitespace
        let path_first = it.next().ok_or_else(|| anyhow!("no path"))?;

        let (from, to) = addrs.split_once('-').ok_or_else(|| anyhow!("no -"))?;
        let from = u64::from_str_radix(from, 16)?;
        let to = u64::from_str_radix(to, 16)?;
        let offset = u64::from_str_radix(offset, 16)?;
        return Ok((from, to, offset));
    }

    bail!("no executable map found");
}

fn work(pid: Pid) -> Result<()> {
    ptrace::attach(pid)?;
    wait_for_stop(pid)?;

    let (from, to, offset) = find_executable_map(pid)?;
    assert_eq!(from % 8, 0);

    let stage1 = pad_to_word(include_bytes!("../shellcode/stage1.bin"), 0xcc);

    let backup = read_words_var(pid, from, stage1.len())?;
    write_words_ptr(pid, from, &stage1)?;

    let orig_regs = ptrace::getregs(pid)?;
    let mut regs = orig_regs.clone();
    // 4: something(tm) is angry about starting at the start (maybe it's to do with decoding instructions?),
    // so just jump into the middle of the nop slide. 4 is arbitrary (>1, <11)
    regs.rip = from + 4;
    ptrace::setregs(pid, regs)?;

    run_until_stop(pid)?;

    regs = ptrace::getregs(pid)?;
    // let executed = regs.rip as i64 - from as i64;
    // executed == stage1_bytes.len()

    let map_addr = regs.rax;
    // println!("{}", fs::read_to_string(format!("/proc/{}/maps", pid))?);

    write_words_ptr(pid, from, &backup)?;
    ptrace::setregs(pid, orig_regs)?;

    write_words_ptr(
        pid,
        map_addr,
        &pad_to_word(include_bytes!("../shellcode/crafting.bin"), 0xcc),
    )?;

    ptrace::cont(pid, None)?;

    Ok(())
}
