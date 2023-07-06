use std::ffi::c_void;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail};
use anyhow::{Context, Result};
use archiv::{Compress, CompressStream};
use elf::endian::AnyEndian;
use elf::ElfBytes;
use nix::libc::c_long;
use nix::sys::ptrace;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const DR0: *mut c_void = 848 as *mut c_void;
const DR1: *mut c_void = 856 as *mut c_void;
const DR6: *mut c_void = 896 as *mut c_void;
const DR7: *mut c_void = 904 as *mut c_void;

fn main() -> Result<()> {
    let mut archiv =
        archiv::CompressOptions::default().stream_compress(fs::File::create(path_for_now())?)?;
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
    let (crafting_insert, _) = find_symbol(&bin_path, "_ZNSt8_Rb_treeIP15CraftingMachineS1_St9_IdentityIS1_E20UnitNumberComparatorSaIS1_EE16_M_insert_uniqueIS1_EESt4pairISt17_Rb_tree_iteratorIS1_EbEOT_")?;
    let (game_update_step, _) = find_symbol(&bin_path, "_ZN8MainLoop14gameUpdateStepEP22MultiplayerManagerBaseP8ScenarioP10AppManagerNS_9HeavyModeE")?;
    println!("found crafting_insert() at 0x{crafting_insert:x}");

    let parent_pid = find_pid(bin_path)?;
    println!("found pid {parent_pid}");
    let game_update = Pid::from_raw(find_thread(parent_pid, "GameUpdate")?);
    println!("found GameUpdate thread {game_update}");

    ptrace::attach(game_update)?;
    waitpid(game_update, Some(WaitPidFlag::WSTOPPED))?;

    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))?;

    unsafe {
        ptrace::write_user(game_update, DR0, crafting_insert as *mut c_void)?;
        ptrace::write_user(game_update, DR1, game_update_step as *mut c_void)?;
        let mut dr7 = ptrace::read_user(game_update, DR7)?;
        // bit 0: local enable 0
        // bit 2: local enable 1
        dr7 |= 0b101;
        ptrace::write_user(game_update, DR7, dr7 as *mut c_void)?;
    }

    println!("debugging, waiting for an assembler place...");

    let mut hits = 0;

    let mut state = BodyState {
        game_update,
        set_base: 0,
        hits: 0,
    };

    while !term.load(Ordering::Relaxed) {
        ptrace::cont(game_update, None)?;
        let status = waitpid(game_update, Some(WaitPidFlag::WSTOPPED))?;
        match status {
            WaitStatus::Stopped(_, _) => (),
            _ => continue,
        };

        if let Err(e) = body(&mut state, &mut archiv) {
            println!("fatal error: {:?}", e);
            break;
        }
    }

    println!("detaching...");

    archiv.finish()?.flush()?;

    unsafe {
        let mut dr7 = ptrace::read_user(game_update, DR7)?;
        dr7 &= !0b101;
        ptrace::write_user(game_update, DR7, dr7 as *mut c_void)?;
    }

    ptrace::detach(game_update, None)?;

    Ok(())
}

struct BodyState {
    game_update: Pid,
    set_base: u64,
    hits: u64,
}

fn body(state: &mut BodyState, archiv: &mut CompressStream<fs::File>) -> Result<()> {
    let regs = ptrace::getregs(state.game_update)?;

    let dr6 = ptrace::read_user(state.game_update, DR6)?;

    if dr6 & 0b1 == 1 {
        println!(
            "hit place: old base: {:x}, new base: {:x}",
            state.set_base, regs.rdi
        );
        state.set_base = regs.rdi;
    }

    state.hits += 1;

    if state.hits % 60 != 0 {
        return Ok(());
    }
    if state.set_base == 0 {
        return Ok(());
    }
    let addresses = walk_set_u64(state.game_update, state.set_base)?;
    let mut buf = Vec::with_capacity(64 + addresses.len() * 8);
    buf.write_all(&OffsetDateTime::now_utc().unix_timestamp().to_le_bytes())?;
    println!("walked {:?} items", addresses.len());
    for data_ptr in addresses {
        let lite = read_crafting_lite(state.game_update, data_ptr)?;
        buf.write_all(&lite.unit_number.to_le_bytes())?;
        buf.write_all(&lite.products_complete.to_le_bytes())?;
    }

    archiv.write_item(&buf)?;

    Ok(())
}

#[derive(Debug)]
struct CraftingLite {
    unit_number: u32,
    products_complete: u32,
}

fn read_crafting_lite(pid: Pid, ptr: u64) -> Result<CraftingLite> {
    let [unit_number] = bulk_read_ptr(pid, ptr + 0x98)?;
    let [products_complete] = bulk_read_ptr(pid, ptr + 0x204)?;
    return Ok(CraftingLite {
        unit_number: (unit_number & 0xffff) as u32,
        products_complete: (products_complete & 0xffff) as u32,
    });
}

fn walk_set_u64(pid: Pid, set_base: u64) -> Result<Vec<u64>> {
    let [_unknown, _parent, begin, _end, _unknown_2, size] = bulk_read_ptr(pid, set_base)?;
    let mut ret = Vec::with_capacity(size.min(4096) as usize);
    let mut search = Vec::with_capacity(16);
    search.push(begin);
    while let Some(here) = search.pop() {
        // https://github.com/gcc-mirror/gcc/blob/85d8e0d8d5342ec8b4e6a54e22741c30b33c6f04/libstdc%2B%2B-v3/include/bits/stl_tree.h#L106-L109
        // I don't think this is really color, it's full of garbage
        let [_color, _parent, left_ptr, right_ptr, data_ptr] = bulk_read_ptr(pid, here)?;
        if left_ptr != 0 {
            search.push(left_ptr);
        }
        if right_ptr != 0 {
            search.push(right_ptr);
        }
        if data_ptr != 0 {
            ret.push(data_ptr);
        }
    }

    Ok(ret)
}

fn bulk_read_ptr<const N: usize>(pid: Pid, addr: u64) -> Result<[u64; N]> {
    let mut ret = [0u64; N];
    let mut offset = 0;
    for i in 0..N {
        let start = addr
            .checked_add(u64::try_from(offset)?)
            .ok_or(anyhow!("overflow during read"))?;
        let word = ptrace::read(pid, start as *mut _)?;
        ret[i] = word as u64;
        offset += 8;
    }
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
    let f = fs::read(bin_path)?;
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

fn find_thread(pid: i32, name: &str) -> Result<i32> {
    for d in std::fs::read_dir(format!("/proc/{}/task", pid))? {
        let d = d?;
        if !d.file_type()?.is_dir() {
            continue;
        }
        let tid = d.file_name().to_string_lossy().parse()?;
        let comm = std::fs::read_to_string(format!("/proc/{}/task/{}/comm", pid, tid))?;
        if comm.trim() == name {
            return Ok(tid);
        }
    }
    bail!("thread not found");
}

fn path_for_now() -> String {
    let time = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("static formatter");
    format!("{}.facto-cp.archiv", time)
}
