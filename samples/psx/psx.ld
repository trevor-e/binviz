/* A PlayStation executable's layout: code and data from 0x80010000, as the
   BIOS loads a PS-X EXE. */
ENTRY(main)
SECTIONS
{
  . = 0x80010000;
  .text : { *(.text .text.*) }
  .rodata : ALIGN(16) { *(.rodata .rodata.*) }
  .data : ALIGN(16) { *(.data .data.*) }
  .sdata : ALIGN(16) { *(.sdata .sdata.*) }
  __data_end = .;
  .sbss : ALIGN(16) { *(.sbss .sbss.*) }
  .bss : ALIGN(16) { *(.bss .bss.* COMMON) }
  __bss_end = .;
  /DISCARD/ : { *(.MIPS.abiflags .reginfo .comment .note.* .gnu.*) }
}
