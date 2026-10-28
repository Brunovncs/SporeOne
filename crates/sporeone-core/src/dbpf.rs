//! The resources in a `.package` (Spore's DBPF archives), read from their index alone. Two mods
//! that carry a resource with the same type, group and instance both replace that one resource of
//! the game, and only one of them wins: the one the game loads last.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// A resource's type, group and instance.
pub type Key = (u32, u32, u32);

/// A resource and a hash of its bytes as stored: two mods carrying the same resource with the same
/// bytes do not get in each other's way.
pub type Resource = (Key, u64);

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// The keys of every resource in the package `r` holds, sorted, without reading the resources.
pub fn keys<R: Read + Seek>(r: &mut R) -> Result<Vec<Key>, String> {
    let mut keys: Vec<Key> = entries(r)?.into_iter().map(|(k, _, _)| k).collect();
    keys.sort_unstable();
    keys.dedup();
    Ok(keys)
}

/// Every resource in the package `r` holds, with a hash of its bytes, sorted.
pub fn resources<R: Read + Seek>(r: &mut R) -> Result<Vec<Resource>, String> {
    use std::hash::{Hash, Hasher};
    let mut out = Vec::new();
    for (key, offset, size) in entries(r)? {
        let mut bytes = vec![0u8; size as usize];
        r.seek(SeekFrom::Start(offset.into())).map_err(|e| e.to_string())?;
        r.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        out.push((key, h.finish()));
    }
    out.sort_unstable();
    out.dedup_by_key(|(k, _)| *k);
    Ok(out)
}

/// The index: each resource's key, and where its bytes are and how many.
fn entries<R: Read + Seek>(r: &mut R) -> Result<Vec<(Key, u32, u32)>, String> {
    let mut header = [0u8; 96];
    r.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..4] != b"DBPF" {
        return Err("not a DBPF package".into());
    }
    let count = u32_at(&header, 0x24).unwrap_or(0) as usize;
    let size = u32_at(&header, 0x2c).unwrap_or(0) as usize;
    let offset = u32_at(&header, 0x40).unwrap_or(0) as u64;
    if count == 0 {
        return Ok(Vec::new());
    }
    // Four megabytes hold 150 thousand entries, far more than the game's largest package.
    if !(4..=4 << 20).contains(&size) {
        return Err(format!("an index of {size} bytes"));
    }
    let mut index = vec![0u8; size];
    r.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    r.read_exact(&mut index).map_err(|e| e.to_string())?;
    let flags = u32_at(&index, 0).ok_or("an empty index")?;
    let mut at = 4;
    let mut constant = |bit: u32| -> Option<u32> {
        (flags & bit != 0).then(|| {
            let v = u32_at(&index, at);
            at += 4;
            v.unwrap_or(0)
        })
    };
    let (ty, group, unknown) = (constant(1), constant(2), constant(4));
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut next = || {
            let v = u32_at(&index, at);
            at += 4;
            v
        };
        let t = match ty {
            Some(t) => t,
            None => next().ok_or("a short index")?,
        };
        let g = match group {
            Some(g) => g,
            None => next().ok_or("a short index")?,
        };
        if unknown.is_none() {
            next().ok_or("a short index")?;
        }
        let i = next().ok_or("a short index")?;
        let offset = next().ok_or("a short index")?;
        // The size as stored, its top bit a flag; then the size unpacked and two short flags.
        let size = next().ok_or("a short index")? & 0x7fff_ffff;
        at += 8;
        out.push(((t, g, i), offset, size));
    }
    if at > index.len() {
        return Err("a short index".into());
    }
    Ok(out)
}

pub fn keys_of_file(path: &Path) -> Result<Vec<Key>, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    keys(&mut f)
}

/// The resources of a downloaded mod: a `.package`, or every `.package` inside a `.sporemod`.
pub fn resources_of_mod(path: &Path) -> Result<Vec<Resource>, String> {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    if lower.ends_with(".package") {
        let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        return resources(&mut f);
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut item = zip.by_index(i).map_err(|e| e.to_string())?;
        if !item.name().to_ascii_lowercase().ends_with(".package") {
            continue;
        }
        let mut bytes = Vec::with_capacity(item.size() as usize);
        item.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        out.extend(resources(&mut std::io::Cursor::new(bytes))?);
    }
    out.sort_unstable();
    out.dedup_by_key(|(k, _)| *k);
    Ok(out)
}

/// How many resources two sorted lists both carry with different bytes.
pub fn clashes(a: &[Resource], b: &[Resource]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].0.cmp(&b[j].0) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                n += usize::from(a[i].1 != b[j].1);
                i += 1;
                j += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A package as Spore writes it: the type and group of each entry given, the unknown word
    /// shared by all.
    fn package(entries: &[Key]) -> Vec<u8> {
        let mut index = vec![];
        index.extend(4u32.to_le_bytes());
        index.extend(0u32.to_le_bytes());
        for (n, (t, g, i)) in entries.iter().enumerate() {
            for v in [*t, *g, *i, 0, n as u32 % 2, 0] {
                index.extend(v.to_le_bytes());
            }
            index.extend([0u8; 4]);
        }
        let mut b = vec![0u8; 96];
        b[..4].copy_from_slice(b"DBPF");
        b[4..8].copy_from_slice(&3u32.to_le_bytes());
        b[0x24..0x28].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        b[0x2c..0x30].copy_from_slice(&(index.len() as u32).to_le_bytes());
        b[0x3c..0x40].copy_from_slice(&3u32.to_le_bytes());
        b[0x40..0x44].copy_from_slice(&96u32.to_le_bytes());
        b.extend(index);
        b
    }

    #[test]
    fn reads_the_index() {
        let p = package(&[(2, 1, 9), (1, 1, 5), (2, 1, 9)]);
        assert_eq!(keys(&mut std::io::Cursor::new(p.clone())).unwrap(), [(1, 1, 5), (2, 1, 9)]);
        let r = resources(&mut std::io::Cursor::new(p)).unwrap();
        assert_eq!(r.iter().map(|(k, _)| *k).collect::<Vec<_>>(), [(1, 1, 5), (2, 1, 9)]);
        assert_ne!(r[0].1, r[1].1);
        assert!(keys(&mut std::io::Cursor::new(b"PK\x03\x04".repeat(30))).is_err());
        let mut short = package(&[(1, 1, 1)]);
        short.truncate(110);
        assert!(keys(&mut std::io::Cursor::new(short)).is_err());
    }

    #[test]
    fn counts_resources_carried_differently() {
        let a = [((1, 1, 1), 7), ((1, 1, 2), 8), ((3, 0, 0), 9)];
        let b = [((1, 1, 2), 8), ((3, 0, 0), 1), ((4, 0, 0), 9)];
        assert_eq!(clashes(&a, &b), 1);
        assert_eq!(clashes(&[], &a), 0);
    }

    /// The game's own packages, when this computer has Spore.
    #[test]
    #[ignore]
    fn reads_the_games_packages() {
        for g in crate::game::find() {
            for dir in [g.spore_data, g.ga_data].into_iter().flatten() {
                for f in std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()) {
                    if f.extension().is_some_and(|x| x == "package") {
                        let k = keys_of_file(&f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
                        println!("{} {}", f.display(), k.len());
                    }
                }
            }
        }
    }
}
