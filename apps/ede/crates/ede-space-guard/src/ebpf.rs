//! A small eBPF assembler (the instructions the guard's programs use, with
//! labels) and an interpreter of the same subset, so the gate runs the
//! exact bytes the guard loads on tables of inputs, without a kernel.

use std::collections::HashMap;

/// `BPF_FUNC_probe_read_kernel`.
pub const PROBE_READ_KERNEL: i32 = 113;

/// One instruction slot: `code`, `dst | src << 4`, `off`, `imm`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Insn {
    pub code: u8,
    pub dst: u8,
    pub src: u8,
    pub off: i16,
    pub imm: i32,
}

impl Insn {
    pub fn bytes(&self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0] = self.code;
        b[1] = (self.src << 4) | (self.dst & 0x0f);
        b[2..4].copy_from_slice(&self.off.to_le_bytes());
        b[4..8].copy_from_slice(&self.imm.to_le_bytes());
        b
    }

    pub fn parse(bytes: &[u8]) -> Vec<Insn> {
        bytes
            .chunks_exact(8)
            .map(|b| Insn { code: b[0], dst: b[1] & 0x0f, src: b[1] >> 4, off: i16::from_le_bytes([b[2], b[3]]), imm: i32::from_le_bytes([b[4], b[5], b[6], b[7]]) })
            .collect()
    }
}

/// Registers: r0 = return value, r1..r5 = arguments, r6..r9 kept across
/// calls, r10 = the frame pointer (read only).
pub type Reg = u8;
pub const R0: Reg = 0;
pub const R1: Reg = 1;
pub const R2: Reg = 2;
pub const R3: Reg = 3;
pub const R4: Reg = 4;
pub const R5: Reg = 5;
pub const R6: Reg = 6;
pub const R7: Reg = 7;
pub const R8: Reg = 8;
pub const R10: Reg = 10;

/// Conditional jumps (`BPF_JMP | op | BPF_K`, against an immediate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    Eq = 0x10,
    Gt = 0x20,
    Ge = 0x30,
    Set = 0x40,
    Ne = 0x50,
    Lt = 0xa0,
    Le = 0xb0,
}

/// Memory access sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    W = 0x00,
    H = 0x08,
    B = 0x10,
    Dw = 0x18,
}

impl Size {
    fn bytes(self) -> usize {
        match self {
            Size::W => 4,
            Size::H => 2,
            Size::B => 1,
            Size::Dw => 8,
        }
    }
}

const ALU64: u8 = 0x07;
const JMP: u8 = 0x05;
const LDX: u8 = 0x01;
const STX: u8 = 0x03;
const ST: u8 = 0x02;
const MEM: u8 = 0x60;
const K: u8 = 0x00;
const X: u8 = 0x08;

/// An assembler with forward labels.
#[derive(Default)]
pub struct Asm {
    insns: Vec<Insn>,
    labels: HashMap<String, usize>,
    fixups: Vec<(usize, String)>,
}

impl Asm {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, code: u8, dst: Reg, src: Reg, off: i16, imm: i32) -> &mut Self {
        self.insns.push(Insn { code, dst, src, off, imm });
        self
    }

    pub fn label(&mut self, name: &str) -> &mut Self {
        let at = self.insns.len();
        assert!(self.labels.insert(name.to_string(), at).is_none(), "label {name} twice");
        self
    }

    /// `dst = imm` (sign-extended to 64 bits).
    pub fn mov(&mut self, dst: Reg, imm: i32) -> &mut Self {
        self.push(ALU64 | 0xb0 | K, dst, 0, 0, imm)
    }

    /// `dst = src`.
    pub fn mov_reg(&mut self, dst: Reg, src: Reg) -> &mut Self {
        self.push(ALU64 | 0xb0 | X, dst, src, 0, 0)
    }

    pub fn add(&mut self, dst: Reg, imm: i32) -> &mut Self {
        self.push(ALU64 | K, dst, 0, 0, imm)
    }

    pub fn and(&mut self, dst: Reg, imm: i32) -> &mut Self {
        self.push(ALU64 | 0x50 | K, dst, 0, 0, imm)
    }

    pub fn and_reg(&mut self, dst: Reg, src: Reg) -> &mut Self {
        self.push(ALU64 | 0x50 | X, dst, src, 0, 0)
    }

    pub fn rsh(&mut self, dst: Reg, imm: i32) -> &mut Self {
        self.push(ALU64 | 0x70 | K, dst, 0, 0, imm)
    }

    /// `dst = imm64` (two slots).
    pub fn mov64(&mut self, dst: Reg, value: u64) -> &mut Self {
        self.push(0x18, dst, 0, 0, value as u32 as i32);
        self.push(0, 0, 0, 0, (value >> 32) as u32 as i32)
    }

    /// `dst = *(size *)(src + off)`.
    pub fn load(&mut self, size: Size, dst: Reg, src: Reg, off: i16) -> &mut Self {
        self.push(LDX | MEM | size as u8, dst, src, off, 0)
    }

    /// `*(size *)(dst + off) = src`.
    pub fn store(&mut self, size: Size, dst: Reg, off: i16, src: Reg) -> &mut Self {
        self.push(STX | MEM | size as u8, dst, src, off, 0)
    }

    /// `*(size *)(dst + off) = imm`.
    pub fn store_imm(&mut self, size: Size, dst: Reg, off: i16, imm: i32) -> &mut Self {
        self.push(ST | MEM | size as u8, dst, 0, off, imm)
    }

    /// `if dst <cond> imm goto label`.
    pub fn jump_if(&mut self, cond: Cond, dst: Reg, imm: i32, label: &str) -> &mut Self {
        self.fixups.push((self.insns.len(), label.to_string()));
        self.push(JMP | cond as u8 | K, dst, 0, 0, imm)
    }

    /// `if dst <cond> src goto label`.
    pub fn jump_if_reg(&mut self, cond: Cond, dst: Reg, src: Reg, label: &str) -> &mut Self {
        self.fixups.push((self.insns.len(), label.to_string()));
        self.push(JMP | cond as u8 | X, dst, src, 0, 0)
    }

    pub fn goto(&mut self, label: &str) -> &mut Self {
        self.fixups.push((self.insns.len(), label.to_string()));
        self.push(JMP, 0, 0, 0, 0)
    }

    pub fn call(&mut self, helper: i32) -> &mut Self {
        self.push(JMP | 0x80, 0, 0, 0, helper)
    }

    pub fn exit(&mut self) -> &mut Self {
        self.push(JMP | 0x90, 0, 0, 0, 0)
    }

    /// `r0 = value; exit`.
    pub fn ret(&mut self, value: i32) -> &mut Self {
        self.mov(R0, value).exit()
    }

    /// The program's bytes. Panics on an unknown label (a generator bug).
    pub fn build(&self) -> Vec<u8> {
        let mut insns = self.insns.clone();
        for (at, label) in &self.fixups {
            let target = *self.labels.get(label).unwrap_or_else(|| panic!("no label {label}"));
            let off = target as i64 - *at as i64 - 1;
            assert!(off >= 0, "backward jump to {label}: the guard's programs only jump forward");
            insns[*at].off = i16::try_from(off).expect("a jump within 32767 instructions");
        }
        insns.iter().flat_map(Insn::bytes).collect()
    }
}

/// A memory region the interpreter maps.
struct Region {
    base: u64,
    bytes: Vec<u8>,
    writable: bool,
}

/// The interpreter: runs a program of [`Asm`]'s subset on a context.
/// Every jump goes forward, so a program ends in at most its length.
pub struct Vm {
    regions: Vec<Region>,
}

/// Where the interpreter maps the stack (512 bytes below it), the context
/// and the kernel memory a test hands it.
pub const STACK_TOP: u64 = 0x7000_0000_0000;
pub const CTX: u64 = 0x1000_0000_0000;

impl Vm {
    /// `ctx` at [`CTX`]; `kernel` = other memory `bpf_probe_read_kernel`
    /// may read (address, bytes).
    pub fn new(ctx: Vec<u8>, kernel: Vec<(u64, Vec<u8>)>) -> Self {
        let mut regions = vec![Region { base: STACK_TOP - 512, bytes: vec![0xa5; 512], writable: true }, Region { base: CTX, bytes: ctx, writable: false }];
        regions.extend(kernel.into_iter().map(|(base, bytes)| Region { base, bytes, writable: false }));
        Self { regions }
    }

    fn region(&mut self, addr: u64, len: usize) -> Option<(&mut Region, usize)> {
        self.regions.iter_mut().find(|r| addr >= r.base && addr.checked_add(len as u64).is_some_and(|end| end <= r.base + r.bytes.len() as u64)).map(|r| {
            let at = (addr - r.base) as usize;
            (r, at)
        })
    }

    fn read(&mut self, addr: u64, len: usize) -> Result<u64, String> {
        let (region, at) = self.region(addr, len).ok_or_else(|| format!("read of {len} bytes at {addr:#x} outside every region"))?;
        let mut b = [0u8; 8];
        b[..len].copy_from_slice(&region.bytes[at..at + len]);
        Ok(u64::from_le_bytes(b))
    }

    fn write(&mut self, addr: u64, len: usize, value: u64) -> Result<(), String> {
        let (region, at) = self.region(addr, len).ok_or_else(|| format!("write of {len} bytes at {addr:#x} outside every region"))?;
        if !region.writable {
            return Err(format!("write at {addr:#x}: read-only region"));
        }
        region.bytes[at..at + len].copy_from_slice(&value.to_le_bytes()[..len]);
        Ok(())
    }

    /// Runs `program` with r1 = the context; its r0.
    pub fn run(&mut self, program: &[u8]) -> Result<u64, String> {
        let insns = Insn::parse(program);
        let mut r = [0u64; 11];
        r[1] = CTX;
        r[10] = STACK_TOP;
        let mut pc = 0usize;
        loop {
            let i = *insns.get(pc).ok_or("ran off the end")?;
            pc += 1;
            let (dst, src) = (i.dst as usize, i.src as usize);
            let imm = i.imm as i64 as u64;
            let operand = if i.code & X != 0 { r[src] } else { imm };
            match i.code & 0x07 {
                ALU64 => {
                    if dst == 10 {
                        return Err("r10 is read only".into());
                    }
                    r[dst] = match i.code & 0xf0 {
                        0x00 => r[dst].wrapping_add(operand),
                        0x10 => r[dst].wrapping_sub(operand),
                        0x40 => r[dst] | operand,
                        0x50 => r[dst] & operand,
                        0x60 => r[dst] << (operand & 63),
                        0x70 => r[dst] >> (operand & 63),
                        0xb0 => operand,
                        op => return Err(format!("alu op {op:#x}")),
                    };
                }
                JMP => {
                    let op = i.code & 0xf0;
                    let taken = match op {
                        0x00 => true,
                        0x80 => {
                            r[0] = self.helper(i.imm, &r)?;
                            // r1..r5 are clobbered by a call.
                            for reg in &mut r[1..=5] {
                                *reg = 0xdead_beef;
                            }
                            false
                        }
                        0x90 => return Ok(r[0]),
                        0x10 => r[dst] == operand,
                        0x20 => r[dst] > operand,
                        0x30 => r[dst] >= operand,
                        0x40 => r[dst] & operand != 0,
                        0x50 => r[dst] != operand,
                        0xa0 => r[dst] < operand,
                        0xb0 => r[dst] <= operand,
                        op => return Err(format!("jump op {op:#x}")),
                    };
                    if taken {
                        if i.off < 0 {
                            return Err("backward jump".into());
                        }
                        pc += i.off as usize;
                    }
                }
                LDX => {
                    let size = size_of(i.code)?;
                    r[dst] = self.read(r[src].wrapping_add(i.off as i64 as u64), size)?;
                }
                STX => {
                    let size = size_of(i.code)?;
                    self.write(r[dst].wrapping_add(i.off as i64 as u64), size, r[src])?;
                }
                ST => {
                    let size = size_of(i.code)?;
                    self.write(r[dst].wrapping_add(i.off as i64 as u64), size, imm)?;
                }
                0x00 if i.code == 0x18 => {
                    let high = insns.get(pc).ok_or("half an imm64")?;
                    pc += 1;
                    r[dst] = (i.imm as u32 as u64) | ((high.imm as u32 as u64) << 32);
                }
                _ => return Err(format!("instruction {:#x}", i.code)),
            }
        }
    }

    fn helper(&mut self, id: i32, r: &[u64; 11]) -> Result<u64, String> {
        match id {
            PROBE_READ_KERNEL => {
                let (dst, len, src) = (r[1], r[2] as usize, r[3]);
                // The kernel zeroes the destination when the read fails.
                match self.region(src, len).map(|(region, at)| region.bytes[at..at + len].to_vec()) {
                    Some(bytes) => {
                        let (region, at) = self.region(dst, len).ok_or("probe_read_kernel into unmapped memory")?;
                        region.bytes[at..at + len].copy_from_slice(&bytes);
                        Ok(0)
                    }
                    None => {
                        let (region, at) = self.region(dst, len).ok_or("probe_read_kernel into unmapped memory")?;
                        region.bytes[at..at + len].fill(0);
                        Ok((-libc::EFAULT) as i64 as u64)
                    }
                }
            }
            id => Err(format!("helper {id}")),
        }
    }
}

fn size_of(code: u8) -> Result<usize, String> {
    Ok(match code & 0x18 {
        0x00 => Size::W.bytes(),
        0x08 => Size::H.bytes(),
        0x10 => Size::B.bytes(),
        _ => Size::Dw.bytes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_encode_as_the_kernel_reads_them() {
        // `r2 = *(u32 *)(r1 + 4)`: code 0x61, dst 2, src 1, off 4.
        let mut asm = Asm::new();
        asm.load(Size::W, R2, R1, 4).mov64(R3, 0x1122_3344_5566_7788).ret(1);
        let bytes = asm.build();
        assert_eq!(&bytes[..8], &[0x61, 0x12, 4, 0, 0, 0, 0, 0]);
        assert_eq!(&bytes[8..16], &[0x18, 0x03, 0, 0, 0x88, 0x77, 0x66, 0x55]);
        assert_eq!(&bytes[16..24], &[0, 0, 0, 0, 0x44, 0x33, 0x22, 0x11]);
        assert_eq!(&bytes[24..], &[0xb7, 0x00, 0, 0, 1, 0, 0, 0, 0x95, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn labels_resolve_forward_and_the_interpreter_follows_them() {
        let mut asm = Asm::new();
        asm.load(Size::W, R2, R1, 0).jump_if(Cond::Eq, R2, 7, "seven").ret(0).label("seven").ret(1);
        let program = asm.build();
        assert_eq!(Vm::new(7u32.to_le_bytes().to_vec(), vec![]).run(&program), Ok(1));
        assert_eq!(Vm::new(8u32.to_le_bytes().to_vec(), vec![]).run(&program), Ok(0));
    }

    #[test]
    fn the_interpreter_refuses_what_the_verifier_would() {
        let mut asm = Asm::new();
        asm.load(Size::Dw, R2, R1, 64).ret(0);
        assert!(Vm::new(vec![0; 8], vec![]).run(&asm.build()).unwrap_err().contains("outside every region"));
        let mut asm = Asm::new();
        asm.store_imm(Size::W, R1, 0, 1).ret(0);
        assert!(Vm::new(vec![0; 8], vec![]).run(&asm.build()).unwrap_err().contains("read-only"));
    }

    #[test]
    fn probe_read_kernel_copies_or_zeroes() {
        let mut asm = Asm::new();
        asm.mov_reg(R1, R10).add(R1, -8).mov(R2, 8).mov64(R3, 0x5000).call(PROBE_READ_KERNEL).mov_reg(R6, R0).load(Size::Dw, R0, R10, -8).exit();
        let program = asm.build();
        assert_eq!(Vm::new(vec![], vec![(0x5000, 42u64.to_le_bytes().to_vec())]).run(&program), Ok(42));
        assert_eq!(Vm::new(vec![], vec![]).run(&program), Ok(0));
    }
}
