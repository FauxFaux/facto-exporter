use anyhow::{anyhow, Result};
use nix::libc::c_long;
use std::ffi::c_void;
use std::io::IoSliceMut;

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
pub fn read_words_ptr<const N: usize>(pid: Pid, addr: u64) -> Result<[u64; N]> {
    let mut ret = [0u64; N];
    for i in 0..N {
        let start = addr
            .checked_add(u64::try_from(i * 8)?)
            .ok_or(anyhow!("overflow during read"))?;
        let word = ptrace::read(pid, start as *mut _)?;
        ret[i] = word as u64;
    }
    Ok(ret)
}

pub fn write_words_ptr(pid: Pid, addr: u64, data: &[u8]) -> Result<()> {
    let mut addr = addr;
    let ptr_size = std::mem::size_of::<*mut c_void>();
    assert_eq!(ptr_size, 8, "64-bit machines only");
    assert_eq!(
        data.len() % ptr_size,
        0,
        "data of length {} is not divisible by the word size, 8",
        data.len()
    );
    for chunk in data.chunks_exact(ptr_size) {
        let word = u64::from_le_bytes(
            chunk
                .try_into()
                .expect("chunks_exact (or, in the future, array_chunks)"),
        );
        unsafe {
            ptrace::write(pid, addr as *mut _, word as *mut _)?;
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

/// issue a cont(), then wait for a stop; if we didn't get a stop, issue another cont() and try again repeatedly
pub fn cont_until_stop(pid: Pid) -> Result<Signal> {
    loop {
        ptrace::cont(pid, None)?;

        if let Some(signal) = wait_for_stop(pid)? {
            return Ok(signal);
        }
    }
}

/// issue a single waitpid, expecting a stop, and return Some if we hit it
pub fn wait_for_stop(pid: Pid) -> Result<Option<Signal>> {
    Ok(match waitpid(pid, Some(WaitPidFlag::WSTOPPED))? {
        WaitStatus::Stopped(stopped_pid, signal) => {
            assert_eq!(stopped_pid, pid, "waitpid should only return our pid");
            Some(signal)
        }
        _ => None,
    })
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
