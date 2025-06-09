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
# cargo build --target x86_64-unknown-uefi
# file target/x86_64-unknown-uefi/debug/wasabi.efi
# qemu-system-x86_64 --version
# wget https://github.com/hikalium/wasabi/raw/main/third_party/ovmf/RELEASEX64_OVMF.fd
# qemu-system-x86_64 -bios third_party/ovmf/RELEASEX64_OVMF.fd
# cargo build --target x86_64-unknown-uefi
# cp target/x86_64-unknown-uefi/debug/wasabi.efi mnt/EFI/BOOT/BOOTX64.EFI
# qemu-system-x86_64 -bios third_party/ovmf/RELEASEX64_OVMF.fd -drive format=raw,file=fat:rw:mnt
cargo build --target x86_64-unknown-uefi
cp target/x86_64-unknown-uefi/debug/wasabi.efi mnt/EFI/BOOT/BOOTX64.EFI
qemu-system-x86_64 -bios third_party/ovmf/RELEASEX64_OVMF.fd -drive format=raw,file=fat:rw:mnt
