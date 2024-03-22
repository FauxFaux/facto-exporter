use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use std::{fs, thread};

use anyhow::{anyhow, bail, ensure, Result};
use facto_exporter::debug::elf::{full_symbol_table, Symbol};
use facto_exporter::debug::pad_to_word;
use facto_exporter::debug::ptrace::{
    cont_until_stop, read_words_var, wait_for_stop, write_words_ptr,
};
use nix::libc::{pid_t, ptrace};
use nix::sys::ptrace;
use nix::unistd::Pid;

#[test]
fn smoke() -> Result<()> {
    let victim_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/victim1/victim1");

    let table = full_symbol_table(victim_path)?;
    println!("{:?}", table.get("main"));

    let mut child = Command::new(victim_path).spawn()?;
    let child_pid = Pid::from_raw(pid_t::try_from(child.id())?);
    thread::sleep(Duration::from_millis(30));
    let res = work(child_pid, &table);
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

fn work(pid: Pid, symbols: &HashMap<String, Symbol>) -> Result<()> {
    ptrace::attach(pid)?;
    ensure!(wait_for_stop(pid)?.is_some());

    // let (main, main_len) = *symbols
    //     .get("main")
    //     .ok_or_else(|| anyhow!("no main symbol"))?;

    let (from, to, offset) = find_executable_map(pid)?;

    let stage1 = pad_to_word(include_bytes!("../shellcode/stage1.bin"), 0xcc);
    // assert!(
    //     main_len>= stage1.len() * 8,
    //     "stage1 too big to fit in main, {} > {}",
    //     stage1.len(),
    //     main_len
    // );

    let backup = read_words_var(pid, from, stage1.len())?;
    write_words_ptr(pid, from, &stage1)?;

    let orig_regs = ptrace::getregs(pid)?;
    let mut regs = orig_regs.clone();
    regs.rip = from;
    ptrace::setregs(pid, regs)?;

    cont_until_stop(pid)?;

    {
        // let meaningless_stop_regs = ptrace::getregs(pid)?;

        // I have no idea why this is necessary, implying the thing stopped before we started running again
        // It stops at the instruction we just wrote (base + 0).
        // println!("meaningless(?) stop at {:x} (base + {})", meaningless_stop_regs.rip, meaningless_stop_regs.rip as i64 - from as i64);

        todo!("okay, we're ignoring segfaults, which is why this continues executing for a bit");

        cont_until_stop(pid)?;
    }

    regs = ptrace::getregs(pid)?;
    let executed = regs.rip as i64 - from as i64;
    // executed == stage1_bytes.len() + 1

    println!("stopped at {:x} after {} bytes", regs.rip, executed);
    // let mapped = regs.rax;
    println!("map: {:x}", regs.rax);
    println!("{}", fs::read_to_string(format!("/proc/{}/maps", pid))?);
    panic!("from: {from}, mapped: {:?}", regs);
    ptrace::setregs(pid, orig_regs)?;
    ptrace::cont(pid, None)?;

    Ok(())
}
