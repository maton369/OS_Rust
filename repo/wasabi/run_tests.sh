#!/bin/bash -e

# プロジェクトのルートディレクトリへ移動
PROJ_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJ_ROOT"

TARGET="x86_64-unknown-uefi"
EFI_BINARY="target/${TARGET}/debug/wasabi.efi"
OVMF_PATH="third_party/ovmf/RELEASEX64_OVMF.fd"

# 1. ビルド（テストモード有効）
echo "🔧 Building UEFI test binary..."
cargo build --target $TARGET --features test

# 2. UEFIバイナリが存在するか確認
if [[ ! -f "$EFI_BINARY" ]]; then
  echo "❌ Error: EFI binary not found at path '$EFI_BINARY'"
  exit 1
fi

# 3. テストディスクの準備
echo "📦 Preparing EFI boot structure..."
rm -rf mnt
mkdir -p mnt/EFI/BOOT/
cp "$EFI_BINARY" mnt/EFI/BOOT/BOOTX64.EFI

# 4. QEMU起動
echo "🚀 Running tests in QEMU..."
set +e
qemu-system-x86_64 \
  -m 512M \
  -bios "$OVMF_PATH" \
  -drive if=none,format=raw,file=fat:rw:mnt,id=hd0 \
  -device isa-debug-exit,iobase=0xf4,iosize=0x01 \
  -device ide-hd,drive=hd0 \
  -serial stdio \
  -display none
EXIT_CODE=$?
set -e

# 5. 結果判定（QEMUの仕様：exit_code = (code << 1) | 1）
ACTUAL_CODE=$((EXIT_CODE >> 1))
if [[ $ACTUAL_CODE -eq 1 ]]; then
  echo "✅ TEST PASSED (exit code: $ACTUAL_CODE)"
  exit 0
else
  echo "❌ TEST FAILED (exit code: $ACTUAL_CODE)"
  exit 1
fi
