// no_std により標準ライブラリの使用を禁止（カーネルやUEFI環境で必要）
#![no_std]
// エントリポイントを独自に定義する（通常の main 関数を使用しない）
#![no_main]
// Rust の未安定機能 offset_of を使用するために必要
#![feature(offset_of)]

use core::arch::asm; // アセンブリ言語の使用
use core::mem::{offset_of, size_of}; // メモリ操作用
use core::panic::PanicInfo; // パニック時の情報取得
use core::ptr::null_mut; // ヌルポインタ操作
use core::slice; // 生ポインタからスライス生成

// UEFI の void 型相当（任意のポインタ）
type EfiVoid = u8;

// UEFI のハンドル型（通常はポインタや64bit整数）
type EfiHandle = u64;

// 独自定義の Result 型（エラー型は文字列参照）
type Result<T> = core::result::Result<T, &'static str>;

/// UEFI で使用される GUID（グローバル一意識別子）
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct EfiGuid {
    pub data0: u32,
    pub data1: u16,
    pub data2: u16,
    pub data3: [u8; 8],
}

// Graphics Output Protocol の GUID（GOPを特定するために使用）
const EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data0: 0x9042a9de,
    data1: 0x23dc,
    data2: 0x4a38,
    data3: [0x96, 0xfb, 0x7a, 0xde, 0xd0, 0x80, 0x51, 0x6a],
};

/// UEFI 関数の返却値を示すステータス列挙体（Success のみ定義）
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[must_use]
#[repr(u64)]
enum EfiStatus {
    Success = 0,
}

/// UEFI のブートサービス構造体（一部のみ定義）
#[repr(C)]
struct EfiBootServicesTable {
    _reserved0: [u64; 40], // 先頭の40個の未使用領域
    locate_protocol: extern "win64" fn(
        protocol: *const EfiGuid,
        registration: *const EfiVoid,
        interface: *mut *mut EfiVoid,
    ) -> EfiStatus, // プロトコル検索関数
}

// locate_protocol のオフセットが 320 であることをコンパイル時に検証
const _: () = assert!(offset_of!(EfiBootServicesTable, locate_protocol) == 320);

/// UEFI のシステムテーブル構造体（ブートサービスへのポインタを含む）
#[repr(C)]
struct EfiSystemTable {
    _reserved0: [u64; 12],                            // 未使用領域
    pub boot_services: &'static EfiBootServicesTable, // ブートサービス構造体
}

// boot_services のオフセットが 96 であることを検証
const _: () = assert!(offset_of!(EfiSystemTable, boot_services) == 96);

/// 画面情報の構造体（GOPモード情報の一部）
#[repr(C)]
#[derive(Debug)]
struct EfiGraphicsOutputProtocolPixelInfo {
    version: u32,
    pub horizontal_resolution: u32,
    pub vertical_resolution: u32,
    _padding0: [u32; 5], // パディング（未使用）
    pub pixels_per_scan_line: u32,
}

// サイズが36バイトであることを確認（UEFI ABI 要件）
const _: () = assert!(size_of::<EfiGraphicsOutputProtocolPixelInfo>() == 36);

/// GOPモード情報構造体
#[repr(C)]
#[derive(Debug)]
struct EfiGraphicsOutputProtocolMode<'a> {
    pub max_mode: u32,
    pub mode: u32,
    pub info: &'a EfiGraphicsOutputProtocolPixelInfo,
    pub size_of_info: u64,
    pub frame_buffer_base: usize, // VRAM の先頭アドレス
    pub frame_buffer_size: usize, // VRAM のサイズ（バイト数）
}

/// GOP の構造体（この中の mode フィールドを使って VRAM にアクセスする）
#[repr(C)]
#[derive(Debug)]
struct EfiGraphicsOutputProtocol<'a> {
    reserved: [u64; 3], // 未使用領域
    pub mode: &'a EfiGraphicsOutputProtocolMode<'a>,
}

/// UEFI システムテーブルから Graphics Output Protocol を取得する関数
fn locate_graphic_protocol<'a>(
    efi_system_table: &EfiSystemTable,
) -> Result<&'a EfiGraphicsOutputProtocol<'a>> {
    // 出力用ポインタを null 初期化
    let mut graphic_output_protocol = null_mut::<EfiGraphicsOutputProtocol>();

    // locate_protocol を使って GOP のインターフェースを取得
    let status = (efi_system_table.boot_services.locate_protocol)(
        &EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
        null_mut::<EfiVoid>(),
        &mut graphic_output_protocol as *mut *mut EfiGraphicsOutputProtocol as *mut *mut EfiVoid,
    );

    // 成功しなければエラー
    if status != EfiStatus::Success {
        return Err("Failed to locate graphics output protocol");
    }

    // 安全に参照へ変換して返却
    Ok(unsafe { &*graphic_output_protocol })
}

pub fn hlt() {
    // CPU を停止するためのアセンブリ命令
    unsafe {
        asm!("hlt");
    }
}

/// UEFI エントリポイント（UEFIアプリケーションの実行開始点）
#[no_mangle]
fn efi_main(_image_handle: EfiHandle, efi_system_table: &EfiSystemTable) {
    // GOP を取得
    let efi_graphics_output_protocol = locate_graphic_protocol(efi_system_table).unwrap();

    // VRAM 情報を取得
    let vram_addr = efi_graphics_output_protocol.mode.frame_buffer_base;
    let vram_byte_size = efi_graphics_output_protocol.mode.frame_buffer_size;

    // VRAM を u32 単位のスライスに変換
    let vram = unsafe {
        slice::from_raw_parts_mut(vram_addr as *mut u32, vram_byte_size / size_of::<u32>())
    };

    // VRAM のすべてのピクセルを白 (0xFFFFFF) に塗りつぶす
    for e in vram {
        *e = 0xffffff;
    }

    // 無限ループで終了をブロック
    loop {
        hlt(); // CPU を停止
    }
}

/// パニックハンドラ（panic 時の処理）
/// no_std 環境ではこれが必須
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        hlt();
    }
}
