ENTRY(_start)

PHDRS
{
  text PT_LOAD FLAGS(5);
  rodata PT_LOAD FLAGS(4);
  data PT_LOAD FLAGS(6);
}

SECTIONS
{
  . = 0x40000000;

  .text : ALIGN(0x1000)
  {
    *(.text._start)
    *(.text .text.*)
  } :text

  .rodata : ALIGN(0x1000)
  {
    *(.rodata .rodata.*)
  } :rodata

  .data : ALIGN(0x1000)
  {
    *(.data .data.*)
  } :data

  .bss : ALIGN(0x1000)
  {
    *(COMMON)
    *(.bss .bss.*)
  } :data

  /DISCARD/ :
  {
    *(.eh_frame .eh_frame.*)
    *(.note .note.*)
    *(.comment)
  }
}
