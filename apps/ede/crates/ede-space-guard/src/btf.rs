//! Reads the kernel's BTF (`/sys/kernel/btf/vmlinux`) for the one layout
//! the connect program depends on: where `struct bpf_sock_addr_kern`
//! keeps the caller's address and its length. A kernel whose layout
//! differs gets a program for its own layout, or none.

/// Byte offsets of the members `name` of struct `strukt`, in that order.
pub fn member_offsets(btf: &[u8], strukt: &str, names: &[&str]) -> Result<Vec<u32>, String> {
    let u32_at = |at: usize| -> Result<u32, String> { btf.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| "BTF ends early".to_string()) };
    if btf.len() < 24 || u16::from_le_bytes([btf[0], btf[1]]) != 0xeb9f {
        return Err("not BTF (or not little-endian)".into());
    }
    let hdr_len = u32_at(4)? as usize;
    let (type_off, type_len, str_off, str_len) = (u32_at(8)? as usize, u32_at(12)? as usize, u32_at(16)? as usize, u32_at(20)? as usize);
    let types = hdr_len + type_off..hdr_len + type_off + type_len;
    let strings = btf.get(hdr_len + str_off..hdr_len + str_off + str_len).ok_or("BTF strings out of range")?;
    let name = |off: u32| -> &[u8] {
        let rest = strings.get(off as usize..).unwrap_or_default();
        &rest[..rest.iter().position(|b| *b == 0).unwrap_or(rest.len())]
    };
    let mut at = types.start;
    while at < types.end {
        let (name_off, info) = (u32_at(at)?, u32_at(at + 4)?);
        let (vlen, kind, kind_flag) = ((info & 0xffff) as usize, (info >> 24) & 0x1f, info >> 31 == 1);
        let body = at + 12;
        let extra = match kind {
            1 | 14 | 17 => 4,                 // INT, VAR, DECL_TAG
            3 => 12,                          // ARRAY
            4 | 5 => 12 * vlen,               // STRUCT, UNION: members
            6 | 13 => 8 * vlen,               // ENUM, FUNC_PROTO
            15 | 19 => 12 * vlen,             // DATASEC, ENUM64
            2 | 7..=12 | 16 | 18 => 0,        // PTR, FWD, TYPEDEF, qualifiers, FUNC, FLOAT, TYPE_TAG
            kind => return Err(format!("BTF kind {kind} unknown")),
        };
        if kind == 4 && name(name_off) == strukt.as_bytes() {
            let members: Vec<(&[u8], u32)> = (0..vlen)
                .map(|m| {
                    let at = body + 12 * m;
                    let offset = u32_at(at + 8)?;
                    let bits = if kind_flag { offset & 0x00ff_ffff } else { offset };
                    Ok((name(u32_at(at)?), bits))
                })
                .collect::<Result<_, String>>()?;
            return names
                .iter()
                .map(|want| {
                    let bits = members.iter().find(|(n, _)| *n == want.as_bytes()).map(|(_, bits)| *bits).ok_or_else(|| format!("{strukt} has no {want}"))?;
                    if bits % 8 != 0 {
                        return Err(format!("{strukt}.{want} is a bit field"));
                    }
                    Ok(bits / 8)
                })
                .collect();
        }
        at = body + extra;
    }
    Err(format!("no struct {strukt} in the kernel's BTF"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A BTF blob with an int and `struct name { members }` (byte offsets).
    pub(crate) fn blob(name: &str, members: &[(&str, u32)]) -> Vec<u8> {
        let mut strings = vec![0u8];
        let mut add = |s: &str| {
            let off = strings.len() as u32;
            strings.extend(s.as_bytes());
            strings.push(0);
            off
        };
        let int_name = add("int");
        let struct_name = add(name);
        let member_names: Vec<u32> = members.iter().map(|(n, _)| add(n)).collect();
        let mut types = Vec::new();
        let mut push = |v: u32| types.extend(v.to_le_bytes());
        // [1] int: name, info (kind 1), size 4, encoding.
        push(int_name);
        push(1 << 24);
        push(4);
        push(32);
        // [2] the struct.
        push(struct_name);
        push(4 << 24 | members.len() as u32);
        push(64);
        for (n, (_, offset)) in member_names.iter().zip(members) {
            push(*n);
            push(1);
            push(offset * 8);
        }
        let mut btf = Vec::new();
        btf.extend(0xeb9fu16.to_le_bytes());
        btf.extend([1u8, 0]);
        btf.extend(24u32.to_le_bytes());
        btf.extend(0u32.to_le_bytes());
        btf.extend((types.len() as u32).to_le_bytes());
        btf.extend((types.len() as u32).to_le_bytes());
        btf.extend((strings.len() as u32).to_le_bytes());
        btf.extend(types);
        btf.extend(strings);
        btf
    }

    #[test]
    fn the_offsets_of_a_structs_members() {
        let btf = blob("bpf_sock_addr_kern", &[("sk", 0), ("uaddr", 8), ("tmp_reg", 16), ("t_ctx", 24), ("uaddrlen", 32)]);
        assert_eq!(member_offsets(&btf, "bpf_sock_addr_kern", &["uaddr", "uaddrlen"]), Ok(vec![8, 32]));
        assert!(member_offsets(&btf, "bpf_sock_addr_kern", &["nope"]).unwrap_err().contains("has no nope"));
        assert!(member_offsets(&btf, "other", &["uaddr"]).unwrap_err().contains("no struct other"));
        assert!(member_offsets(&btf[..30], "bpf_sock_addr_kern", &["uaddr"]).is_err());
        assert!(member_offsets(b"not btf at all, not at all", "x", &["y"]).is_err());
    }

    /// This machine's kernel, where it has BTF.
    #[test]
    fn this_kernels_bpf_sock_addr_kern() {
        let Ok(btf) = std::fs::read("/sys/kernel/btf/vmlinux") else { return };
        let offsets = member_offsets(&btf, "bpf_sock_addr_kern", &["uaddr", "uaddrlen"]);
        // `uaddrlen` exists from Linux 6.7 on, with the unix hooks.
        if let Ok(offsets) = offsets {
            assert_eq!(offsets[0], 8, "{offsets:?}");
        }
    }
}
