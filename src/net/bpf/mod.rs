//! eBPF interpreter (mirrors Linux `net/bpf/`)

pub struct BpfInstruction {
    pub code: u8,
    pub dst: u8,
    pub src: u8,
    pub off: i16,
    pub imm: i32,
}

pub fn execute_bpf(packet: &[u8], prog: &[BpfInstruction]) -> u64 {
    let mut regs = [0u64; 11];
    regs[1] = packet.as_ptr() as u64;
    regs[2] = packet.len() as u64;

    for inst in prog {
        match inst.code {
            0xB7 => regs[inst.dst as usize] = inst.imm as u64,
            0x07 => regs[inst.dst as usize] = regs[inst.dst as usize].wrapping_add(inst.imm as u64),
            0x95 => return regs[0],
            _ => {}
        }
    }
    0
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("bpf: net interpreter initialized");
    Ok(())
}
