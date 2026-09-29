/* An overlay loaded at 0x80100000: a flat image the game reads from the
   disc into RAM, with no header. Its calls into the boot executable are
   resolved against that executable's symbols (--just-symbols). */
ENTRY(field_main)
SECTIONS
{
  . = 0x80100000;
  .text : { *(.text .text.*) }
  .rodata : ALIGN(16) { *(.rodata .rodata.*) }
  .data : ALIGN(16) { *(.data .data.*) }
  .bss : ALIGN(16) { *(.bss .bss.* .sbss .sbss.* COMMON) }
  /DISCARD/ : { *(.MIPS.abiflags .reginfo .comment .note.* .gnu.*) }
}
