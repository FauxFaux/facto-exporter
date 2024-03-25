pub mod elf;
pub mod inject;
mod mangle;
pub mod ptrace;

pub fn pad_to_word(buf: &[u8], with: u8) -> Vec<u64> {
    assert_eq!(std::mem::size_of::<usize>(), 8);

    let mut ret = Vec::with_capacity(buf.len() / 8 + 1);

    let mut it = buf.chunks_exact(8);
    while let Some(chunk) = it.next() {
        let arr = chunk.try_into().expect("chunks_exact");
        ret.push(u64::from_le_bytes(arr));
    }
    let mut remainder = it.remainder().to_vec();
    if !remainder.is_empty() {
        remainder.resize(8, with);
        let arr = remainder.try_into().expect("remainder");
        ret.push(u64::from_le_bytes(arr));
    }
    ret
}
