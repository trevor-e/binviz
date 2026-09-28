#!/usr/bin/env python3
"""Writes tiny game ROMs for the tests: tiny.nes, tiny.gb, tiny.gba, tiny.md, tiny.sfc, tiny.z64
and tiny-psx.exe (a PlayStation executable).

Hand-assembled (a few dozen instructions each), with valid headers and
checksums: code that sets up the hardware through its registers, calls
subroutines, reads a table, and waits for interrupts, so that finding the
code from the vectors, naming the registers and banking can be tested.

    python3 roms.py <output folder>
"""

import sys
from pathlib import Path


class Asm:
    """Bytes at an address, with labels resolved once everything is placed."""

    def __init__(self, base):
        self.base = base
        self.code = bytearray()
        self.labels = {}
        self.fixups = []

    def pc(self):
        return self.base + len(self.code)

    def label(self, name):
        self.labels[name] = self.pc()

    def b(self, *bs):
        self.code += bytes(bs)

    def ref(self, op, label, kind):
        if isinstance(op, int):
            op = [op]
        self.code += bytes(op)
        self.fixups.append((len(self.code), label, kind))
        self.code += bytes({"abs16": 2, "rel8": 1, "long24": 3}[kind])

    def resolve(self, extra=None):
        labels = dict(self.labels, **(extra or {}))
        for at, label, kind in self.fixups:
            t = labels[label]
            if kind == "abs16":
                self.code[at:at + 2] = (t & 0xFFFF).to_bytes(2, "little")
            elif kind == "long24":
                self.code[at:at + 3] = (t & 0xFFFFFF).to_bytes(3, "little")
            else:
                off = t - (self.base + at + 1)
                assert -128 <= off <= 127, (label, off)
                self.code[at] = off & 0xFF
        return self.code


def nes():
    """UxROM (mapper 2): four 16 KiB PRG banks, the last fixed at $C000, one CHR bank."""
    fixed = Asm(0xC000)
    a = fixed
    a.label("reset")
    a.b(0x78)                      # sei
    a.b(0xD8)                      # cld
    a.b(0xA2, 0xFF)                # ldx #$FF
    a.b(0x9A)                      # txs
    a.b(0xA9, 0x00)                # lda #$00
    a.b(0x8D, 0x00, 0x20)          # sta PPUCTRL
    a.b(0x8D, 0x01, 0x20)          # sta PPUMASK
    a.ref(0x20, "wait_vblank", "abs16")    # jsr wait_vblank
    a.ref(0x20, "load_palette", "abs16")   # jsr load_palette
    a.b(0xA9, 0x01)                # lda #1
    a.b(0x8D, 0x00, 0x80)          # sta $8000: switch bank 1 in
    a.b(0x20, 0x00, 0x80)          # jsr $8000: which bank is there isn't known here
    a.b(0xA9, 0x80)                # lda #$80
    a.b(0x8D, 0x00, 0x20)          # sta PPUCTRL: NMI on
    a.label("forever")
    a.ref(0x4C, "forever", "abs16")        # jmp forever
    a.label("wait_vblank")
    a.b(0x2C, 0x02, 0x20)          # bit PPUSTATUS
    a.ref(0x10, "wait_vblank", "rel8")     # bpl wait_vblank
    a.b(0x60)                      # rts
    a.label("load_palette")
    a.b(0xA9, 0x3F, 0x8D, 0x06, 0x20)      # lda #$3F / sta PPUADDR
    a.b(0xA9, 0x00, 0x8D, 0x06, 0x20)      # lda #$00 / sta PPUADDR
    a.b(0xA2, 0x00)                # ldx #0
    a.label("palette_loop")
    a.ref(0xBD, "palette", "abs16")        # lda palette,x
    a.b(0x8D, 0x07, 0x20)          # sta PPUDATA
    a.b(0xE8)                      # inx
    a.b(0xE0, 0x04)                # cpx #4
    a.ref(0xD0, "palette_loop", "rel8")    # bne palette_loop
    a.b(0x60)                      # rts
    a.label("palette")
    a.b(0x0F, 0x16, 0x27, 0x30)
    a.label("nmi")
    a.b(0x48)                      # pha
    a.b(0xA9, 0x02, 0x8D, 0x14, 0x40)      # lda #$02 / sta OAMDMA
    a.b(0xE6, 0x10)                # inc $10: a frame counter
    a.b(0x68)                      # pla
    a.b(0x40)                      # rti
    a.label("irq")
    a.b(0x40)                      # rti
    fixed.resolve()
    last = bytearray(fixed.code) + bytes(0x4000 - 6 - len(fixed.code))
    for v in ("nmi", "reset", "irq"):
        last += fixed.labels[v].to_bytes(2, "little")
    # Bank 1: code that only runs once bank 1 is switched in, calling within its bank.
    b1 = Asm(0x8000)
    b1.b(0xA9, 0x3F)               # lda #$3F
    b1.ref(0x20, "inner", "abs16") # jsr inner
    b1.b(0x60)                     # rts
    b1.label("inner")
    b1.b(0x60)                     # rts
    bank1 = bytearray(b1.resolve()) + bytes(0x4000 - len(b1.code))
    # Bank 2: the game's text, as many NES games encode it: digits from $00,
    # letters from $0A, a space $24, $FF at the end of each string.
    def encode(text):
        out = bytearray()
        for c in text:
            if c.isdigit():
                out.append(ord(c) - ord("0"))
            elif c.isalpha():
                out.append(0x0A + ord(c) - ord("A"))
            else:
                out.append(0x24)
        return bytes(out) + bytes([0xFF])
    text = b"".join(encode(t) for t in ["PUSH START", "1 PLAYER GAME", "2 PLAYER GAME", "GAME OVER", "CONTINUE"])
    bank2 = text + bytes(0x4000 - len(text))
    prg = bytes(0x4000) + bank1 + bank2 + last
    # CHR: a smiley in tile 1 (two bit planes), the rest blank.
    smiley = [0x3C, 0x42, 0xA5, 0x81, 0xA5, 0x99, 0x42, 0x3C]
    chr_rom = bytearray(0x2000)
    chr_rom[16:24] = bytes(smiley)
    chr_rom[24:32] = bytes(smiley)
    header = b"NES\x1a" + bytes([4, 1, 0x21, 0x00]) + bytes(8)
    return header + prg + bytes(chr_rom)


LOGO = bytes.fromhex(
    "CEED6666CC0D000B03730083000C000D0008111F8889000EDCCC6EE6DDDDD999BBBB67636E0EECCCDDDC999FBBB9333E"
)


def gb():
    """32 KiB, no bank controller: bank 0 at $0000, bank 1 at $4000."""
    rom = bytearray(0x8000)
    # Interrupt vectors: V-blank counts frames, the others return.
    vblank = Asm(0x40)
    vblank.b(0xF5)                 # push af
    vblank.b(0xF0, 0x80)           # ldh a, [$FF80]
    vblank.b(0x3C)                 # inc a
    vblank.b(0xE0, 0x80)           # ldh [$FF80], a
    vblank.b(0xF1)                 # pop af
    vblank.b(0xD9)                 # reti
    rom[0x40:0x40 + len(vblank.code)] = vblank.resolve()
    for at in (0x48, 0x50, 0x58, 0x60):
        rom[at] = 0xD9             # reti
    # Entry: nop / jp $0150
    rom[0x100:0x104] = bytes([0x00, 0xC3, 0x50, 0x01])
    rom[0x104:0x134] = LOGO
    rom[0x134:0x13A] = b"BINVIZ"
    rom[0x147] = 0x00              # ROM only
    rom[0x148] = 0x00              # 32 KiB
    rom[0x149] = 0x00
    rom[0x14A] = 0x01
    main = Asm(0x150)
    a = main
    a.b(0xF3)                      # di
    a.b(0x31, 0xFE, 0xFF)          # ld sp, $FFFE
    a.ref(0xCD, "wait_vblank", "abs16")    # call wait_vblank
    a.b(0xAF)                      # xor a
    a.b(0xE0, 0x40)                # ldh [LCDC], a: screen off
    a.b(0x21, 0x00, 0xC0)          # ld hl, $C000
    a.b(0x3E, 0x12)                # ld a, $12
    a.b(0x22)                      # ld [hl+], a
    a.b(0xCD, 0x00, 0x40)          # call $4000: bank 1
    a.b(0x3E, 0x91)                # ld a, $91
    a.b(0xE0, 0x40)                # ldh [LCDC], a: screen on
    a.b(0x3E, 0x01)                # ld a, 1
    a.b(0xE0, 0xFF)                # ldh [IE], a: V-blank interrupts
    a.b(0xFB)                      # ei
    a.label("idle")
    a.b(0x76)                      # halt
    a.ref(0x18, "idle", "rel8")    # jr idle
    a.label("wait_vblank")
    a.b(0xF0, 0x44)                # ldh a, [LY]
    a.b(0xFE, 0x90)                # cp $90
    a.ref(0x20, "wait_vblank", "rel8")     # jr nz, wait_vblank
    a.b(0xC9)                      # ret
    rom[0x150:0x150 + len(main.code)] = main.resolve()
    # Bank 1, at $4000: bumps a counter in WRAM.
    bank1 = Asm(0x4000)
    bank1.b(0xFA, 0x00, 0xC0)      # ld a, [$C000]
    bank1.b(0x3C)                  # inc a
    bank1.b(0xEA, 0x00, 0xC0)      # ld [$C000], a
    bank1.b(0xC9)                  # ret
    rom[0x4000:0x4000 + len(bank1.code)] = bank1.resolve()
    check = 0
    for i in range(0x134, 0x14D):
        check = (check - rom[i] - 1) & 0xFF
    rom[0x14D] = check
    total = sum(rom) & 0xFFFF
    rom[0x14E:0x150] = total.to_bytes(2, "big")
    return bytes(rom)


def snes():
    """LoROM, 128 KiB (four 32 KiB banks), SlowROM."""
    rom = bytearray(0x20000)
    code = Asm(0x008000)
    a = code
    a.label("reset")
    a.b(0x78)                      # sei
    a.b(0x18)                      # clc
    a.b(0xFB)                      # xce: native mode
    a.b(0xC2, 0x30)                # rep #$30: 16-bit A and X
    a.b(0xA2, 0xFF, 0x1F)          # ldx #$1FFF
    a.b(0x9A)                      # txs
    a.b(0xE2, 0x20)                # sep #$20: 8-bit A
    a.b(0xA9, 0x8F)                # lda #$8F
    a.b(0x8D, 0x00, 0x21)          # sta INIDISP: force blank
    a.b(0x9C, 0x00, 0x42)          # stz NMITIMEN
    a.ref(0x22, "far", "long24")   # jsl far (bank $01)
    a.b(0xC2, 0x20)                # rep #$20
    a.b(0xA9, 0x34, 0x12)          # lda #$1234: 16 bits wide now
    a.b(0x8F, 0x10, 0x00, 0x7E)    # sta $7E0010
    a.ref(0x20, "clear", "abs16")  # jsr clear
    a.b(0xE2, 0x20)                # sep #$20
    a.b(0xA9, 0x81)                # lda #$81
    a.b(0x8D, 0x00, 0x42)          # sta NMITIMEN: NMI on
    a.label("forever")
    a.ref(0x80, "forever", "rel8") # bra forever
    a.label("clear")
    a.b(0xA2, 0x00, 0x00)          # ldx #$0000 (16-bit X)
    a.label("clear_loop")
    a.b(0x9E, 0x00, 0x01)          # stz $0100,x
    a.b(0xE8)                      # inx
    a.b(0xE0, 0x00, 0x01)          # cpx #$0100
    a.ref(0xD0, "clear_loop", "rel8")      # bne clear_loop
    a.b(0x60)                      # rts
    a.label("nmi")
    a.b(0xAD, 0x10, 0x42)          # lda RDNMI
    a.b(0x40)                      # rti
    code.resolve({"far": 0x018000})
    rom[0:len(code.code)] = code.code
    # Bank $01: a long subroutine.
    far = Asm(0x018000)
    far.b(0x08)                    # php
    far.b(0xE2, 0x20)              # sep #$20
    far.b(0xA9, 0x0F)              # lda #$0F
    far.b(0x8D, 0x00, 0x21)        # sta INIDISP
    far.b(0x28)                    # plp
    far.b(0x6B)                    # rtl
    rom[0x8000:0x8000 + len(far.code)] = far.resolve()
    # The internal header at $00:FFC0.
    h = 0x7FC0
    rom[h:h + 21] = b"BINVIZ TEST".ljust(21, b" ")
    rom[h + 0x15] = 0x20           # LoROM, SlowROM
    rom[h + 0x16] = 0x00           # ROM only
    rom[h + 0x17] = 0x07           # 128 KiB
    rom[h + 0x18] = 0x00
    rom[h + 0x19] = 0x01           # North America
    rom[h + 0x1A] = 0x33
    rom[h + 0x1B] = 0x00
    vectors = {0x2A: "nmi", 0x3C: "reset", 0x3A: "nmi"}
    for at, label in vectors.items():
        rom[h + at:h + at + 2] = (code.labels[label] & 0xFFFF).to_bytes(2, "little")
    # The checksum counts its own field as $0000 + $FFFF (complement + checksum).
    rom[h + 0x1C:h + 0x20] = bytes([0xFF, 0xFF, 0x00, 0x00])
    total = sum(rom) & 0xFFFF
    rom[h + 0x1C:h + 0x1E] = (total ^ 0xFFFF).to_bytes(2, "little")
    rom[h + 0x1E:h + 0x20] = total.to_bytes(2, "little")
    return bytes(rom)


class Mips:
    """MIPS words at an address; `j`/`jal`/branches take labels."""

    REGS = {n: i for i, n in enumerate(
        "zero at v0 v1 a0 a1 a2 a3 t0 t1 t2 t3 t4 t5 t6 t7 s0 s1 s2 s3 s4 s5 s6 s7 t8 t9 k0 k1 gp sp fp ra".split())}

    def __init__(self, base):
        self.base = base
        self.words = []
        self.labels = {}
        self.fixups = []

    def pc(self):
        return self.base + 4 * len(self.words)

    def label(self, name):
        self.labels[name] = self.pc()

    def r(self, name):
        return self.REGS[name]

    def i(self, op, rs, rt, imm):
        self.words.append(op << 26 | self.r(rs) << 21 | self.r(rt) << 16 | (imm & 0xFFFF))

    def lui(self, rt, imm):
        self.i(15, "zero", rt, imm)

    def addiu(self, rt, rs, imm):
        self.i(9, rs, rt, imm)

    def lw(self, rt, off, base):
        self.i(35, base, rt, off)

    def sw(self, rt, off, base):
        self.i(43, base, rt, off)

    def jr(self, rs):
        self.words.append(self.r(rs) << 21 | 8)

    def nop(self):
        self.words.append(0)

    def jump(self, op, label):
        self.fixups.append((len(self.words), label, op))
        self.words.append(0)

    def resolve(self, big):
        for at, label, op in self.fixups:
            t = self.labels[label]
            self.words[at] = op << 26 | ((t >> 2) & 0x03FFFFFF)
        order = "big" if big else "little"
        return b"".join(w.to_bytes(4, order) for w in self.words)


def n64():
    """A .z64 ROM: header, a boot code stub, and a little game code at 0x80000400."""
    rom = bytearray(0x1000 + 0x1000)
    rom[0:4] = bytes([0x80, 0x37, 0x12, 0x40])
    rom[4:8] = (0x0F).to_bytes(4, "big")
    rom[8:12] = (0x80000400).to_bytes(4, "big")
    rom[12:16] = (0x1449).to_bytes(4, "big")
    rom[0x10:0x18] = bytes.fromhex("0123456789ABCDEF")
    rom[0x20:0x34] = b"BINVIZ TEST".ljust(20, b" ")
    rom[0x3B:0x40] = b"NBZE\x00"
    # The boot code (IPL3) would copy the game and jump there; a loop here.
    ipl3 = Mips(0xA4000040)
    ipl3.label("spin")
    ipl3.jump(2, "spin")
    ipl3.nop()
    rom[0x40:0x40 + 8] = ipl3.resolve(True)
    a = Mips(0x80000400)
    a.label("entry")
    a.lui("sp", 0x8040)
    a.addiu("sp", "sp", -0x10)
    a.lui("t0", 0xA440)
    a.sw("zero", 0, "t0")                 # VI_STATUS
    a.lui("a0", 0x8000)
    a.jump(3, "init")                     # jal init
    a.addiu("a0", "a0", 0x500)            # (delay slot) an address
    a.label("loop")
    a.jump(2, "loop")                     # j loop
    a.nop()
    a.label("init")
    a.lui("at", 0x8000)
    a.lw("t1", 0x600, "at")               # a counter at 0x80000600
    a.addiu("t1", "t1", 1)
    a.sw("t1", 0x600, "at")
    a.jr("ra")
    a.nop()
    code = a.resolve(True)
    rom[0x1000:0x1000 + len(code)] = code
    return bytes(rom)


def psx():
    """A PS-X EXE: 2 KiB of header, then 2 KiB of code loaded at 0x80010000."""
    exe = bytearray(0x800 + 0x800)
    exe[0:8] = b"PS-X EXE"
    for at, v in [(0x10, 0x80010000), (0x14, 0x80018000), (0x18, 0x80010000), (0x1C, 0x800), (0x30, 0x801FFFF0)]:
        exe[at:at + 4] = v.to_bytes(4, "little")
    exe[0x4C:0x4C + 56] = b"Sony Computer Entertainment Inc. for North America area".ljust(56, bytes(1))
    a = Mips(0x80010000)
    a.label("entry")
    a.lui("t0", 0x1F80)
    a.sw("zero", 0x1074, "t0")            # I_MASK
    a.lw("v0", 0x1814, "t0")              # GP1 (the GPU's status)
    a.jump(3, "func")                     # jal func
    a.nop()
    a.addiu("t2", "zero", 0xA0)           # a BIOS call: jr $t2 with the function in $t1
    a.jr("t2")
    a.addiu("t1", "zero", 0x3F)           # (delay slot)
    a.label("func")
    a.lw("v0", -0x8000, "gp")             # $gp-relative: 0x80010000
    a.jr("ra")
    a.nop()
    code = a.resolve(False)
    exe[0x800:0x800 + len(code)] = code
    return bytes(exe)


def gba():
    """A cartridge: an ARM branch past the header, ARM start-up code, then a Thumb main."""
    rom = bytearray(0x400)

    def w32(at, v):
        rom[at:at + 4] = v.to_bytes(4, "little")

    def w16(at, v):
        rom[at:at + 2] = v.to_bytes(2, "little")

    w32(0x00, 0xEA000000 | ((0xC0 - 8) >> 2))  # b start
    rom[0xA0:0xAC] = b"BINVIZ TEST "
    rom[0xAC:0xB0] = b"BZTE"
    rom[0xB0:0xB2] = b"01"
    rom[0xB2] = 0x96
    check = 0
    for b in rom[0xA0:0xBD]:
        check = (check - b) & 0xFF
    rom[0xBD] = (check - 0x19) & 0xFF
    # ARM, at 0x080000C0: IRQ mode, a stack, the display on, then main in Thumb.
    for i, w in enumerate([
        0xE3A00012,  # mov r0, #0x12
        0xE129F000,  # msr cpsr_fc, r0
        0xE59FD018,  # ldr sp, [pc, #0x18]    = 0x03007F00
        0xE3A00301,  # mov r0, #0x04000000
        0xE3A01C01,  # mov r1, #0x100
        0xE1C010B0,  # strh r1, [r0]          DISPCNT
        0xE59F100C,  # ldr r1, [pc, #0xc]     = main + 1 (Thumb)
        0xE1A0E00F,  # mov lr, pc
        0xE12FFF11,  # bx r1
        0xEAFFFFFE,  # b . (forever)
        0x03007F00,  # the stack
        0x08000101,  # main, in Thumb
    ]):
        w32(0xC0 + 4 * i, w)
    # Thumb, at 0x08000100.
    for i, h in enumerate([
        0xB510,      # push {r4, lr}
        0x4802,      # ldr r0, [pc, #8]       = 0x04000130 (KEYINPUT)
        0x8801,      # ldrh r1, [r0]
        0xF000,      # bl sub (high half)
        0xF803,      # bl sub (low half)
        0xBD10,      # pop {r4, pc}
    ]):
        w16(0x100 + 2 * i, h)
    w32(0x10C, 0x04000130)
    w16(0x110, 0x2001)  # sub: mov r0, #1
    w16(0x112, 0x4770)  # bx lr
    return bytes(rom)


def megadrive():
    """A Mega Drive cartridge: vectors, header, and 68000 code that sets up the VDP."""
    rom = bytearray(0x400)

    def w16(at, v):
        rom[at:at + 2] = v.to_bytes(2, "big")

    def w32(at, v):
        rom[at:at + 4] = v.to_bytes(4, "big")

    w32(0, 0x00FFFE00)                 # initial stack
    w32(4, 0x200)                      # reset
    for i in range(2, 64):
        w32(4 * i, 0x240)              # every other exception: an error loop
    w32(4 * 30, 0x250)                 # V-blank (level 6)
    rom[0x100:0x110] = b"SEGA MEGA DRIVE "
    rom[0x110:0x120] = b"(C)BINV 2026.SEP"
    rom[0x120:0x150] = b"BINVIZ TEST".ljust(48, b" ")
    rom[0x150:0x180] = b"BINVIZ TEST".ljust(48, b" ")
    rom[0x180:0x18E] = b"GM 00000000-00"
    rom[0x190:0x1A0] = b"J".ljust(16, b" ")
    w32(0x1A0, 0)
    w32(0x1A4, len(rom) - 1)
    w32(0x1A8, 0xFF0000)
    w32(0x1AC, 0xFFFFFF)
    rom[0x1F0:0x1F3] = b"JUE"
    code = [
        # reset, at $200
        0x46FC, 0x2700,                # move.w #$2700,sr
        0x49F9, 0x00C0, 0x0004,        # lea ($C00004).l,a4
        0x38BC, 0x8144,                # move.w #$8144,(a4)   VDP register 1
        0x28BC, 0x4000, 0x0000,        # move.l #$40000000,(a4)
        0x6100, 0x0008,                # bsr.w clear ($21E)
        0x46FC, 0x2000,                # move.w #$2000,sr
        0x60FE,                        # bra.s * (forever)
        # clear, at $21E: RAM to zeros
        0x41F9, 0x00FF, 0x0000,        # lea ($FF0000).l,a0
        0x303C, 0x3FFF,                # move.w #$3FFF,d0
        0x4298,                        # clr.l (a0)+
        0x51C8, 0xFFFC,                # dbra d0,*-2
        0x4E75,                        # rts
    ]
    for i, w in enumerate(code):
        w16(0x200 + 2 * i, w)
    w16(0x240, 0x60FE)                 # the error loop: bra.s *
    for i, w in enumerate([0x5279, 0x00FF, 0x0010, 0x4E73]):
        w16(0x250 + 2 * i, w)          # V-blank: addq.w #1,($FF0010).l / rte
    total = 0
    for at in range(0x200, len(rom), 2):
        total = (total + (rom[at] << 8 | rom[at + 1])) & 0xFFFF
    w16(0x18E, total)
    return bytes(rom)


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    (out / "tiny.gba").write_bytes(gba())
    (out / "tiny.md").write_bytes(megadrive())
    (out / "tiny.nes").write_bytes(nes())
    (out / "tiny.gb").write_bytes(gb())
    (out / "tiny.sfc").write_bytes(snes())
    (out / "tiny.z64").write_bytes(n64())
    (out / "tiny-psx.exe").write_bytes(psx())


if __name__ == "__main__":
    main()
