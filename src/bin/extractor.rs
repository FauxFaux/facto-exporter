use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use std::{fs, thread};

use anyhow::anyhow;
use anyhow::{ensure, Result};
use archiv::Compress;
use nix::libc::c_long;
use nix::sys::ptrace;
use nix::unistd::Pid;
use reqwest::StatusCode;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use facto_exporter::debug::elf::{find_pid, find_thread, full_symbol_table};
use facto_exporter::debug::pad_to_word;
use facto_exporter::debug::ptrace::{
    breakpoint, bulk_read, read_words_arr, run_until_stop, wait_for_stop, which_breakpoints,
    write_words_ptr,
};
use facto_exporter::{pack_observation, CraftingLite, Observation};

#[tokio::main]
async fn main() -> Result<()> {
    let archiv = Arc::new(std::sync::Mutex::new(Some(
        archiv::CompressOptions::default().stream_compress(fs::File::create(path_for_now())?)?,
    )));
    assert_eq!(std::mem::size_of::<c_long>(), 8);
    let bin_path = fs::canonicalize(
        std::env::args_os()
            .nth(1)
            .ok_or(anyhow!("usage: bin path"))?,
    )?;
    println!("loading symbols from {bin_path:?}...");
    let symtab = full_symbol_table(&bin_path)?;
    let find_symbol = |symbol: &str| -> Result<(u64, usize)> {
        Ok(*symtab
            .get(symbol)
            .ok_or_else(|| anyhow!("{symbol} not found"))?)
    };
    let (products_addr, products_size) =
        find_symbol("_ZN15CraftingMachine12giveProductsERK6Recipeb")?;
    println!("found products() at 0x{products_addr:x} for {products_size} bytes");
    let (crafting_insert, _) = find_symbol("_ZNSt8_Rb_treeIP15CraftingMachineS1_St9_IdentityIS1_E20UnitNumberComparatorSaIS1_EE16_M_insert_uniqueIS1_EESt4pairISt17_Rb_tree_iteratorIS1_EbEOT_")?;
    let (game_update_step, _) =
        // 1.1.53
        find_symbol("_ZN8MainLoop14gameUpdateStepEP22MultiplayerManagerBaseP8ScenarioP10AppManagerNS_9HeavyModeE")
            // 1.1.104
            .or_else(|_| find_symbol("_ZN8MainLoop14gameUpdateStepEP22MultiplayerManagerBaseP8ScenarioP10AppManagerNS_9HeavyModeE.isra.0"))?;
    let (symbol_main, _) = find_symbol("main")?;
    let (symbol_crafting_status, _) = find_symbol("_ZNK15CraftingMachine9getStatusEv")?;

    // using crypto variants as they're statically linked; we don't have to deal with dynamic linking
    let (symbol_malloc, _) = find_symbol("CRYPTO_malloc")?;
    let (symbol_free, _) = find_symbol("CRYPTO_free")?;

    println!("found main() at 0x{symbol_main:x}");
    println!("found malloc() at 0x{symbol_malloc:x}");
    println!("found crafting_insert() at 0x{crafting_insert:x}");

    let parent_pid = find_pid(bin_path)?;
    println!("found pid {parent_pid}");
    let game_update = find_thread(parent_pid, "GameUpdate")?;
    println!("found GameUpdate thread {game_update}");

    ptrace::attach(game_update)?;
    wait_for_stop(game_update)?;

    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))?;

    write_words_ptr(
        game_update,
        symbol_main,
        &pad_to_word(include_bytes!("../../shellcode/crafting.bin"), 0x90),
    )?;

    breakpoint(
        game_update,
        [Some(crafting_insert), Some(game_update_step), None, None],
    )?;

    println!("debugging, waiting for an assembler place...");

    let mut state = BodyState {
        game_update,
        set_base: 0,
        // these are internally consistent, even though they're nonsense
        // I don't really care about games with no assemblers
        set_size: 0,
        set_data: Vec::new(),
        hits: 0,
        symbols: Symbols {
            shell: symbol_main,
            malloc: symbol_malloc,
            free: symbol_free,
            crafting_status: symbol_crafting_status,
        },
    };

    // this whole loop is horribly unsafe; the cleanup is afterwards,
    // and can't be run unless then process is stopped, so you can't break or error
    while !term.load(Ordering::SeqCst) {
        run_until_stop(game_update)?;

        let start = Instant::now();
        let obs = match observe(&mut state) {
            Ok(Some(obs)) => {
                println!("observed in {:?}", start.elapsed());
                obs
            }
            Ok(None) => continue,
            Err(e) => {
                println!("error: {:?}", e);
                break;
            }
        };

        // this is just bincode, so pretty much can't fail (right?)
        let packed = pack_observation(&obs)?;
        let packed2 = packed.clone();

        let term = Arc::clone(&term);
        let archiv = Arc::clone(&archiv);
        // i.e. go back around the loop and continue doing nothing while this is writing
        thread::spawn(move || {
            let mut archiv = archiv.lock().expect("no thread panic");
            let archiv = match archiv.as_mut() {
                Some(archiv) => archiv,
                // only none during cleanup
                None => return,
            };
            let mut tried = || -> Result<()> {
                archiv.write_item(&packed)?;
                archiv.flush()?;
                Ok(())
            };
            if let Err(e) = tried() {
                eprintln!("archiv error: {:?}", e);
                term.store(true, Ordering::SeqCst);
            }
        });

        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let res = client
                .post("http://localhost:9429/exp/store")
                .body(packed2)
                .send()
                .await;
            match res {
                Ok(res) if res.status() == StatusCode::ACCEPTED => (),
                Ok(res) => eprintln!("surprising send response: {:?}", res),
                Err(e) => eprintln!("send error: {:?}", e),
            }
        });
    }

    println!("detaching...");

    breakpoint(game_update, [None, None, None, None])?;

    ptrace::detach(game_update, None)?;

    match archiv.lock() {
        // if_let_guard unavailable due to mutation in take
        // nested destructuring doesn't understand mutex guard (or I don't)
        Ok(mut archiv) if archiv.is_some() => {
            archiv.take().expect("just checked").finish()?.flush()?;
        }
        _ => {
            eprintln!("archiv poisoned or None, ignoring for shutdown");
        }
    }

    Ok(())
}

struct Symbols {
    shell: u64,
    malloc: u64,
    free: u64,
    crafting_status: u64,
}

struct BodyState {
    game_update: Pid,
    set_base: u64,
    // re-read the data iff there's an insert, or the size has changed
    set_size: u64,
    set_data: Vec<u64>,
    hits: u64,
    symbols: Symbols,
}

fn observe(state: &mut BodyState) -> Result<Option<Observation>> {
    let regs = ptrace::getregs(state.game_update)?;

    let hits = which_breakpoints(state.game_update)?;

    if hits[0] {
        println!(
            "hit place: old base: {:x}, new base: {:x}",
            state.set_base, regs.rdi
        );
        state.set_base = regs.rdi;
        // we can't update the actual data here, 'cos we know it is just about to change,
        // just leave this as "before" data, so the next tick re-reads the full data
        state.set_size = read_set_size(state)?;
    }

    state.hits += 1;

    // only work every N game seconds (N real seconds at 60UPS, N/2 at 30UPS)
    if state.hits % (60 * 7) != 0 {
        return Ok(None);
    }
    if state.set_base == 0 {
        return Ok(None);
    }

    // println!("stepped over the breakpoint?");
    // ptrace::step(state.game_update, None)?;
    // waitpid(state.game_update, Some(WaitPidFlag::WSTOPPED))?;

    println!("stopped, ready to jump to shell");
    let orig_regs = ptrace::getregs(state.game_update)?;
    let mut regs = orig_regs.clone();
    regs.rip = state.symbols.shell;
    regs.rdi = state.set_base;
    regs.rsi = state.symbols.malloc;
    regs.rdx = state.symbols.free;
    regs.rcx = state.symbols.crafting_status;
    ptrace::setregs(state.game_update, regs)?;

    println!("shell jumped to {:x}", regs.rip);
    let [first_word] = read_words_arr(state.game_update, regs.rip)?;
    println!("first word: {:x}", first_word);

    run_until_stop(state.game_update)?;
    let results = ptrace::getregs(state.game_update)?;
    // println!("rip: {:x}", results.rip);
    // println!("r10: {:x}", results.r10);
    // println!("r11: {:x}", results.r11);

    ensure!(
        results.r10 > 1000,
        "debug status code in r10? {}",
        results.r10
    );
    ensure!(
        results.r11 < 1024 * 1024,
        "sane item count: {}",
        results.r11
    );
    let size_of_c_crafting_lite = 3 * 4;
    let buf = bulk_read(
        state.game_update,
        results.r10 as usize,
        results.r11 as usize * size_of_c_crafting_lite,
    )?;

    // let the free() run
    run_until_stop(state.game_update)?;
    // jump back
    ptrace::setregs(state.game_update, orig_regs)?;

    let mut lites = buf
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("exact")))
        // TODO: pointless collect
        .collect::<Vec<_>>()
        .chunks_exact(3)
        .map(|c| {
            assert_eq!(c.len(), 3);
            CraftingLite {
                unit_number: c[0],
                products_complete: c[1],
                status: c[2],
            }
        })
        .collect::<Vec<_>>();

    lites.sort_unstable_by_key(|l| l.unit_number);

    Ok(Some(Observation {
        time: OffsetDateTime::now_utc(),
        inner: lites,
    }))
}

fn read_set_size(state: &BodyState) -> Result<u64> {
    let [size] = read_words_arr(state.game_update, state.set_base + 40)?;
    Ok(size)
}

fn path_for_now() -> String {
    let time = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("static formatter");
    format!("{}.facto-cp.archiv", time)
}
