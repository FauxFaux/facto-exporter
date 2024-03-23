use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use std::{iter, thread};

use anyhow::Result;
use facto_exporter::debug::elf::{find_function, full_symbol_table, Symbol};
use facto_exporter::debug::inject::{entry_in_addr, inject_mmap};
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

    let set_off = 32;
    let mut mem = Vec::with_capacity(4096);
    mem.extend(iter::repeat(0).take(set_off));

    let mut craftings = Vec::new();
    for i in 0..4 {
        craftings.push(mem.len());
        let mut crafting = FakeCrafting::default();
        crafting.data[0x26] = 0x100 + i;
        crafting.data[0x81] = 0x1000 + i;
        mem.extend_from_slice(&bytemuck::bytes_of(&crafting));
    }

    let mut entries = Vec::new();
    let root = place(&mut entries, &craftings);
    let set_size = std::mem::size_of::<FakeSetEntry>();
    let set_base = fake_structs_addr + mem.len() as u64;
    let to_set_addr = |x: Option<usize>| x.map(|x| set_base + (set_size * x) as u64).unwrap_or(0);
    for (left, right, crafting) in entries {
        mem.extend_from_slice(&bytemuck::bytes_of(&FakeSetEntry {
            left: to_set_addr(left),
            right: to_set_addr(right),
            data: fake_structs_addr + crafting as u64,
            ..FakeSetEntry::default()
        }));
    }

    let set_addr = fake_structs_addr + mem.len() as u64;
    mem.extend_from_slice(&bytemuck::bytes_of(&FakeSet {
        begin: to_set_addr(root),
        size: craftings.len(),
        ..FakeSet::default()
    }));

    write_words_ptr(pid, fake_structs_addr, &pad_to_word(&mem, 0x66))?;

    let call_end = pad_to_word(include_bytes!("../shellcode/call-end.bin"), 0xcc);
    assert_eq!(call_end.len(), 1);

    let mock_get_status = pad_to_word(include_bytes!("../shellcode/mock-get-status.bin"), 0xcc);
    assert_eq!(mock_get_status.len(), 1);

    // if this isn't right, call-end.bin needs to learn to jump further forward
    assert_eq!(
        0,
        entry_in_addr(include_str!("../shellcode/crafting2.bin.addr"))?
    );

    // if this isn't right, mock_get_status_addr needs handling (but it won't change)
    assert_eq!(
        0,
        entry_in_addr(include_str!("../shellcode/mock-get-status.bin.addr"))?
    );

    let mut mem = Vec::with_capacity(64);
    // 0-8: jump to code
    mem.extend_from_slice(&call_end);
    // 8-whatever: code
    mem.extend(pad_to_word(
        include_bytes!("../shellcode/crafting2.bin"),
        0xcc,
    ));

    let mock_get_status_addr = map_addr + 8 * mem.len() as u64;
    mem.extend_from_slice(&mock_get_status);

    let shellcode_fits_in = 1024;
    // padding
    assert!(mem.len() < shellcode_fits_in / 8);
    mem.resize(shellcode_fits_in / 8, 0xcc);
    let mem_addr = map_addr + u64::try_from(shellcode_fits_in).expect("sub-128bit machine please");

    // now, crafting2.c's interface struct, "Mem":
    // 0-8: pointer to the set, in the real world will be set by code
    mem.push(set_addr);
    // 8-16: pointer to the get
    mem.push(mock_get_status_addr);
    // 16-24: estimated capacity, based on the size of the mmap from stage1
    mem.push(60 * 1024 * 1024 / 8 / 3);
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

        // trap was from an int3 (0xcc)
        if word.to_le_bytes()[0] == 0xcc {
            break;
        }
    }

    // size
    let [word] = read_words_arr(pid, mem_addr + 24)?;
    assert_eq!(4, word, "{word}, {word:#x}");
    let words = read_words_var(pid, mem_addr + 32, 8)?;
    let words = words
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect::<Vec<_>>();
    let words = words
        .chunks_exact(4)
        .map(|x| u32::from_le_bytes(x.try_into().expect("chunks_exact")))
        .collect::<Vec<_>>();

    let mock = 0xf00dd00d;
    assert_eq!(
        [
            0x100, 0x1000, mock, 0x101, 0x1001, mock, 0x102, 0x1002, mock, 0x103, 0x1003, mock, 0,
            0, 0, 0
        ],
        words.as_slice(),
        "{words:x?}"
    );
    // assert_eq!((0x100, 0x1000, 0), (unit, products, status), "{unit:#016x}, {products:#016x}, {status:#016x}");

    println!("checking it isn't completely corrupt...");
    run_until_stop(pid)?;

    Ok(())
}

// the layout here is arbitrary
fn place<T: Copy>(heap: &mut Vec<(Option<usize>, Option<usize>, T)>, vals: &[T]) -> Option<usize> {
    if vals.is_empty() {
        return None;
    }
    let (left, right) = vals.split_at(vals.len() / 2);
    let (us, right) = right.split_first().expect("len >= 1");
    let left = place(heap, left);
    let right = place(heap, right);

    let idx = heap.len();
    heap.push((left, right, *us));
    Some(idx)
}

#[test]
fn test_place() {
    let mut heap = Vec::new();
    let vals = [1, 2, 3, 4, 5, 6, 7];
    let root = place(&mut heap, &vals);

    for i in 0..heap.len() {
        println!("{i}: {:?}", heap[i]);
    }

    // the layout here is arbitrary
    assert_eq!(root, Some(6));
    assert_eq!(heap.len(), 7);

    assert_eq!(heap[0], (None, None, 1));
    assert_eq!(heap[1], (None, None, 3));
    assert_eq!(heap[2], (Some(0), Some(1), 2));
    assert_eq!(heap[3], (None, None, 5));
    assert_eq!(heap[4], (None, None, 7));
    assert_eq!(heap[5], (Some(3), Some(4), 6));
    assert_eq!(heap[6], (Some(2), Some(5), 4));
}

#[repr(C)]
#[derive(bytemuck::NoUninit, Copy, Clone, Default)]
struct FakeSet {
    _unknown: u64,
    _parent: u64,
    begin: u64, // *FakeSetEntry
    _end: u64,
    _unknown2: u64,
    size: usize,
}

#[repr(C)]
#[derive(bytemuck::NoUninit, Copy, Clone, Default)]
struct FakeSetEntry {
    _unknown: u64,
    _unknown2: u64,
    left: u64,  // *FakeSetEntry
    right: u64, // *FakeSetEntry
    data: u64,  // *FakeCrafting
}

#[repr(C)]
#[derive(bytemuck::NoUninit, Copy, Clone)]
struct FakeCrafting {
    data: [u32; 256],
}

impl Default for FakeCrafting {
    fn default() -> Self {
        Self { data: [0; 256] }
    }
}
