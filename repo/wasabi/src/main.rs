// no_std により標準ライブラリの使用を禁止（カーネルやUEFI環境で必要）
#![no_std]
// エントリポイントを独自に定義する（通常の main 関数を使用しない）
#![no_main]
// Rust の未安定機能 offset_of を使用するために必要
#![feature(offset_of)]

mod graphics;
mod result;
mod uefi;
mod x86;

use core::arch::asm; // アセンブリ言語の使用
use core::fmt::Write; // 書き込みトレイト
use core::mem::{offset_of, size_of}; // メモリ操作用
use core::panic::PanicInfo; // パニック時の情報取得
use core::writeln; // 64ビット整数の書き込み
use graphics::*;
use uefi::*;
use wasabi::qemu::*;

pub fn hlt() {
    // CPU を停止するためのアセンブリ命令
    unsafe {
        asm!("hlt");
    }
}

/// UEFI エントリポイント（UEFIアプリケーションの実行開始点）
#[no_mangle]
fn efi_main(image_handle: EfiHandle, efi_system_table: &EfiSystemTable) {
    // VRAM の初期化を行う。失敗した場合は panic。
    let mut vram = init_vram(efi_system_table).expect("init_vram failed");

    let vw = vram.width;
    let vh = vram.height;

    // 画面全体を黒で塗りつぶす（背景）
    fill_rect(&mut vram, 0x000000, 0, 0, vw, vh).expect("fill_rect failed");

    draw_test_pattern(&mut vram);

    let mut w = VramTextWriter::new(&mut vram);
    for i in 0..4 {
        writeln!(w, "i={i}").unwrap();
    }

    let mut memory_map = MemoryMapHolder::new();

    // メモリマップ取得
    let status: EfiStatus = efi_system_table
        .boot_services
        .get_memory_map(&mut memory_map);

    // 結果を表示
    writeln!(w, "get_memory_map status: {:?}", status).unwrap();

    let mut total_memory_pages = 0;
    for e in memory_map.iter() {
        if e.memory_type != EfiMemoryType::CONVENTIONAL_MEMORY {
            continue;
        }
        total_memory_pages += e.number_of_pages;
        writeln!(w, "{e:?}").unwrap();
    }

    let total_memory_size_mib = total_memory_pages * 4096 / 1024 / 1024;
    writeln!(
        w,
        "Total: {total_memory_pages} pages = {total_memory_size_mib} MiB"
    )
    .unwrap();

    exit_boot_services(image_handle, efi_system_table, &mut memory_map);

    // 無限ループで終了をブロック
    loop {
        hlt(); // CPU を停止
    }
}

/// パニックハンドラ（panic 時の処理）
/// no_std 環境ではこれが必須
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit_qemu(QemuExitCode::Fail)
}
