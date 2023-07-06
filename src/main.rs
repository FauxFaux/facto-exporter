use std::ffi::c_void;
use std::path::Path;

use anyhow::{Context, ensure, Result};
use anyhow::{anyhow, bail};
use elf::endian::AnyEndian;
use elf::ElfBytes;
use nix::libc::c_long;
use nix::sys::ptrace;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;

fn main() -> Result<()> {
    assert_eq!(std::mem::size_of::<c_long>(), 8);
    let bin_path = std::fs::canonicalize(
        std::env::args_os()
            .nth(1)
            .ok_or(anyhow!("usage: bin path"))?,
    )?;
    println!("loading symbols from {bin_path:?}...");
    let (products_addr, products_size) =
        find_symbol(&bin_path, "_ZN15CraftingMachine12giveProductsERK6Recipeb")?;
    println!("found products() at 0x{products_addr:x} for {products_size} bytes");


    let pid = find_pid(bin_path)?;
    let mut threads = find_threads(pid)?;
    println!("found pid {pid}");
    let parent = Pid::from_raw(threads.pop().expect("all processes have threads"));
    for pid in &threads {
        let pid = Pid::from_raw(*pid);
        ptrace::attach(pid)?;
        waitpid(pid, None)?;
        ptrace::setoptions(pid, ptrace::Options::PTRACE_O_TRACECLONE)?;
        ptrace::cont(pid, None)?;
    }
    println!("attaching to parent {pid}");
    ptrace::attach(parent)?;
    waitpid(parent, None)?;
    ptrace::setoptions(parent, ptrace::Options::PTRACE_O_TRACECLONE)?;
    let body = bulk_read(parent, products_addr, products_size)?;
    dump(&body, products_addr)?;
    // 83 83 04 02 00 00 01 xx: ADD PTR [rbx+0x204],0x1
    let instr = memchr::memmem::find(body.as_slice(), b"\x83\x83\x04\x02\x00\x00\x01")
        .ok_or(anyhow!("interesting instruction missing from function"))?;
    println!("found interesting instruction at offset 0x{instr:x}");
    ensure!(instr % 8 == 0, "instruction not aligned");
    let backup = &body[instr..instr + 8];
    let mut new: [u8; 8] = backup.try_into()?;
    new[0] = 0xcc;
    let new = c_long::from_le_bytes(new);
    println!("patching instruction to 0x{new:x}");
    let patch_address = (products_addr + u64::try_from(instr)?) as *mut _;
    unsafe {
        ptrace::write(parent, patch_address, new as *mut c_void)?;
    }
    println!("patched instruction");
    ptrace::cont(parent, None)?;
    let (active, status) = loop {
        let status = waitpid(None, Some(WaitPidFlag::WSTOPPED))?;
        match status {
            WaitStatus::Stopped(_, _) => (),
            _ => continue,
        };
        let active = status.pid().ok_or(anyhow!("no pid in {status:?}"))?;

        if false {
            break (active, status);
        }
        let mut regs = ptrace::getregs(active)
            .with_context(|| anyhow!("{status:?}"))?;
        let val = ptrace::read(active, (regs.rbx + 0x204) as *mut _)?;
        println!("asm:{:016x} products_finished:{}", regs.rbx, val & 0xffff);
        unsafe {
            ptrace::write(active, patch_address, c_long::from_le_bytes(backup.try_into()?) as *mut c_void)?;
        }
        regs.rip = patch_address as u64;
        ptrace::setregs(active, regs)?;
        ptrace::step(active, None)?;
        waitpid(active, Some(WaitPidFlag::WSTOPPED))?;
        unsafe {
            ptrace::write(active, patch_address, new as *mut c_void)?;
        }
        ptrace::cont(active, None)?;
    };

    println!("detaching via. {active} from {status:?}");

    unsafe {
        ptrace::write(active, patch_address, c_long::from_le_bytes(backup.try_into()?) as *mut c_void)?;
    }
    println!("unpatched instruction");
    println!("after: {:x}", ptrace::read(active, patch_address)?);
    ptrace::detach(active, None)?;
    println!("done");

    for pid in find_threads(parent.as_raw())? {
        let pid = Pid::from_raw(pid);
        if ptrace::interrupt(pid).is_ok() {
            waitpid(pid, Some(WaitPidFlag::WSTOPPED))?;
            println!("detaching from thread {pid}: {}", ptrace::detach(pid, None).is_ok());
        }
    }
    Ok(())
}

fn bulk_read(pid: Pid, addr: u64, size: usize) -> Result<Vec<u8>> {
    let mut ret = Vec::with_capacity(size);
    let mut offset = 0;
    while offset < size {
        let start = (addr
            .checked_add(u64::try_from(offset)?)
            .ok_or(anyhow!("overflow during read"))?);
        let word = ptrace::read(pid, start as *mut _)?;
        ret.extend_from_slice(&word.to_le_bytes());
        offset += 8;
    }
    ret.truncate(size);
    Ok(ret)
}

fn dump(mem: &[u8], addr: u64) -> Result<()> {
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

fn find_symbol(bin_path: impl AsRef<Path>, symbol: &str) -> Result<(u64, usize)> {
    let f = std::fs::read(bin_path)?;
    let f = f.as_slice();
    let f = ElfBytes::<AnyEndian>::minimal_parse(f)?;

    let common = f.find_common_data()?;
    let symtab = common.symtab.ok_or(anyhow!("no symtab"))?;
    let strtab = common.symtab_strs.ok_or(anyhow!("no strtab"))?;

    for sym in symtab {
        let name = strtab.get(usize::try_from(sym.st_name)?)?;
        if name == symbol {
            return Ok((sym.st_value, usize::try_from(sym.st_size)?));
        }
    }

    bail!("{symbol} not found");
}

fn find_pid(bin_path: impl AsRef<Path>) -> Result<i32> {
    let mut candidates = Vec::with_capacity(4);
    let bin_path = bin_path.as_ref();
    for d in std::fs::read_dir("/proc")? {
        let d = d?;
        if !d.file_type()?.is_dir() {
            continue;
        }
        match d.path().join("exe").read_link() {
            Ok(p) => {
                if p == bin_path {
                    candidates.push(d.file_name().to_string_lossy().parse()?);
                }
            }
            Err(_) => continue,
        }
    }

    match candidates.len() {
        0 => bail!("pid not found"),
        1 => return Ok(candidates[0]),
        _ => bail!("multiple pids found"),
    }
}

fn find_threads(pid: i32) -> Result<Vec<i32>> {
    let mut ret = Vec::new();
    for d in std::fs::read_dir(format!("/proc/{}/task", pid))? {
        let d = d?;
        if !d.file_type()?.is_dir() {
            continue;
        }
        ret.push(d.file_name().to_string_lossy().parse()?);
    }
    Ok(ret)
}
