// no_std により標準ライブラリの使用を禁止（カーネルやUEFI環境で必要）
#![no_std]
// エントリポイントを独自に定義する（通常の main 関数を使用しない）
#![no_main]
// Rust の未安定機能 offset_of を使用するために必要
#![feature(offset_of)]

use core::arch::asm; // アセンブリ言語の使用
use core::cmp::min; // 最小値を取得するための関数
use core::mem::{offset_of, size_of}; // メモリ操作用
use core::panic::PanicInfo; // パニック時の情報取得
use core::ptr::null_mut; // ヌルポインタ操作

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
    // VRAM の初期化を行う。失敗した場合は panic。
    let mut vram = init_vram(efi_system_table).expect("init_vram failed");

    // VRAM の全ピクセルを走査して、緑 (0x00ff00) に塗りつぶす
    for y in 0..vram.height {
        for x in 0..vram.width {
            // ピクセル位置 (x, y) に対応する可変参照を取得し、存在する場合は色を変更
            if let Some(pixel) = vram.pixel_at_mut(x, y) {
                *pixel = 0x00ff00; // 緑 (RGB)
            }
        }
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

/// VRAMなどのビットマップ（画像バッファ）操作のためのトレイト
trait Bitmap {
    fn bytes_per_pixel(&self) -> i64;
    fn pixels_per_line(&self) -> i64;
    fn width(&self) -> i64;
    fn height(&self) -> i64;
    fn buf_mut(&mut self) -> *mut u8;

    /// 任意の座標 (x, y) のピクセルへのポインタを取得（安全性は呼び出し元に依存）
    ///
    /// # Safety
    /// 呼び出し側は `is_in_*_range` を使って `x`, `y` が有効であることを保証する必要があります。
    unsafe fn unchecked_pixel_at_mut(&mut self, x: i64, y: i64) -> *mut u32 {
        self.buf_mut()
            .add(((y * self.pixels_per_line() + x) * self.bytes_per_pixel()) as usize)
            as *mut u32
    }

    /// 安全なピクセル取得：範囲チェック込み
    fn pixel_at_mut(&mut self, x: i64, y: i64) -> Option<&mut u32> {
        if self.is_in_x_range(x) && self.is_in_y_range(y) {
            // SAFETY: x, y の範囲は事前に検証されているため安全
            unsafe { Some(&mut *(self.unchecked_pixel_at_mut(x, y))) }
        } else {
            None
        }
    }

    /// X座標が有効範囲かをチェック
    fn is_in_x_range(&self, px: i64) -> bool {
        0 <= px && px < min(self.width(), self.pixels_per_line())
    }

    /// Y座標が有効範囲かをチェック
    fn is_in_y_range(&self, py: i64) -> bool {
        0 <= py && py < self.height()
    }
}

/// VRAM 情報を保持する構造体（Uefi Graphics Output Protocol を元に構築）
#[derive(Clone, Copy)]
struct VramBufferInfo {
    buf: *mut u8,
    width: i64,
    height: i64,
    pixels_per_line: i64,
}

/// VramBufferInfo に対して Bitmap トレイトを実装
impl Bitmap for VramBufferInfo {
    fn bytes_per_pixel(&self) -> i64 {
        4 // 常に 4 bytes/pixel (ARGB or BGRA などを想定)
    }

    fn pixels_per_line(&self) -> i64 {
        self.pixels_per_line
    }

    fn width(&self) -> i64 {
        self.width
    }

    fn height(&self) -> i64 {
        self.height
    }

    fn buf_mut(&mut self) -> *mut u8 {
        self.buf
    }
}

/// Graphics Output Protocol を取得して VRAM 情報を構築
fn init_vram(efi_system_table: &EfiSystemTable) -> Result<VramBufferInfo> {
    let gp = locate_graphic_protocol(efi_system_table)?;
    Ok(VramBufferInfo {
        buf: gp.mode.frame_buffer_base as *mut u8,
        width: gp.mode.info.horizontal_resolution as i64,
        height: gp.mode.info.vertical_resolution as i64,
        pixels_per_line: gp.mode.info.pixels_per_scan_line as i64,
    })
}
