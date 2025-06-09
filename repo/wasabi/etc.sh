#!/bin/zsh

# curl -s https://hikalium.com/bmp/white_diamond_5x5.bmp.hex > bmp.hex
# cat bmp.hex | sed 's/ffffff/ff0000/g' | sed 's/000000/00ffff/g' > bmp2.hex
# cat bmp2.hex | xxd -r -p > bmp2.bmp

# xxd -r -p img.hex > img.bin
# ls -lah img.bin
# qemu-system-x86_64 -drive file=img.bin,format=raw
# rustup target list
# rustup target list | grep uefi
# cargo build --target x86_64-unknown-uefi
# rustup target add x86_64-unknown-uefi
cargo build --target x86_64-unknown-uefi