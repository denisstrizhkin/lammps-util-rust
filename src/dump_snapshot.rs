use std::collections::HashMap;
use std::fmt;
use std::io;

use itertools::izip;
use log::debug;

use crate::dump_file::DumpParsingError;
use crate::XYZ;

pub const HEADER_TIMESTEP: &str = "ITEM: TIMESTEP";
pub const HEADER_NUM_OF_ATOMS: &str = "ITEM: NUMBER OF ATOMS";
pub const HEADER_SYM_BOX: &str = "ITEM: BOX BOUNDS";
pub const HEADER_ATOMS: &str = "ITEM: ATOMS";

#[derive(Debug, Clone)]
pub struct SymBox {
    pub boundaries: String,
    pub xlo: f64,
    pub xhi: f64,
    pub ylo: f64,
    pub yhi: f64,
    pub zlo: f64,
    pub zhi: f64,
}

#[derive(Clone)]
pub struct DumpSnapshot {
    pub step: u64,
    pub atoms_count: usize,
    pub sym_box: SymBox,
    keys: HashMap<String, usize>,
    atoms: Vec<f64>,
}

impl DumpSnapshot {
    pub fn new(
        keys: HashMap<String, usize>,
        step: u64,
        atoms_count: usize,
        sym_box: SymBox,
    ) -> Self {
        Self {
            step,
            atoms_count,
            atoms: vec![0.0; atoms_count * keys.len()],
            keys,
            sym_box,
        }
    }

    pub fn read<I>(lines: &mut I, step: u64, atoms_count: usize) -> Result<Self, DumpParsingError>
    where
        I: Iterator<Item = String>,
    {
        debug!("reading snapshot");
        let sym_box = match lines.next().and_then(|l| {
            l.split_at_checked(HEADER_SYM_BOX.len())
                .map(|(_, boundaries)| boundaries.to_string())
        }) {
            Some(boundaries) => {
                let borders: Vec<(f64, f64)> = (0..3)
                    .filter_map(|_| {
                        lines.next().map(|l| {
                            let mut s = l.split_whitespace();
                            (
                                s.next().and_then(|s| s.parse::<f64>().ok()),
                                s.next().and_then(|s| s.parse::<f64>().ok()),
                            )
                        })
                    })
                    .filter_map(|p| match p {
                        (Some(lo), Some(hi)) => Some((lo, hi)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if borders.len() != 3 {
                    return Err(DumpParsingError::MissingSymBox);
                }
                SymBox {
                    boundaries: boundaries[1..].to_string(),
                    xlo: borders[0].0,
                    xhi: borders[0].1,
                    ylo: borders[1].0,
                    yhi: borders[1].1,
                    zlo: borders[2].0,
                    zhi: borders[2].1,
                }
            }
            _ => return Err(DumpParsingError::MissingSymBox),
        };
        debug!("read symbox");
        let mut keys_map = HashMap::new();
        match lines.next().and_then(|l| {
            l.split_at_checked(HEADER_ATOMS.len())
                .map(|(_, boundaries)| boundaries.to_string())
        }) {
            Some(tokens) => {
                for token in tokens.split_whitespace() {
                    if keys_map.insert(token.to_string(), keys_map.len()).is_some() {
                        return Err(DumpParsingError::DuplicateAtomKeys);
                    }
                }
            }
            _ => return Err(DumpParsingError::MissingAtomKeys),
        }
        let mut snapshot = DumpSnapshot::new(keys_map, step, atoms_count, sym_box);
        for i in 0..atoms_count {
            let values: Vec<f64> = lines
                .next()
                .map(|l| l.split_whitespace().flat_map(str::parse::<f64>).collect())
                .ok_or(DumpParsingError::InvalidOrMissingAtomRow)?;
            for (j, val) in values.into_iter().enumerate() {
                snapshot.set_atom_value(j, i, val);
            }
        }
        Ok(snapshot)
    }

    pub fn write<W>(&self, w: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        writeln!(w, "{HEADER_TIMESTEP}")?;
        writeln!(w, "{}", self.step)?;
        writeln!(w, "{HEADER_NUM_OF_ATOMS}")?;
        writeln!(w, "{}", self.atoms_count)?;
        writeln!(w, "{HEADER_SYM_BOX} {}", self.sym_box.boundaries)?;
        writeln!(w, "{} {}", self.sym_box.xlo, self.sym_box.xhi)?;
        writeln!(w, "{} {}", self.sym_box.ylo, self.sym_box.yhi)?;
        writeln!(w, "{} {}", self.sym_box.zlo, self.sym_box.zhi)?;
        writeln!(w, "{HEADER_ATOMS} {}", self.get_keys().join(" "))?;
        for i in 0..self.atoms_count {
            write!(w, "{}", self.atoms[i])?;
            for j in 1..self.keys.len() {
                write!(w, " {}", self.get_atom_value(j, i))?;
            }
            writeln!(w)?;
        }
        Ok(())
    }

    pub fn get_keys_map(&self) -> &HashMap<String, usize> {
        &self.keys
    }

    pub fn get_keys(&self) -> Vec<&str> {
        let mut keys: Vec<(&String, &usize)> = self.keys.iter().collect();
        keys.sort_by(|a, b| a.1.cmp(b.1));
        keys.into_iter().map(|i| i.0.as_str()).collect()
    }

    pub fn get_property_index(&self, key: &str) -> usize {
        self.keys[key]
    }

    pub fn get_property(&self, key: &str) -> &[f64] {
        let start = self.keys[key] * self.atoms_count;
        let end = start + self.atoms_count;
        &self.atoms[start..end]
    }

    pub fn get_property_mut(&mut self, key: &str) -> &mut [f64] {
        let start = self.keys[key] * self.atoms_count;
        let end = start + self.atoms_count;
        &mut self.atoms[start..end]
    }

    pub fn get_atom_value(&self, property_index: usize, atom_index: usize) -> f64 {
        self.atoms[self.atoms_count * property_index + atom_index]
    }

    pub fn set_atom_value(&mut self, property_index: usize, atom_index: usize, value: f64) {
        self.atoms[self.atoms_count * property_index + atom_index] = value
    }

    pub fn get_zero_lvl(&self) -> f64 {
        self.get_property("z")
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    pub fn get_coordinates(&self) -> Vec<XYZ> {
        izip!(
            self.get_property("x").iter(),
            self.get_property("y").iter(),
            self.get_property("z").iter()
        )
        .enumerate()
        .map(|(i, (&x, &y, &z))| XYZ::from([x, y, z], i))
        .collect()
    }
}

impl fmt::Debug for DumpSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DumpSnapshot")
            .field("step", &self.step)
            .field("atoms_count", &self.atoms_count)
            .field("keys", &self.keys)
            .finish()
    }
}

pub fn copy_snapshot(input_snapshot: &DumpSnapshot) -> DumpSnapshot {
    copy_snapshot_with_indices_with_keys(
        input_snapshot,
        std::iter::empty(),
        0..input_snapshot.atoms_count,
    )
}

pub fn copy_snapshot_with_indices(
    input_snapshot: &DumpSnapshot,
    indices: impl Iterator<Item = usize>,
) -> DumpSnapshot {
    copy_snapshot_with_indices_with_keys(input_snapshot, std::iter::empty(), indices)
}

pub fn copy_snapshot_with_keys<'a>(
    input_snapshot: &DumpSnapshot,
    additional_keys: impl Iterator<Item = &'a str>,
) -> DumpSnapshot {
    copy_snapshot_with_indices_with_keys(
        input_snapshot,
        additional_keys,
        0..input_snapshot.atoms_count,
    )
}

pub fn copy_snapshot_with_indices_with_keys<'a>(
    input_snapshot: &DumpSnapshot,
    additional_keys: impl Iterator<Item = &'a str>,
    indices: impl Iterator<Item = usize>,
) -> DumpSnapshot {
    let mut keys = input_snapshot.get_keys_map().clone();
    for key in additional_keys {
        keys.insert(key.to_string(), keys.len());
    }
    let indices = indices.collect::<Vec<_>>();
    let mut snapshot = DumpSnapshot::new(
        keys,
        input_snapshot.step,
        indices.len(),
        input_snapshot.sym_box.clone(),
    );
    for (new_i, i) in indices.into_iter().enumerate() {
        for (j, _) in input_snapshot.get_keys().iter().enumerate() {
            snapshot.set_atom_value(j, new_i, input_snapshot.get_atom_value(j, i));
        }
    }
    snapshot
}
