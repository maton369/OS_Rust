#!/bin/bash -e

# プロジェクトのルートディレクトリへ移動
PROJ_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJ_ROOT"

# EFIファイルのパス（引数から取得）
PATH_TO_EFI="$1"
if [[ ! -f "$PATH_TO_EFI" ]]; then
  echo "Error: EFI binary not found at path '$PATH_TO_EFI'"
  exit 1
fi

# 一時マウントディレクトリを再作成
rm -rf mnt
mkdir -p mnt/EFI/BOOT/
cp ${PATH_TO_EFI} mnt/EFI/BOOT/BOOTX64.EFI
set +e
mkdir -p log
qemu-system-x86_64 \
  -m 4G \
  -bios third_party/ovmf/RELEASEX64_OVMF.fd \
  -drive format=raw,file=fat:rw:mnt \
  -chardev stdio,id=char_com1,mux=on,logfile=log/com1.txt \
  -serial chardev:char_com1 \
  -device isa-debug-exit,iobase=0xf4,iosize=0x01
RETCODE=$?
set -e