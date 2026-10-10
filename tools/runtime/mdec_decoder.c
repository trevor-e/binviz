/* SPDX-License-Identifier: MIT
 * Derived from allkern/psxe psx/dev/mdec.c at
 * 8459c63ed78c703f46e3c51daa65a2fdf96d02e6.
 * Copyright (c) 2024 Allkern (Lisandro Alarcon).
 * Permission and warranty terms: vendor/psxe/LICENSE (retained verbatim).
 * Changes: fixed storage; bounded two-pass RLE validation; no allocator or bus;
 * upper-13-bit scale/signed fractional stripping and signed-nine-bit color
 * clipping in documented mode. Original psxe math is retained for comparison.
 * This is a functional device candidate, not a recovered PsyQ game function.
 * The documented IDCT/color rules are approximate; silicon parity is unproven.
 */
typedef unsigned char uint8_t;
typedef unsigned short uint16_t;
typedef short int16_t;
typedef int int32_t;
typedef unsigned uint32_t;
#define CLAMP(v,l,h) ((v)<=(l)?(l):((v)>=(h)?(h):(v)))
#define MAX_BLOCKS 4096u
#define MAX_HALFWORDS 131070u
typedef struct {
 int output_signed,output_depth,output_bit15;
 int16_t crblk[64],cbblk[64],yblk[64];
} psx_mdec_t;
static uint16_t input[MAX_HALFWORDS];
static uint8_t output[MAX_BLOCKS*768u],quant_y[64],quant_uv[64];
static int16_t scales[64];
static uint32_t result[4];
static int floor_div(int n,int d) { return n>=0?n/d:-((-n+d-1)/d); }
static int signed9(int n) { n &= 511; return n>=256?n-512:n; }
static int signed10(int n) { n &= 1023; return n>=512?n-1024:n; }
int zagzig[] = {
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36, 
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63
};

void real_idct(int16_t* blk, int16_t* scale) {
    int16_t buf[64];

    int16_t* src = blk;
    int16_t* dst = buf;

    for (int pass = 0; pass < 2; pass++) {
        for (int x = 0; x < 8; x++) {
            for (int y = 0; y < 8; y++) {
                int sum = 0;

                for (int z = 0; z < 8; z++)
                    sum += (int32_t)src[y+z*8] * ((int32_t)scale[x+z*8] / 8);
                
                dst[x+y*8] = (sum + 0xfff) / 0x2000;
            }
        }

        int16_t* temp = src;

        src = dst;
        dst = temp;
    }
}

void yuv_to_rgb(psx_mdec_t* mdec, uint8_t* buf, int xx, int yy) {
    for (int y = 0; y < 8; y++) {
        for (int x = 0; x < 8; x++) {
            int16_t r = mdec->crblk[((x + xx) >> 1) + ((y + yy) >> 1) * 8];
            int16_t b = mdec->cbblk[((x + xx) >> 1) + ((y + yy) >> 1) * 8];
            int16_t g = (-0.3437 * (float)b) + (-0.7143 * (float)r);

            r = (1.402 * (float)r);
            b = (1.772 * (float)b);

            int16_t l = mdec->yblk[x + y * 8];

            r = CLAMP(l + r, -128, 127);
            g = CLAMP(l + g, -128, 127);
            b = CLAMP(l + b, -128, 127);

            if (!mdec->output_signed) {
                r ^= 0x80;
                g ^= 0x80;
                b ^= 0x80;
            }

            if (mdec->output_depth == 3) {
                uint16_t r5 = ((uint8_t)r) >> 3;
                uint16_t g5 = ((uint8_t)g) >> 3;
                uint16_t b5 = ((uint8_t)b) >> 3;

                uint16_t rgb = (b5 << 10) | (g5 << 5) | r5;

                if (mdec->output_bit15)
                    rgb |= 0x8000;
                
                buf[0 + ((x + xx) + (y + yy) * 16) * 2] = rgb & 0xff;
                buf[1 + ((x + xx) + (y + yy) * 16) * 2] = rgb >> 8;
            } else {
                buf[0 + ((x + xx) + (y + yy) * 16) * 3] = r & 0xff;
                buf[1 + ((x + xx) + (y + yy) * 16) * 3] = g & 0xff;
                buf[2 + ((x + xx) + (y + yy) * 16) * 3] = b & 0xff;
            }
        }
    }
}

void documented_idct(int16_t* blk, int16_t* scale) {
    int16_t buf[64];

    int16_t* src = blk;
    int16_t* dst = buf;

    for (int pass = 0; pass < 2; pass++) {
        for (int x = 0; x < 8; x++) {
            for (int y = 0; y < 8; y++) {
                int sum = 0;

                for (int z = 0; z < 8; z++)
                    sum += (int32_t)src[y+z*8] * floor_div((int32_t)scale[x+z*8], 8);
                
                dst[x+y*8] = floor_div(sum + 0xfff, 0x2000);
            }
        }

        int16_t* temp = src;

        src = dst;
        dst = temp;
    }
}

void documented_yuv_to_rgb(psx_mdec_t* mdec, uint8_t* buf, int xx, int yy) {
    for (int y = 0; y < 8; y++) {
        for (int x = 0; x < 8; x++) {
            int16_t r = mdec->crblk[((x + xx) >> 1) + ((y + yy) >> 1) * 8];
            int16_t b = mdec->cbblk[((x + xx) >> 1) + ((y + yy) >> 1) * 8];
            int16_t g = (-0.3437 * (float)b) + (-0.7143 * (float)r);

            r = (1.402 * (float)r);
            b = (1.772 * (float)b);

            int16_t l = mdec->yblk[x + y * 8];

            r = CLAMP(signed9(l + r), -128, 127);
            g = CLAMP(signed9(l + g), -128, 127);
            b = CLAMP(signed9(l + b), -128, 127);

            if (!mdec->output_signed) {
                r ^= 0x80;
                g ^= 0x80;
                b ^= 0x80;
            }

            if (mdec->output_depth == 3) {
                uint16_t r5 = ((uint8_t)r) >> 3;
                uint16_t g5 = ((uint8_t)g) >> 3;
                uint16_t b5 = ((uint8_t)b) >> 3;

                uint16_t rgb = (b5 << 10) | (g5 << 5) | r5;

                if (mdec->output_bit15)
                    rgb |= 0x8000;
                
                buf[0 + ((x + xx) + (y + yy) * 16) * 2] = rgb & 0xff;
                buf[1 + ((x + xx) + (y + yy) * 16) * 2] = rgb >> 8;
            } else {
                buf[0 + ((x + xx) + (y + yy) * 16) * 3] = r & 0xff;
                buf[1 + ((x + xx) + (y + yy) * 16) * 3] = g & 0xff;
                buf[2 + ((x + xx) + (y + yy) * 16) * 3] = b & 0xff;
            }
        }
    }
}


/* No parameter read or quant-table access occurs past its declared bound.
 * k=63 completes a block without consuming the following padding/DC word.
 * An overflowing run terminates the block before quant[k] is evaluated.
 */
static int block(int16_t* dst,uint32_t* pos,uint32_t end,const uint8_t* quant) {
 uint32_t p=*pos,k=0,n;
 int q,val;
 for(int i=0;i<64;i++)dst[i]=0;
 do {if(p>=end)return 0;n=input[p++];}while(n==0xfe00);
 q=n>>10;val=signed10(n)*quant[0];
 for(;;) {
  if(!q)val=signed10(n)*2;
  dst[q?zagzig[k]:k]=(int16_t)CLAMP(val,-1024,1023);
  if(k==63)break;
  if(p>=end)return 0;
  n=input[p++];k+=(n>>10)+1;
  if(k>=64)break;
  val=(signed10(n)*quant[k]*q+4)/8;
 }
 *pos=p;return 1;
}
static void transform(int16_t* dst,int mode) {
 if(mode)documented_idct(dst,scales);else real_idct(dst,scales);
}
static void rgb_block(psx_mdec_t* ctx,uint8_t* dst,int x,int y,int mode) {
 if(mode)documented_yuv_to_rgb(ctx,dst,x,y);else yuv_to_rgb(ctx,dst,x,y);
}
/* Fixed private-memory buffers, accessed through the typed JS service. */
uint32_t mdec_buffer(uint32_t which) {
 switch(which) {
 case 0:return (uint32_t)input;case 1:return (uint32_t)output;
 case 2:return (uint32_t)quant_y;case 3:return (uint32_t)quant_uv;
 case 4:return (uint32_t)scales;case 5:return (uint32_t)result;
 default:return 0;
 }
}
/* Error codes: 1 contract, 2 incomplete macroblock, 3 output bound.
 * On refusal output and result remain unchanged. The first pass validates the
 * entire parameter stream and capacity before any pixel/result write.
 * Output is an ordered sequence of row-major 16x16 RGB macroblocks. DMA1 bus
 * reordering/FIFO signaling and callbacks belong to the device integration.
 */
int mdec_decode(uint32_t bytes,uint32_t command,uint32_t capacity,
                uint32_t maximum_blocks,int math_mode) {
 uint32_t end=bytes/2,p=0,count=0,padding=0,bpp=(command>>27)&3;
 uint32_t block_bytes=bpp==3?512:768;
 int16_t scratch[64];
 if(!bytes||(bytes&3)||bytes>MAX_HALFWORDS*2u||(command>>29)!=1||
    (command&0x01ff0000u)||(command&65535)!=bytes/4||bpp<2||
    !maximum_blocks||maximum_blocks>MAX_BLOCKS||capacity>sizeof(output)||
    math_mode<0||math_mode>1)return 1;
 while(p<end) {
  uint32_t padding_start=p;
  while(p<end&&input[p]==0xfe00)p++;
  if(p==end){padding=end-padding_start;break;}
  if(count>=maximum_blocks||(count+1)*block_bytes>capacity)return 3;
  for(int j=0;j<6;j++)if(!block(scratch,&p,end,j<2?quant_uv:quant_y))return 2;
  count++;
 }
 if(!count)return 2;
 p=0;
 psx_mdec_t ctx;
 ctx.output_depth=bpp;ctx.output_signed=(command>>26)&1;
 ctx.output_bit15=(command>>25)&1;
 for(uint32_t i=0;i<count;i++) {
  block(ctx.crblk,&p,end,quant_uv);transform(ctx.crblk,math_mode);
  block(ctx.cbblk,&p,end,quant_uv);transform(ctx.cbblk,math_mode);
  for(int j=0;j<4;j++) {
   block(ctx.yblk,&p,end,quant_y);transform(ctx.yblk,math_mode);
   rgb_block(&ctx,output+i*block_bytes,(j&1)*8,(j>>1)*8,math_mode);
  }
 }
 result[0]=end;result[1]=count;result[2]=padding;result[3]=count*block_bytes;
 return 0;
}
