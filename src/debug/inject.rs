use super::pad_to_word;
use super::ptrace::{read_words_var, run_until_stop, write_words_ptr};
use anyhow::{anyhow, ensure, Result};
use nix::sys::ptrace;
use nix::unistd::Pid;

/// precondition: thread is stopped at a reasonable place, e.g. a breakpoint outside of a lock or syscall
pub fn inject_mmap(pid: Pid, scratch: u64) -> Result<u64> {
    let stage1 = pad_to_word(include_bytes!("../../shellcode/stage1.bin"), 0xcc);

    let backup = read_words_var(pid, scratch, stage1.len())?;
    write_words_ptr(pid, scratch, &stage1)?;

    let orig_regs = ptrace::getregs(pid)?;
    let mut regs = orig_regs.clone();
    regs.rip = scratch;
    ptrace::setregs(pid, regs)?;

    run_until_stop(pid)?;

    regs = ptrace::getregs(pid)?;
    // let executed = regs.rip as i64 - from as i64;
    // executed == stage1_bytes.len()

    let map_addr = regs.rax;
    ensure!(map_addr != u64::MAX, "mmap failed with -1");
    // println!("{}", fs::read_to_string(format!("/proc/{}/maps", pid))?);

    write_words_ptr(pid, scratch, &backup)?;
    ptrace::setregs(pid, orig_regs)?;

    Ok(map_addr)
}

pub fn entry_in_addr(addr_file: &str) -> Result<u64> {
    let line = addr_file
        .lines()
        .find(|line| line.ends_with("entry"))
        .ok_or_else(|| anyhow!("entry not found"))?;
    let mut it = line.split_whitespace();
    let offset = it.next().ok_or_else(|| anyhow!("no offset"))?;
    Ok(u64::from_str_radix(offset, 16)?)
}

/// (mem, mock_get_status_offset in words)
pub fn shell_code() -> (Vec<u64>, u64) {
    // if this isn't right, call-end.bin needs to learn to jump further forward
    assert_eq!(
        0,
        entry_in_addr(include_str!("../../shellcode/crafting2.bin.addr"))
            .expect("parsing static asset")
    );

    // if this isn't right, mock_get_status_addr needs handling (but it won't change)
    assert_eq!(
        0,
        entry_in_addr(include_str!("../../shellcode/mock-get-status.bin.addr"))
            .expect("parsing static asset")
    );

    let call_end = pad_to_word(include_bytes!("../../shellcode/call-end.bin"), 0xcc);
    assert_eq!(call_end.len(), 1);

    let mock_get_status = pad_to_word(include_bytes!("../../shellcode/mock-get-status.bin"), 0xcc);
    assert_eq!(mock_get_status.len(), 1);

    let main_code = pad_to_word(include_bytes!("../../shellcode/crafting2.bin"), 0xcc);

    let mut mem = Vec::with_capacity(64);
    // 0-8: jump to code
    mem.extend_from_slice(&call_end);

    // 8-whatever: code
    mem.extend_from_slice(&main_code);

    let mock_get_status_off = u64::try_from(mem.len()).expect("40 < 2^64");
    mem.extend_from_slice(&mock_get_status);

    (mem, mock_get_status_off)
}
