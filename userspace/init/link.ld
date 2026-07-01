/* RustOS native userspace linker script.
 *
 * Produces a static ET_EXEC with a SINGLE read+execute PT_LOAD segment at a
 * fixed user virtual address inside the kernel's user range
 * [USER_SPACE_START=0x1000_0000, USER_SPACE_END=0x8000_0000).
 *
 * Keep the base above the low physical-memory identity map left by common
 * bootloader page tables. The kernel exec loader maps ET_EXEC at p_vaddr, so
 * linking at 0x1000_0000 can collide with a pre-existing 256 MiB identity
 * mapping before userspace has its own page tables.
 *
 * The kernel ELF loader enforces W^X, so we must NOT emit a writable+executable
 * segment.  This program therefore has no writable data/bss of its own; its only
 * writable memory is the kernel-provided stack.  Everything (.text/.rodata) lives
 * in one R-X segment whose runtime address equals its link address, so the loader
 * maps it at p_vaddr and absolute references resolve correctly.
 */

ENTRY(_start)

PHDRS {
    text PT_LOAD FLAGS(5); /* PF_R | PF_X */
}

SECTIONS {
    . = 0x40000000;

    .text : ALIGN(0x1000) {
        *(.text._start)
        *(.text .text.*)
        *(.rodata .rodata.*)
    } :text

    /DISCARD/ : {
        *(.data .data.* .bss .bss.* COMMON)
        *(.eh_frame .eh_frame_hdr)
        *(.comment)
        *(.note .note.*)
        *(.got .got.plt .plt .plt.*)
        *(.dynamic .dynsym .dynstr .hash .gnu.hash)
    }
}
