use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use std::{iter, thread};

use anyhow::Result;
use facto_exporter::debug::elf::{find_function, full_symbol_table, Symbol};
use facto_exporter::debug::inject::{inject_mmap, CraftingLite, Shell};
use facto_exporter::debug::pad_to_word;
use facto_exporter::debug::ptrace::{
    breakpoint, debug_to_int3, find_executable_map, run_until_stop, wait_for_stop,
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
    let crafting_lite_size = std::mem::size_of::<CraftingLite>() as u64;
    assert_eq!(crafting_lite_size, 4 * 4);

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

    let set_addr = alloc_fake_set(pid, from)?;

    let shell = Shell::inject_into(pid, from)?;
    shell.set_set_addr(set_addr)?;

    println!("shell written, resuming...");
    run_until_stop(pid)?;
    assert_eq!([false, false, true, false], which_breakpoints(pid)?);

    println!("jumping to shell...");
    shell.enter()?;

    debug_to_int3(pid, shell.map_addr)?;

    // size
    let count = shell.read_count()?;
    assert_eq!(4, count, "{count}, {count:#x}");

    let craftings = shell.read_craftings()?;
    assert_eq!(count, craftings.len());

    let mock = 0xf00dd00d;
    let c = |unit, products, status| CraftingLite {
        unit,
        products,
        status,
        _reserved: 0,
    };
    assert_eq!(
        [
            c(0x100, 0x1000, mock),
            c(0x101, 0x1001, mock),
            c(0x102, 0x1002, mock),
            c(0x103, 0x1003, mock),
        ],
        craftings.as_slice()
    );

    println!("checking it isn't completely corrupt...");
    run_until_stop(pid)?;

    Ok(())
}

fn alloc_fake_set(pid: Pid, from: u64) -> Result<u64> {
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
    Ok(set_addr)
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
