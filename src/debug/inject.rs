use std::slice;

use anyhow::{anyhow, ensure, Result};
use nix::sys::ptrace;
use nix::unistd::Pid;

use super::pad_to_word;
use super::ptrace::{read_words_arr, read_words_var, run_until_stop, write_words_ptr};

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CraftingLite {
    pub unit: u32,
    pub products: u32,
    pub status: u32,
    pub _reserved: u32,
}

pub struct Shell {
    pid: Pid,
    // TODO: private?
    pub map_addr: u64,
    shared_addr: u64,
}

impl Shell {
    const S_SET: u64 = 0;
    const S_GET_STATUS: u64 = 8;

    // S_CAPACITY = 16
    const S_COUNT: u64 = 24;
    const S_DATA: u64 = 32;

    /// precondition: thread is stopped at a reasonable place, e.g. a breakpoint outside of a lock or syscall
    pub fn inject_into(pid: Pid, working_map: u64) -> Result<Self> {
        let map_addr = inject_mmap(pid, working_map)?;
        let mut mem = Vec::with_capacity(64);

        let (code, mock_get_status) = shell_code();
        mem.extend_from_slice(&code);
        let mock_get_status_addr = map_addr + 8 * mock_get_status;

        let shared_addr = map_addr + 8 * (mem.len() as u64);

        // now, crafting2.c's interface struct, "Shared":
        // 0-8: pointer to the set, in the real will be set by code
        mem.push(0);
        // 8-16: pointer to the get
        mem.push(mock_get_status_addr);
        // 16-24: estimated capacity, based on the size of the mmap from stage1
        mem.push(60 * 1024 * 1024 / std::mem::size_of::<CraftingLite>() as u64);
        // 24-32: size, set by code
        mem.push(0);
        // 32+: data as a list of CraftingLite

        write_words_ptr(pid, map_addr, &mem)?;

        Ok(Self {
            pid,
            map_addr,
            shared_addr,
        })
    }

    pub fn enter(&self) -> Result<()> {
        let mut regs = ptrace::getregs(self.pid)?;
        regs.rip = self.map_addr;

        // first param
        regs.rdi = self.shared_addr;
        ptrace::setregs(self.pid, regs)?;

        Ok(())
    }

    pub fn set_set_addr(&self, set_addr: u64) -> Result<()> {
        write_words_ptr(self.pid, self.shared_addr + Self::S_SET, &[set_addr])?;
        Ok(())
    }

    pub fn set_get_status_addr(&self, get_status_addr: u64) -> Result<()> {
        write_words_ptr(
            self.pid,
            self.shared_addr + Self::S_GET_STATUS,
            &[get_status_addr],
        )?;
        Ok(())
    }

    pub fn read_count(&self) -> Result<usize> {
        let [count] = read_words_arr(self.pid, self.shared_addr + Self::S_COUNT)?;
        Ok(count as usize)
    }

    pub fn read_craftings(&self) -> Result<Vec<CraftingLite>> {
        let count = self.read_count()?;
        let crafting_lite_size = std::mem::size_of::<CraftingLite>();

        let needed_bytes = crafting_lite_size * count;
        assert_eq!(crafting_lite_size % 8, 0);
        let needed_words = needed_bytes / 8;

        let words = read_words_var(self.pid, self.shared_addr + Self::S_DATA, needed_words)?;

        // is this just transmute?
        let craftings =
            unsafe { slice::from_raw_parts(words.as_ptr() as *const CraftingLite, count) };

        Ok(craftings.to_vec())
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        let _ = ptrace::detach(self.pid, None);
    }
}

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
fn shell_code() -> (Vec<u64>, u64) {
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
