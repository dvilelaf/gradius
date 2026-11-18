//! A small, self-contained 6502 disassembler for the oracle's PRG window.
//!
//! The ROM is a 32 KiB mapper-3 image mapped at `$8000-$FFFF`; `Image` reads
//! through that window. The opcode table covers the documented instruction set
//! plus the common undocumented ("illegal") opcodes that disassemblers need to
//! stay in sync on real code and data tables. It is deliberately data-only: no
//! cartridge bytes are printed except through the disassembly listing the user
//! asks for.

use std::collections::{BTreeMap, BTreeSet};

/// 6502 addressing modes, including the two accumulator/implicit forms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// No operand (`RTS`).
    Imp,
    /// Accumulator (`ASL A`, usually written without the `A`).
    Acc,
    /// Immediate (`LDA #$12`).
    Imm,
    /// Zero page (`LDA $12`).
    Zp,
    /// Zero page, X (`LDA $12,X`).
    ZpX,
    /// Zero page, Y (`LDX $12,Y`).
    ZpY,
    /// Absolute (`LDA $1234`).
    Abs,
    /// Absolute, X (`LDA $1234,X`).
    AbsX,
    /// Absolute, Y (`LDA $1234,Y`).
    AbsY,
    /// Indirect, only `JMP ($1234)`.
    Ind,
    /// Indexed indirect (`LDA ($12,X)`).
    IndX,
    /// Indirect indexed (`LDA ($12),Y`).
    IndY,
    /// Relative branch (`BNE $8010`).
    Rel,
}

impl Mode {
    pub const fn operand_len(self) -> usize {
        match self {
            Mode::Imp | Mode::Acc => 0,
            Mode::Imm | Mode::Zp | Mode::ZpX | Mode::ZpY | Mode::IndX | Mode::IndY | Mode::Rel => 1,
            Mode::Abs | Mode::AbsX | Mode::AbsY | Mode::Ind => 2,
        }
    }
}

/// One opcode-table entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Op {
    pub mnemonic: &'static str,
    pub mode: Mode,
}

macro_rules! ops {
    ($(($mnemonic:literal, $mode:ident)),* $(,)?) => {
        [$(Op { mnemonic: $mnemonic, mode: Mode::$mode }),*]
    };
}

/// The complete opcode table, one entry per byte.
///
/// Documented opcodes first; every other byte is one of the stable
/// undocumented operations (`SLO`, `RLA`, `SRE`, `RRA`, `SAX`, `LAX`, `DCP`,
/// `ISC`, `ANC`, `ALR`, `ARR`, `SBX`, `LAS`, `AHX`, `SHX`, `SHY`, `TAS`,
/// `XAA`), a multi-byte `NOP`, or `JAM` (which locks the CPU).
#[rustfmt::skip]
pub const OPS: [Op; 256] = ops![
    // 0x00
    ("BRK", Imp), ("ORA", IndX), ("JAM", Imp), ("SLO", IndX),
    ("NOP", Zp), ("ORA", Zp), ("ASL", Zp), ("SLO", Zp),
    ("PHP", Imp), ("ORA", Imm), ("ASL", Acc), ("ANC", Imm),
    ("NOP", Abs), ("ORA", Abs), ("ASL", Abs), ("SLO", Abs),
    // 0x10
    ("BPL", Rel), ("ORA", IndY), ("JAM", Imp), ("SLO", IndY),
    ("NOP", ZpX), ("ORA", ZpX), ("ASL", ZpX), ("SLO", ZpX),
    ("CLC", Imp), ("ORA", AbsY), ("NOP", Imp), ("SLO", AbsY),
    ("NOP", AbsX), ("ORA", AbsX), ("ASL", AbsX), ("SLO", AbsX),
    // 0x20
    ("JSR", Abs), ("AND", IndX), ("JAM", Imp), ("RLA", IndX),
    ("BIT", Zp), ("AND", Zp), ("ROL", Zp), ("RLA", Zp),
    ("PLP", Imp), ("AND", Imm), ("ROL", Acc), ("ANC", Imm),
    ("BIT", Abs), ("AND", Abs), ("ROL", Abs), ("RLA", Abs),
    // 0x30
    ("BMI", Rel), ("AND", IndY), ("JAM", Imp), ("RLA", IndY),
    ("NOP", ZpX), ("AND", ZpX), ("ROL", ZpX), ("RLA", ZpX),
    ("SEC", Imp), ("AND", AbsY), ("NOP", Imp), ("RLA", AbsY),
    ("NOP", AbsX), ("AND", AbsX), ("ROL", AbsX), ("RLA", AbsX),
    // 0x40
    ("RTI", Imp), ("EOR", IndX), ("JAM", Imp), ("SRE", IndX),
    ("NOP", Zp), ("EOR", Zp), ("LSR", Zp), ("SRE", Zp),
    ("PHA", Imp), ("EOR", Imm), ("LSR", Acc), ("ALR", Imm),
    ("JMP", Abs), ("EOR", Abs), ("LSR", Abs), ("SRE", Abs),
    // 0x50
    ("BVC", Rel), ("EOR", IndY), ("JAM", Imp), ("SRE", IndY),
    ("NOP", ZpX), ("EOR", ZpX), ("LSR", ZpX), ("SRE", ZpX),
    ("CLI", Imp), ("EOR", AbsY), ("NOP", Imp), ("SRE", AbsY),
    ("NOP", AbsX), ("EOR", AbsX), ("LSR", AbsX), ("SRE", AbsX),
    // 0x60
    ("RTS", Imp), ("ADC", IndX), ("JAM", Imp), ("RRA", IndX),
    ("NOP", Zp), ("ADC", Zp), ("ROR", Zp), ("RRA", Zp),
    ("PLA", Imp), ("ADC", Imm), ("ROR", Acc), ("ARR", Imm),
    ("JMP", Ind), ("ADC", Abs), ("ROR", Abs), ("RRA", Abs),
    // 0x70
    ("BVS", Rel), ("ADC", IndY), ("JAM", Imp), ("RRA", IndY),
    ("NOP", ZpX), ("ADC", ZpX), ("ROR", ZpX), ("RRA", ZpX),
    ("SEI", Imp), ("ADC", AbsY), ("NOP", Imp), ("RRA", AbsY),
    ("NOP", AbsX), ("ADC", AbsX), ("ROR", AbsX), ("RRA", AbsX),
    // 0x80
    ("NOP", Imm), ("STA", IndX), ("NOP", Imm), ("SAX", IndX),
    ("STY", Zp), ("STA", Zp), ("STX", Zp), ("SAX", Zp),
    ("DEY", Imp), ("NOP", Imm), ("TXA", Imp), ("XAA", Imm),
    ("STY", Abs), ("STA", Abs), ("STX", Abs), ("SAX", Abs),
    // 0x90
    ("BCC", Rel), ("STA", IndY), ("JAM", Imp), ("AHX", IndY),
    ("STY", ZpX), ("STA", ZpX), ("STX", ZpY), ("SAX", ZpY),
    ("TYA", Imp), ("STA", AbsY), ("TXS", Imp), ("TAS", AbsY),
    ("SHY", AbsX), ("STA", AbsX), ("SHX", AbsY), ("AHX", AbsY),
    // 0xA0
    ("LDY", Imm), ("LDA", IndX), ("LDX", Imm), ("LAX", IndX),
    ("LDY", Zp), ("LDA", Zp), ("LDX", Zp), ("LAX", Zp),
    ("TAY", Imp), ("LDA", Imm), ("TAX", Imp), ("LAX", Imm),
    ("LDY", Abs), ("LDA", Abs), ("LDX", Abs), ("LAX", Abs),
    // 0xB0
    ("BCS", Rel), ("LDA", IndY), ("JAM", Imp), ("LAX", IndY),
    ("LDY", ZpX), ("LDA", ZpX), ("LDX", ZpY), ("LAX", ZpY),
    ("CLV", Imp), ("LDA", AbsY), ("TSX", Imp), ("LAS", AbsY),
    ("LDY", AbsX), ("LDA", AbsX), ("LDX", AbsY), ("LAX", AbsY),
    // 0xC0
    ("CPY", Imm), ("CMP", IndX), ("NOP", Imm), ("DCP", IndX),
    ("CPY", Zp), ("CMP", Zp), ("DEC", Zp), ("DCP", Zp),
    ("INY", Imp), ("CMP", Imm), ("DEX", Imp), ("SBX", Imm),
    ("CPY", Abs), ("CMP", Abs), ("DEC", Abs), ("DCP", Abs),
    // 0xD0
    ("BNE", Rel), ("CMP", IndY), ("JAM", Imp), ("DCP", IndY),
    ("NOP", ZpX), ("CMP", ZpX), ("DEC", ZpX), ("DCP", ZpX),
    ("CLD", Imp), ("CMP", AbsY), ("NOP", Imp), ("DCP", AbsY),
    ("NOP", AbsX), ("CMP", AbsX), ("DEC", AbsX), ("DCP", AbsX),
    // 0xE0
    ("CPX", Imm), ("SBC", IndX), ("NOP", Imm), ("ISC", IndX),
    ("CPX", Zp), ("SBC", Zp), ("INC", Zp), ("ISC", Zp),
    ("INX", Imp), ("SBC", Imm), ("NOP", Imp), ("SBC", Imm),
    ("CPX", Abs), ("SBC", Abs), ("INC", Abs), ("ISC", Abs),
    // 0xF0
    ("BEQ", Rel), ("SBC", IndY), ("JAM", Imp), ("ISC", IndY),
    ("NOP", ZpX), ("SBC", ZpX), ("INC", ZpX), ("ISC", ZpX),
    ("SED", Imp), ("SBC", AbsY), ("NOP", Imp), ("ISC", AbsY),
    ("NOP", AbsX), ("SBC", AbsX), ("INC", AbsX), ("ISC", AbsX),
];

/// The PRG window at `$8000-$FFFF`.
#[derive(Clone, Copy, Debug)]
pub struct Image<'a> {
    prg: &'a [u8],
}

impl<'a> Image<'a> {
    pub fn new(prg: &'a [u8]) -> Self {
        Self { prg }
    }

    pub fn contains(&self, addr: u16) -> bool {
        addr >= 0x8000
    }

    pub fn read(&self, addr: u16) -> Option<u8> {
        if !self.contains(addr) {
            return None;
        }
        self.prg.get(usize::from(addr - 0x8000)).copied()
    }

    /// Decode the instruction at `addr`, or `None` when it is outside PRG or
    /// the operand bytes run past the end of the image.
    pub fn decode(&self, addr: u16) -> Option<Insn> {
        let opcode = self.read(addr)?;
        let op = OPS[usize::from(opcode)];
        let len = 1 + op.mode.operand_len();
        let mut bytes = [0u8; 3];
        for (i, byte) in bytes.iter_mut().enumerate().take(len) {
            *byte = self.read(addr.wrapping_add(i as u16))?;
        }
        let operand = match len {
            2 => u16::from(bytes[1]),
            3 => u16::from_le_bytes([bytes[1], bytes[2]]),
            _ => 0,
        };
        Some(Insn {
            addr,
            opcode,
            op,
            len,
            operand,
            bytes,
        })
    }

    /// The three hardware vectors, read from `$FFFA`.
    pub fn vectors(&self) -> [u16; 3] {
        let word = |addr: u16| {
            u16::from(self.read(addr).unwrap_or(0))
                | (u16::from(self.read(addr.wrapping_add(1)).unwrap_or(0)) << 8)
        };
        [word(0xFFFA), word(0xFFFC), word(0xFFFE)]
    }
}

/// One decoded instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Insn {
    pub addr: u16,
    pub opcode: u8,
    pub op: Op,
    pub len: usize,
    /// Immediate byte, zero-page address, relative offset, or 16-bit address.
    pub operand: u16,
    pub bytes: [u8; 3],
}

impl Insn {
    pub fn mnemonic(&self) -> &'static str {
        self.op.mnemonic
    }

    #[allow(dead_code)]
    pub fn mode(&self) -> Mode {
        self.op.mode
    }

    /// Branch destination (`PC + 2 + offset`) for relative modes.
    pub fn branch_target(&self) -> Option<u16> {
        (self.op.mode == Mode::Rel).then(|| {
            self.addr
                .wrapping_add(2)
                .wrapping_add(self.operand as u8 as i8 as u16)
        })
    }

    /// Absolute operand for `JMP`/`JSR` and the indirect form.
    pub fn abs_target(&self) -> Option<u16> {
        matches!(self.op.mode, Mode::Abs | Mode::Ind).then_some(self.operand)
    }

    /// Any reference to a 16-bit address: absolute and indirect operands, plus
    /// branch destinations. Immediate and zero-page operands are not included.
    pub fn address_ref(&self) -> Option<u16> {
        match self.op.mode {
            Mode::Abs | Mode::AbsX | Mode::AbsY | Mode::Ind => Some(self.operand),
            Mode::Rel => self.branch_target(),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn is_branch(&self) -> bool {
        self.op.mode == Mode::Rel
    }

    /// Operand text, with `Lxxxx` labels for 16-bit targets when requested.
    #[allow(dead_code)]
    pub fn operand_text(&self, labels: Option<&BTreeSet<u16>>) -> String {
        self.operand_text_with(labels, None)
    }

    /// Like [`Insn::operand_text`], but prefer a symbol name at the target.
    pub fn operand_text_with(
        &self,
        labels: Option<&BTreeSet<u16>>,
        names: Option<&BTreeMap<u16, String>>,
    ) -> String {
        match self.op.mode {
            Mode::Imp => String::new(),
            Mode::Acc => "A".to_string(),
            Mode::Imm => format!("#${:02X}", self.operand),
            Mode::Zp => format!("${:02X}", self.operand),
            Mode::ZpX => format!("${:02X},X", self.operand),
            Mode::ZpY => format!("${:02X},Y", self.operand),
            Mode::Abs => self.address_text_with(self.operand, labels, names),
            Mode::AbsX => format!("{},X", self.address_text_with(self.operand, labels, names)),
            Mode::AbsY => format!("{},Y", self.address_text_with(self.operand, labels, names)),
            Mode::Ind => format!("({})", self.address_text_with(self.operand, labels, names)),
            Mode::IndX => format!("(${:02X},X)", self.operand),
            Mode::IndY => format!("(${:02X}),Y", self.operand),
            Mode::Rel => self.address_text_with(self.branch_target().unwrap_or(0), labels, names),
        }
    }

    fn address_text_with(
        &self,
        addr: u16,
        labels: Option<&BTreeSet<u16>>,
        names: Option<&BTreeMap<u16, String>>,
    ) -> String {
        if let Some(name) = names.and_then(|names| names.get(&addr)) {
            return name.clone();
        }
        match labels {
            Some(labels) if labels.contains(&addr) => format!("L{addr:04X}"),
            _ => format!("${addr:04X}"),
        }
    }

    /// Raw bytes as they appear in the listing (`4C 67 80`).
    pub fn byte_text(&self) -> String {
        self.bytes[..self.len]
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[allow(dead_code)]
    pub fn text(&self, labels: Option<&BTreeSet<u16>>) -> String {
        self.text_with(labels, None)
    }

    /// Like [`Insn::text`], but resolve operands through symbol names first.
    pub fn text_with(
        &self,
        labels: Option<&BTreeSet<u16>>,
        names: Option<&BTreeMap<u16, String>>,
    ) -> String {
        let operand = self.operand_text_with(labels, names);
        if operand.is_empty() {
            self.mnemonic().to_string()
        } else {
            format!("{:<4} {operand}", self.mnemonic())
        }
    }
}

/// Result of a recursive-descent pass over the PRG window.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// Instruction starts that static flow reaches.
    pub code: BTreeMap<u16, Insn>,
    /// Every 16-bit address referenced by a decoded instruction, inside PRG.
    pub refs: BTreeSet<u16>,
    /// One flag per PRG byte claimed by an instruction (start or operand).
    claimed: Vec<bool>,
}

impl Analysis {
    pub fn contains(&self, addr: u16) -> bool {
        self.code.contains_key(&addr)
    }

    pub fn instruction_count(&self) -> usize {
        self.code.len()
    }

    /// Addresses that deserve a `Lxxxx` label: branch/jump destinations and
    /// 16-bit operands that point into PRG (including data references).
    pub fn labels(&self) -> BTreeSet<u16> {
        self.refs.clone()
    }
}

/// Follow static control flow from `seeds`.
///
/// Seeds should be the reset/NMI/IRQ vectors and, when a trace is available,
/// every executed PC: executed starts are ground truth, so they remain in the
/// map even where a linear sweep would desynchronise on data.
pub fn analyze(image: &Image, seeds: impl IntoIterator<Item = u16>) -> Analysis {
    analyze_with(image, seeds, None)
}

/// Like [`analyze`], but with a coverage trace.
///
/// When a trace is supplied, a `JSR` only queues its return site if that
/// address actually executed. Gradius dispatches game states with a `JSR` whose
/// return address is the byte after a 16-bit handler table: decoding the table
/// as code would desynchronise the listing, so the trace decides.
pub fn analyze_with(
    image: &Image,
    seeds: impl IntoIterator<Item = u16>,
    hits: Option<&[u32]>,
) -> Analysis {
    let mut analysis = Analysis {
        claimed: vec![false; image.len_hint()],
        ..Analysis::default()
    };
    let mut seeds: Vec<u16> = seeds
        .into_iter()
        .filter(|addr| image.contains(*addr))
        .collect();
    seeds.sort_unstable();
    seeds.dedup();
    let mut queue: std::collections::VecDeque<u16> = seeds.into();

    while let Some(addr) = queue.pop_front() {
        if !image.contains(addr) {
            continue;
        }
        let index = usize::from(addr - 0x8000);
        if index >= analysis.claimed.len() || analysis.claimed[index] {
            continue;
        }
        let Some(insn) = image.decode(addr) else {
            continue;
        };
        if index + insn.len > analysis.claimed.len() {
            continue;
        }
        queue_insn(&mut analysis, &mut queue, insn, hits);
    }
    analysis
}

fn queue_insn(
    analysis: &mut Analysis,
    queue: &mut std::collections::VecDeque<u16>,
    insn: Insn,
    hits: Option<&[u32]>,
) {
    let index = usize::from(insn.addr - 0x8000);
    // Refuse to start inside an instruction already claimed by static flow.
    if (1..insn.len).any(|offset| analysis.claimed[index + offset]) {
        return;
    }
    for offset in 0..insn.len {
        analysis.claimed[index + offset] = true;
    }
    analysis.code.insert(insn.addr, insn);
    if let Some(target) = insn.address_ref() {
        if target >= 0x8000 {
            analysis.refs.insert(target);
        }
    }

    let fallthrough = insn.addr.wrapping_add(insn.len as u16);
    match insn.opcode {
        // JSR: continue at the callee and, when the trace proves the call
        // returns, after the call too.
        0x20 => {
            if let Some(target) = insn.abs_target() {
                queue.push_back(target);
            }
            if hits.map_or(true, |hits| hits[usize::from(fallthrough)] > 0) {
                queue.push_back(fallthrough);
            }
        }
        // JMP absolute / indirect: only the destination.
        0x4C | 0x6C => {
            if let Some(target) = insn.abs_target() {
                queue.push_back(target);
            }
        }
        // RTI, RTS, BRK and JAM: no successor.
        0x40 | 0x60 | 0x00 => {}
        _ if insn.op.mnemonic == "JAM" => {}
        _ => {
            if let Some(target) = insn.branch_target() {
                queue.push_back(target);
            }
            queue.push_back(fallthrough);
        }
    }
}

/// Linear sweep, used when no trace is available.
pub fn linear(image: &Image, from: u16, to: u16) -> Vec<Insn> {
    let mut out = Vec::new();
    let mut addr = from.max(0x8000);
    while addr <= to {
        let Some(insn) = image.decode(addr) else {
            break;
        };
        let next = addr.wrapping_add(insn.len as u16);
        out.push(insn);
        if next <= addr {
            break;
        }
        addr = next;
    }
    out
}

/// Options for [`render`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Render<'a> {
    /// `Some` when a trace run produced coverage: enables executed markers and
    /// `; data` for bytes outside the statically decoded flow.
    pub hits: Option<&'a [u32]>,
    /// Resolve 16-bit operands to `Lxxxx` labels.
    pub labels: bool,
    /// Prefer these symbol names for operands and function-entry labels.
    pub names: Option<&'a BTreeMap<u16, String>>,
}

/// Render a listing for `from..=to`.
pub fn render(
    image: &Image,
    from: u16,
    to: u16,
    analysis: Option<&Analysis>,
    labels: &BTreeSet<u16>,
    options: Render,
) -> String {
    let mut out = String::new();
    let label_refs = options.labels.then_some(labels);
    match analysis {
        Some(analysis) => {
            // Iterate in `usize` so a listing that ends at `$FFFF` cannot wrap
            // around and repeat the whole PRG.
            let mut addr = usize::from(from.max(0x8000));
            let end = usize::from(to);
            while addr <= end {
                let at = addr as u16;
                if let Some(insn) = analysis.code.get(&at) {
                    if let Some(name) = options.names.and_then(|names| names.get(&at)) {
                        out.push_str(&format!("{name}:\n"));
                    } else if options.labels && labels.contains(&at) {
                        out.push_str(&format!("L{at:04X}:\n"));
                    }
                    out.push_str(&line(insn, label_refs, options.hits, options.names));
                    addr += insn.len;
                } else {
                    let start = at;
                    let mut bytes = Vec::new();
                    while addr <= end && !analysis.contains(addr as u16) {
                        if let Some(byte) = image.read(addr as u16) {
                            bytes.push(byte);
                        }
                        addr += 1;
                    }
                    out.push_str(&data_block(start, &bytes));
                }
            }
        }
        None => {
            for insn in linear(image, from, to) {
                if let Some(name) = options.names.and_then(|names| names.get(&insn.addr)) {
                    out.push_str(&format!("{name}:\n"));
                } else if options.labels && labels.contains(&insn.addr) {
                    out.push_str(&format!("L{:04X}:\n", insn.addr));
                }
                out.push_str(&line(&insn, label_refs, options.hits, options.names));
                if insn.addr >= to {
                    break;
                }
            }
        }
    }
    out
}

fn line(
    insn: &Insn,
    labels: Option<&BTreeSet<u16>>,
    hits: Option<&[u32]>,
    names: Option<&BTreeMap<u16, String>>,
) -> String {
    let executed = hits.map_or(0, |hits| hits[usize::from(insn.addr)]);
    let marker = if hits.is_some() && executed > 0 {
        '*'
    } else {
        ' '
    };
    let mut text = format!(
        "{marker}${:04X}  {:<9} {:<16}",
        insn.addr,
        insn.byte_text(),
        insn.text_with(labels, names)
    );
    if executed > 0 {
        text.push_str(&format!(" ; {executed}"));
    }
    while text.ends_with(' ') {
        text.pop();
    }
    text.push('\n');
    text
}

fn data_block(start: u16, bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut addr = start;
    for chunk in bytes.chunks(8) {
        let hex = chunk
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii = chunk
            .iter()
            .map(|byte| {
                if (0x20..0x7F).contains(byte) {
                    char::from(*byte)
                } else {
                    '.'
                }
            })
            .collect::<String>();
        out.push_str(&format!(" ${addr:04X}  ; data: {hex:<23} |{ascii}|\n"));
        addr = addr.wrapping_add(chunk.len() as u16);
    }
    out
}

impl<'a> Image<'a> {
    fn len_hint(&self) -> usize {
        self.prg.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(bytes: &[u8]) -> Image<'_> {
        Image::new(bytes)
    }

    #[test]
    fn decodes_documented_opcodes() {
        let prg = [
            0xA9, 0x42, // LDA #$42
            0x4C, 0x34, 0x12, // JMP $1234
            0x20, 0x50, 0x80, // JSR $8050
            0x6C, 0x00, 0x20, // JMP ($2000)
            0x91, 0x40, // STA ($40),Y
            0xEA, // NOP
        ];
        let image = image(&prg);
        let lda = image.decode(0x8000).unwrap();
        assert_eq!(lda.mnemonic(), "LDA");
        assert_eq!(lda.mode(), Mode::Imm);
        assert_eq!(lda.len, 2);
        assert_eq!(lda.operand_text(None), "#$42");
        assert_eq!(lda.text(None), "LDA  #$42");

        let jmp = image.decode(0x8002).unwrap();
        assert_eq!(jmp.abs_target(), Some(0x1234));
        assert_eq!(jmp.text(None), "JMP  $1234");

        let jsr = image.decode(0x8005).unwrap();
        assert_eq!(jsr.mnemonic(), "JSR");
        assert_eq!(jsr.abs_target(), Some(0x8050));

        let indirect = image.decode(0x8008).unwrap();
        assert_eq!(indirect.text(None), "JMP  ($2000)");
        assert_eq!(indirect.address_ref(), Some(0x2000));

        let sta = image.decode(0x800B).unwrap();
        assert_eq!(sta.text(None), "STA  ($40),Y");

        assert_eq!(image.decode(0x800D).unwrap().text(None), "NOP");
    }

    #[test]
    fn branch_targets_wrap_within_the_page() {
        // $8000: BNE $8000 (offset -2).
        let prg = [0xD0, 0xFE];
        let insn = image(&prg).decode(0x8000).unwrap();
        assert!(insn.is_branch());
        assert_eq!(insn.branch_target(), Some(0x8000));
        // Forwards by +4.
        let prg = [0xF0, 0x04];
        assert_eq!(
            image(&prg).decode(0x8000).unwrap().branch_target(),
            Some(0x8006)
        );
        // A branch at $80FE wraps into the next page.
        let mut prg = [0u8; 0x102];
        prg[0xFE] = 0xD0;
        prg[0xFF] = 0x02;
        assert_eq!(
            image(&prg).decode(0x80FE).unwrap().branch_target(),
            Some(0x8102)
        );
    }

    #[test]
    fn decodes_common_illegal_opcodes() {
        let cases: &[(u8, &str, Mode, usize)] = &[
            (0x03, "SLO", Mode::IndX, 2),
            (0x07, "SLO", Mode::Zp, 2),
            (0x1F, "SLO", Mode::AbsX, 3),
            (0x0B, "ANC", Mode::Imm, 2),
            (0xA7, "LAX", Mode::Zp, 2),
            (0x87, "SAX", Mode::Zp, 2),
            (0xC7, "DCP", Mode::Zp, 2),
            (0xE7, "ISC", Mode::Zp, 2),
            (0x93, "AHX", Mode::IndY, 2),
            (0x9C, "SHY", Mode::AbsX, 3),
            (0x9E, "SHX", Mode::AbsY, 3),
            (0xBB, "LAS", Mode::AbsY, 3),
            (0x02, "JAM", Mode::Imp, 1),
            (0x1A, "NOP", Mode::Imp, 1),
            (0x80, "NOP", Mode::Imm, 2),
            (0x1C, "NOP", Mode::AbsX, 3),
        ];
        for (opcode, mnemonic, mode, len) in cases {
            let prg = [*opcode, 0x12, 0x34];
            let insn = image(&prg).decode(0x8000).unwrap();
            assert_eq!(insn.mnemonic(), *mnemonic, "opcode {opcode:02X}");
            assert_eq!(insn.mode(), *mode, "opcode {opcode:02X}");
            assert_eq!(insn.len, *len, "opcode {opcode:02X}");
        }
    }

    #[test]
    fn every_opcode_has_a_sane_length_and_name() {
        for (opcode, op) in OPS.iter().enumerate() {
            assert!(!op.mnemonic.is_empty(), "opcode {opcode:02X}");
            assert!((3..=5).contains(&op.mnemonic.len()), "opcode {opcode:02X}");
            let len = 1 + op.mode.operand_len();
            assert!((1..=3).contains(&len), "opcode {opcode:02X}");
        }
        // The three documented JAM bytes are all `JAM`.
        assert_eq!(OPS[0x02].mnemonic, "JAM");
        assert_eq!(OPS[0x12].mnemonic, "JAM");
        assert_eq!(OPS[0xF2].mnemonic, "JAM");
    }

    #[test]
    fn labels_resolve_branch_and_absolute_targets() {
        let mut prg = vec![0u8; 0x100];
        prg[0] = 0xD0; // BNE $8006
        prg[1] = 0x04;
        prg[2] = 0x4C; // JMP $8009
        prg[3] = 0x09;
        prg[4] = 0x80;
        prg[5] = 0xEA;
        prg[6] = 0x60; // RTS
        prg[9] = 0x60;
        let image = image(&prg);
        let analysis = analyze(&image, [0x8000]);
        let labels = analysis.labels();
        let text = render(
            &image,
            0x8000,
            0x800A,
            Some(&analysis),
            &labels,
            Render {
                labels: true,
                ..Render::default()
            },
        );
        assert!(text.contains("BNE  L8006"), "{text}");
        assert!(text.contains("JMP  L8009"), "{text}");
    }

    #[test]
    fn symbol_names_label_entries_and_operands() {
        let mut prg = vec![0u8; 0x100];
        prg[0] = 0x4C; // JMP $8006
        prg[1] = 0x06;
        prg[2] = 0x80;
        prg[6] = 0x60; // RTS
        let image = image(&prg);
        let analysis = analyze(&image, [0x8000]);
        let mut names = BTreeMap::new();
        names.insert(0x8000u16, "entry_point".to_string());
        names.insert(0x8006u16, "target".to_string());
        let text = render(
            &image,
            0x8000,
            0x8006,
            Some(&analysis),
            &BTreeSet::new(),
            Render {
                names: Some(&names),
                ..Render::default()
            },
        );
        assert!(text.contains("entry_point:\n"), "{text}");
        assert!(text.contains("JMP  target"), "{text}");
        assert!(text.contains("target:\n"), "{text}");
    }

    #[test]
    fn covered_listing_marks_data_and_executed_instructions() {
        // JMP skips two data bytes; the target is a RTS.
        let mut prg = vec![0u8; 0x100];
        prg[0] = 0x4C;
        prg[1] = 0x05;
        prg[2] = 0x80;
        prg[3] = 0xFF;
        prg[4] = 0xFF;
        prg[5] = 0x60;
        let image = image(&prg);
        let analysis = analyze(&image, [0x8000]);
        assert_eq!(analysis.instruction_count(), 2);
        assert!(!analysis.contains(0x8003));

        let mut hits = vec![0u32; 0x10000];
        hits[0x8000] = 7;
        hits[0x8005] = 1;
        let text = render(
            &image,
            0x8000,
            0x8006,
            Some(&analysis),
            &BTreeSet::new(),
            Render {
                hits: Some(&hits),
                labels: false,
                names: None,
            },
        );
        assert!(text.contains("*$8000"), "{text}");
        assert!(text.contains("; 7"), "{text}");
        assert!(text.contains("; data: FF FF"), "{text}");
        assert!(
            text.contains(" *$8005") || text.contains("*$8005"),
            "{text}"
        );
    }

    #[test]
    fn traced_jsr_skips_an_unexecuted_return_site_but_keeps_branch_targets() {
        // $8000: JSR $8006. $8003-$8005: handler table (never executed).
        let mut prg = vec![0u8; 0x100];
        prg[0] = 0x20;
        prg[1] = 0x06;
        prg[2] = 0x80;
        prg[6] = 0x60; // RTS at the callee
        let img = image(&prg);
        let mut hits = vec![0u32; 0x10000];
        hits[0x8000] = 1;
        hits[0x8006] = 1;

        let untraced = analyze(&img, [0x8000]);
        assert!(
            untraced.contains(0x8003),
            "plain descent follows the JSR return"
        );

        let traced = analyze_with(&img, [0x8000, 0x8006], Some(&hits));
        assert!(traced.contains(0x8000));
        assert!(traced.contains(0x8006));
        assert!(
            !traced.contains(0x8003),
            "unexecuted return site stays data"
        );

        // A branch target is still decoded even when it was not executed.
        let mut prg = vec![0u8; 0x100];
        prg[0] = 0xF0; // BEQ $8006
        prg[1] = 0x04;
        prg[6] = 0x60;
        let img = image(&prg);
        let mut hits = vec![0u32; 0x10000];
        hits[0x8000] = 1;
        let traced = analyze_with(&img, [0x8000], Some(&hits));
        assert!(traced.contains(0x8006));
    }

    #[test]
    fn linear_listing_walks_the_whole_requested_range() {
        let mut prg = vec![0u8; 0x100];
        prg[0] = 0xA9;
        prg[1] = 0x01;
        prg[2] = 0x8D;
        prg[3] = 0x00;
        prg[4] = 0x03;
        let image = image(&prg);
        let insns = linear(&image, 0x8000, 0x8004);
        assert_eq!(insns.len(), 2);
        assert_eq!(insns[0].text(None), "LDA  #$01");
        assert_eq!(insns[1].text(None), "STA  $0300");
    }

    #[test]
    fn vectors_are_read_little_endian() {
        let mut prg = vec![0u8; 0x8000];
        prg[0x7FFA] = 0x6A;
        prg[0x7FFB] = 0x80;
        prg[0x7FFC] = 0x10;
        prg[0x7FFD] = 0x80;
        prg[0x7FFE] = 0xB5;
        prg[0x7FFF] = 0x80;
        assert_eq!(image(&prg).vectors(), [0x806A, 0x8010, 0x80B5]);
    }

    #[test]
    fn decode_past_the_end_returns_none() {
        let prg = [0xA9, 0x01];
        let image = image(&prg);
        // LDA #$01 needs one operand byte, and there is none at $8001.
        assert!(image.decode(0x8001).is_none());
        assert!(image.decode(0x7FFF).is_none());
    }
}
