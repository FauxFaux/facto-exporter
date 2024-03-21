use anyhow::anyhow;
use nix::sys::ptrace;
use nix::unistd::Pid;

#[inline]
pub fn read_words_ptr<const N: usize>(pid: Pid, addr: u64) -> anyhow::Result<[u64; N]> {
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

pub fn write_words_ptr(pid: Pid, addr: u64, data: &[u8]) -> anyhow::Result<()> {
    let mut addr = addr;
    for chunk in data.chunks(8) {
        // let mut bytes = [0u8; 8];
        // for (i, byte) in chunk.iter().enumerate() {
        //     bytes[i] = *byte;
        // }
        if chunk.len() != 8 {
            // TODO: MASSIVELY FAKE
            continue;
        }
        let word = u64::from_le_bytes(chunk.try_into()?);
        unsafe {
            ptrace::write(pid, addr as *mut _, word as *mut _)?;
        }
        addr += 8;
    }
    Ok(())
}

pub fn dump(mem: &[u8], addr: u64) -> anyhow::Result<()> {
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
