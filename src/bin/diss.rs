use std::collections::HashMap;
use std::fs;

use anyhow::{anyhow, Result};
use cpp_demangle::{DemangleNodeType, DemangleWrite, Symbol};
use iced_x86::{Decoder, DecoderOptions, Formatter, NasmFormatter};

fn main() -> Result<()> {
    let bin_path = std::fs::canonicalize(
        std::env::args_os()
            .nth(1)
            .ok_or(anyhow!("usage: bin path"))?,
    )?;

    let bin = fs::read(bin_path)?;
    let name_to_loc = symbol_table(&bin)?;

    // TrainsGui::TrainsGui(GuiActionHandler&, Player const&, TrainManager&)
    let args_re = regex::Regex::new(r"(\w+)::~?(\w+)\(((?:\w+[&*]?,? ?)*)\)(?: \[.*?\])?")?;

    for (name, (loc, size)) in name_to_loc {
        if !name.contains("CraftingMachine") {
            continue;
        }
        // failure to demangle is normally e.g. C names
        let name = Symbol::new(name)
            .map(|s| s.to_string())
            .unwrap_or_else(|_| name.to_string());
        let ca = match args_re.captures(&name) {
            Some(ca) => ca,
            None => {
                println!("No matches in {name}");
                continue;
            }
        };
        let clazz = ca.get(1).expect("static").as_str();
        let method = ca.get(2).expect("static").as_str();
        let args = ca
            .get(3)
            .expect("static")
            .as_str()
            .split(", ")
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        println!(
            "{}: {:#x} {:#x}: {}::{} ({args:?})",
            name, loc, size, clazz, method
        );
        if !args.is_empty() {
            continue;
        }
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

fn symbol_table(bin: &[u8]) -> Result<HashMap<&str, (u64, usize)>> {
    use elf::endian::AnyEndian;
    use elf::ElfBytes;
    let bin = ElfBytes::<AnyEndian>::minimal_parse(bin)?;
    let common = bin.find_common_data()?;
    let symtab = common.symtab.ok_or(anyhow!("no symtab"))?;
    let strtab = common.symtab_strs.ok_or(anyhow!("no strtab"))?;
    let mut symbols = HashMap::with_capacity(symtab.len());
    for sym in symtab {
        let name = strtab.get(usize::try_from(sym.st_name)?)?;
        symbols.insert(name, (sym.st_value, usize::try_from(sym.st_size)?));
    }
    symbols.shrink_to_fit();
    Ok(symbols)
}

#[allow(dead_code)]
fn structured_demangle() {
    struct S {
        d: usize,
    }

    impl DemangleWrite for S {
        fn push_demangle_node(&mut self, nt: DemangleNodeType) {
            println!("{}{nt:?}:", "  ".repeat(self.d));
            self.d += 1;
        }

        fn write_string(&mut self, s: &str) -> std::fmt::Result {
            println!("{}{s:?}", "  ".repeat(self.d));
            Ok(())
        }

        fn pop_demangle_node(&mut self) {
            self.d -= 1;
        }
    }

    // println!("{}: {:#x} {:#x}: {}", name, loc, size, sym);
    // sym.structured_demangle(&mut S { d: 0 }, &DemangleOptions::default())?;
}
