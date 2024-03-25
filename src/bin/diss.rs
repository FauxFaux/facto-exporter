use std::fs;

use anyhow::{anyhow, Context, Result};
use cpp_demangle::Symbol;
use facto_exporter::debug::elf::full_symbol_table;
use facto_exporter::debug::mangle::demangle;
use iced_x86::{Decoder, DecoderOptions, Formatter, NasmFormatter};

fn main() -> Result<()> {
    let bin_path = fs::canonicalize(
        std::env::args_os()
            .nth(1)
            .ok_or(anyhow!("usage: bin path"))?,
    )?;

    let name_to_loc = full_symbol_table(&bin_path)?;
    let bin = fs::read(&bin_path)?;

    for (name, (loc, size)) in name_to_loc {
        if !name.contains("CraftingMachine") {
            continue;
        }
        // failure to demangle is normally e.g. C names
        let sym = Symbol::new(name.as_str())?;
        let func = demangle(&sym).with_context(|| anyhow!("raw: {name:?}"))?;
        println!("{}: {:#x} {:#x}: {func:?}", sym, loc, size);

        let mut decoder = Decoder::with_ip(64, &bin, loc, DecoderOptions::NONE);
        decoder.set_position(usize::try_from(loc - 0x400000)?)?;
        let mut formatter = NasmFormatter::new();

        while decoder.can_decode() {
            let instr = decoder.decode();

            let mut s = String::new();
            formatter.format(&instr, &mut s);
            print!("{:016X} ", instr.ip());
            println!("{}", s);
            if decoder.ip() >= loc + u64::try_from(size)? {
                break;
            }
        }
    }

    Ok(())
}
