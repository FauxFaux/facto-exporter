use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use facto_exporter::debug::elf::{find_function, full_symbol_table, Symbol};
use facto_exporter::debug::inject::inject_mmap;
use facto_exporter::debug::pad_to_word;
use facto_exporter::debug::ptrace::{
    breakpoint, find_executable_map, read_words_arr, read_words_var, run_until_stop, wait_for_stop,
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
    let fake_structs_addr = inject_mmap(pid, from)?;

    let call_end = pad_to_word(include_bytes!("../shellcode/call-end.bin"), 0xcc);
    assert_eq!(call_end.len(), 1);

    let mut mem = Vec::with_capacity(64);
    // 0-8: jump to code
    mem.extend_from_slice(&call_end);
    // 8-whatever: code
    mem.extend(pad_to_word(
        include_bytes!("../shellcode/crafting2.bin"),
        0xcc,
    ));

    let shellcode_fits_in = 1024;
    // padding
    assert!(mem.len() < shellcode_fits_in / 8);
    mem.resize(shellcode_fits_in / 8, 0xcc);
    let mem_addr = map_addr + u64::try_from(shellcode_fits_in).expect("sub-128bit machine please");

    // now, crafting2.c's interface struct, "Mem":
    // 0-8: pointer to the set, set by code, TODO
    mem.push(fake_structs_addr);
    // 8-16: pointer to the get, TODO
    mem.push(0x6666666666666666);
    // 16-24: capacity, based on the size of the mmap from stage1
    mem.push(60 * 1024 * 1024 / 8);
    // 24-32: size, set by code
    mem.push(0);
    // 32+: data

    write_words_ptr(pid, map_addr, &mem)?;

    println!("shell written, resuming...");
    run_until_stop(pid)?;
    assert_eq!([false, false, true, false], which_breakpoints(pid)?);

    println!("jumping to shell...");
    let mut regs = ptrace::getregs(pid)?;
    regs.rip = map_addr;
    regs.rdi = mem_addr;
    ptrace::setregs(pid, regs)?;
    let [word] = read_words_arr(pid, regs.rip)?;
    println!(
        "{:#x} (start + 8 + {:#x}): {:16x}",
        regs.rip,
        regs.rip as i64 - map_addr as i64 - 8,
        word.swap_bytes()
    );

    loop {
        ptrace::step(pid, None)?;
        wait_for_stop(pid)?;
        let regs = ptrace::getregs(pid)?;
        let [word] = read_words_arr(pid, regs.rip)?;
        println!(
            "{:#x} (start + 8 + {:#x}): {:16x}",
            regs.rip,
            regs.rip as i64 - map_addr as i64 - 8,
            word.swap_bytes()
        );
    }

    println!("boing...");
    run_until_stop(pid)?;

    Ok(())
}
