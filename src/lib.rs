use anyhow::Result;
use bincode::Options;
use std::io::Read;
use time::OffsetDateTime;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CraftingLite {
    pub unit_number: u32,
    pub products_complete: u32,
    pub status: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Observation {
    pub time: OffsetDateTime,
    pub inner: Vec<CraftingLite>,
}

fn bincode() -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
}

pub fn pack_observation(obs: &Observation) -> Result<Vec<u8>> {
    Ok(bincode().serialize(obs)?)
}

pub fn unpack_observation(r: impl Read) -> Result<Observation> {
    Ok(bincode().deserialize_from(r)?)
}
