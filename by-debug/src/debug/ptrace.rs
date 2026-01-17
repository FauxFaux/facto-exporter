use std::ffi::c_void;
use std::io::IoSliceMut;

use anyhow::{anyhow, bail, Result};
use nix::libc::c_long;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::uio::{process_vm_readv, RemoteIoVec};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;

const DR0: *mut c_void = 848 as *mut c_void;
const DR1: *mut c_void = 856 as *mut c_void;
const DR2: *mut c_void = 864 as *mut c_void;
const DR3: *mut c_void = 872 as *mut c_void;
const DR6: *mut c_void = 896 as *mut c_void;
const DR7: *mut c_void = 904 as *mut c_void;

#[inline]
pub fn read_words_arr<const N: usize>(pid: Pid, addr: u64) -> Result<[u64; N]> {
    let mut ret = [0u64; N];
    assert!(
        addr.checked_add(u64::try_from(N * 8).expect("usize <= 2^64"))
            .is_some(),
        "{addr} + {N} words overflows u64"
    );
    for i in 0..N {
        // assert! above validates cast
        let start = addr + (i * 8) as u64;
        let word = ptrace::read(pid, start as *mut _)?;
        ret[i] = word as u64;
    }
    Ok(ret)
}

#[inline]
pub fn read_words_var(pid: Pid, addr: u64, words: usize) -> Result<Vec<u64>> {
    let mut ret = vec![0u64; words];
    assert!(
        addr.checked_add(u64::try_from(words * 8).expect("usize <= 2^64"))
            .is_some(),
        "{addr} + {words} words overflows u64"
    );
    for i in 0..words {
        // assert! above validates cast
        let start = addr + (i * 8) as u64;
        println!("reading {start:x}");
        let word = ptrace::read(pid, start as *mut _)?;
        ret[i] = word as u64;
    }
    Ok(ret)
}

pub fn write_words_ptr(pid: Pid, addr: u64, data: &[u64]) -> Result<()> {
    let mut addr = addr;
    let ptr_size = std::mem::size_of::<*mut c_void>();
    assert_eq!(ptr_size, 8, "64-bit machines only");

    for word in data {
        unsafe {
            ptrace::write(pid, addr as *mut _, *word as *mut _)?;
        }
        addr += 8;
    }
    Ok(())
}

pub fn dump(mem: &[u8], addr: u64) -> Result<()> {
    for (off, block) in mem.chunks(8).enumerate() {
        let off = 8 * u64::try_from(off)?;
        print!("{:016x} <+{off:3x}> ", addr + off);
        for byte in block {
            print!("{:02x} ", byte);
        }
        println!();
    }
    Ok(())
}

pub fn breakpoint(pid: Pid, addrs: [Option<u64>; 4]) -> Result<()> {
    let mut dr7 = ptrace::read_user(pid, DR7)?;

    for (i, addr) in addrs.iter().enumerate() {
        let addr = match addr {
            Some(addr) => addr,
            None => continue,
        };
        let dr = match i {
            0 => DR0,
            1 => DR1,
            2 => DR2,
            3 => DR3,
            _ => unreachable!(),
        };
        unsafe {
            ptrace::write_user(pid, dr, *addr as *mut c_void)?;
        }
        set_bit(&mut dr7, i as u8 * 2, true);
    }

    unsafe {
        ptrace::write_user(pid, DR7, dr7 as *mut c_void)?;
    }

    Ok(())
}

pub fn which_breakpoints(pid: Pid) -> Result<[bool; 4]> {
    let dr6 = ptrace::read_user(pid, DR6)?;
    Ok([
        get_bit(dr6, 0),
        get_bit(dr6, 1),
        get_bit(dr6, 2),
        get_bit(dr6, 3),
    ])
}

pub fn bulk_read(pid: Pid, base: usize, len: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0; len];
    let chunks_read = process_vm_readv(
        pid,
        &mut [IoSliceMut::new(&mut buf)],
        &[RemoteIoVec { base, len }],
    )?;

    assert_eq!(
        chunks_read, 1,
        "readv only asked for one chunk, and it succeeded, so must have read one chunk"
    );

    Ok(buf)
}

pub fn run_until_stop(pid: Pid) -> Result<()> {
    ptrace::cont(pid, None)?;
    wait_for_stop(pid)?;
    Ok(())
}

/// issue a cont(), then wait for a stop; if we didn't get a stop, issue another cont() and try again repeatedly
///
/// Stops cleanly on SIGSTOP (ptrace) and SIGTRAP (int3).
pub fn wait_for_stop(pid: Pid) -> Result<()> {
    loop {
        let status = waitpid(pid, Some(WaitPidFlag::WSTOPPED))?;
        println!("waitpid: {:?}", status);

        let signal = match status {
            WaitStatus::Stopped(stopped_pid, signal) => {
                assert_eq!(stopped_pid, pid, "waitpid should only return our pid");
                if signal == Signal::SIGSTOP || signal == Signal::SIGTRAP {
                    return Ok(());
                }
                signal
            }
            _ => continue,
        };

        if let Ok(regs) = ptrace::getregs(pid) {
            println!("passing on signal at {:x}", regs.rip);
        }

        ptrace::cont(pid, signal)?;
    }
}

/// (from, to, offset)
///
/// `from` and `to` are the real start and end addresses in virtual memory
/// `offset` is the offset used in the symbol table, something something the layout in the file
pub fn find_executable_map(pid: Pid) -> Result<(u64, u64, u64)> {
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
        let _path_first = it.next().ok_or_else(|| anyhow!("no path"))?;

        let (from, to) = addrs.split_once('-').ok_or_else(|| anyhow!("no -"))?;
        let from = u64::from_str_radix(from, 16)?;
        let to = u64::from_str_radix(to, 16)?;
        let offset = u64::from_str_radix(offset, 16)?;
        return Ok((from, to, offset));
    }

    bail!("no executable map found");
}

pub fn debug_to_int3(pid: Pid, base_addr: u64) -> Result<()> {
    loop {
        ptrace::step(pid, None)?;
        wait_for_stop(pid)?;
        let regs = ptrace::getregs(pid)?;
        let [word] = read_words_arr(pid, regs.rip)?;
        println!(
            "{:#x} (start + {:#x}): {:016x}",
            regs.rip,
            regs.rip as i64 - base_addr as i64,
            word.swap_bytes()
        );

        // trap was from an int3 (0xcc)
        if word.to_le_bytes()[0] == 0xcc {
            break;
        }
    }

    Ok(())
}

#[inline]
fn get_bit(val: c_long, bit: u8) -> bool {
    (val & (1 << bit)) != 0
}

#[inline]
fn set_bit(val: &mut c_long, bit: u8, to: bool) {
    let mask = 1u64 << bit;
    if to {
        *val |= mask as c_long;
    } else {
        *val &= !(mask as c_long);
    }
}
