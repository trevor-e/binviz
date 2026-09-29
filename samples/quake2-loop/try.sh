#!/bin/sh
# try.sh <function> : compile decomp/<function>.c with the game's flags, match it against game.so
cd "$(dirname "$0")/build"
f=$1
clang -O2 -fno-strict-aliasing -fPIC -fno-stack-protector -fcf-protection=none -w -DYQ2OSTYPE='"Linux"' -DYQ2ARCH='"x86_64"' -DOSTYPE='"Linux"' -DARCH='"x86_64"' -c quake2-5.34/src/gamemut/decomp/$f.c -o decomp_$f.o || exit 1
"${BINVIZ:-../../../target/release/binviz}" match game.so decomp_$f.o $f
