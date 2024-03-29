use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Result};
use cpp_demangle::DemangleOptions;
use elf::endian::AnyEndian;
use elf::ElfBytes;
use nix::unistd::Pid;

/// (address, size)
pub type Symbol = (u64, usize);

pub fn full_symbol_table(bin_path: impl AsRef<Path>) -> Result<HashMap<String, Symbol>> {
    let f = fs::read(bin_path)?;
    let f = f.as_slice();
    let f = ElfBytes::<AnyEndian>::minimal_parse(f)?;

    let common = f.find_common_data()?;
    let symtab = common.symtab.ok_or(anyhow!("no symtab"))?;
    let strtab = common.symtab_strs.ok_or(anyhow!("no strtab"))?;
    let mut ret = HashMap::with_capacity(symtab.len());

    for sym in symtab {
        let name = strtab.get(usize::try_from(sym.st_name)?)?;
        ret.insert(
            name.to_string(),
            (sym.st_value, usize::try_from(sym.st_size)?),
        );
    }

    Ok(ret)
}

pub fn find_pid(bin_path: impl AsRef<Path>) -> Result<Pid> {
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
        1 => return Ok(Pid::from_raw(candidates[0])),
        _ => bail!("multiple pids found"),
    }
}

pub fn find_threads(pid: i32) -> Result<Vec<i32>> {
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

pub fn find_thread(pid: Pid, name: &str) -> Result<Pid> {
    for d in fs::read_dir(format!("/proc/{}/task", pid))? {
        let d = d?;
        if !d.file_type()?.is_dir() {
            continue;
        }
        let tid = d.file_name().to_string_lossy().parse()?;
        let comm = std::fs::read_to_string(format!("/proc/{}/task/{}/comm", pid, tid))?;
        if comm.trim() == name {
            return Ok(Pid::from_raw(tid));
        }
    }
    bail!("thread not found");
}

/// demangle function names and take the first one that looks about right
pub fn find_function(table: &HashMap<String, Symbol>, name: &str) -> Result<(String, u64, usize)> {
    if let Some((addr, size)) = table.get(name) {
        return Ok((name.to_string(), *addr, *size));
    }

    let opts = DemangleOptions::default().no_params().no_return_type();

    for (raw, (addr, size)) in table {
        if !raw.contains(name) {
            continue;
        }
        let Ok(sym) = cpp_demangle::Symbol::new(raw) else {
            continue;
        };
        let Ok(sym) = sym.demangle(&opts) else {
            continue;
        };
        if sym != name {
            continue;
        }

        return Ok((raw.to_string(), *addr, *size));
    }

    bail!("no function found")
}
