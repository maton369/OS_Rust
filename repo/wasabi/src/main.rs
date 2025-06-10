// no_std により標準ライブラリの使用を禁止（カーネルやUEFI環境で必要）
#![no_std]
// エントリポイントを独自に定義する（通常の main 関数を使用しない）
#![no_main]
// Rust の未安定機能 offset_of を使用するために必要
#![feature(offset_of)]

use core::arch::asm; // アセンブリ言語の使用
use core::cmp::min; // 最小値を取得するための関数
use core::fmt; // フォーマット関連のトレイト
use core::fmt::Write; // 書き込みトレイト
use core::mem::{offset_of, size_of}; // メモリ操作用
use core::panic::PanicInfo; // パニック時の情報取得
use core::ptr::null_mut; // ヌルポインタ操作
use core::writeln; // 64ビット整数の書き込み

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

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum EfiMemoryType {
    RESERVED = 0,
    LOADER_CODE,
    LOADER_DATA,
    BOOT_SERVICES_CODE,
    BOOT_SERVICES_DATA,
    RUNTIME_SERVICES_CODE,
    RUNTIME_SERVICES_DATA,
    CONVENTIONAL_MEMORY,
    UNUSABLE_MEMORY,
    ACPI_RECLAIM_MEMORY,
    ACPI_MEMORY_NVS,
    MEMORY_MAPPED_IO,
    MEMORY_MAPPED_IO_PORT_SPACE,
    PAL_CODE,
    PERSISTENT_MEMORY,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct EfiMemoryDescriptor {
    memory_type: EfiMemoryType,
    physical_start: u64,
    virtual_start: u64,
    number_of_pages: u64,
    attribute: u64,
}

const MEMORY_MAP_BUFFER_SIZE: usize = 0x8000;

struct MemoryMapHolder {
    memory_map_buffer: [u8; MEMORY_MAP_BUFFER_SIZE],
    memory_map_size: usize,
    map_key: usize,
    descriptor_size: usize,
    descriptor_version: u32,
}

impl MemoryMapHolder {
    pub const fn new() -> Self {
        Self {
            memory_map_buffer: [0; MEMORY_MAP_BUFFER_SIZE],
            memory_map_size: MEMORY_MAP_BUFFER_SIZE,
            map_key: 0,
            descriptor_size: 0,
            descriptor_version: 0,
        }
    }

    pub fn iter(&self) -> MemoryMapIterator {
        MemoryMapIterator { map: self, ofs: 0 }
    }
}

struct MemoryMapIterator<'a> {
    map: &'a MemoryMapHolder,
    ofs: usize,
}

impl<'a> Iterator for MemoryMapIterator<'a> {
    type Item = &'a EfiMemoryDescriptor;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ofs >= self.map.memory_map_size {
            None
        } else {
            let entry = unsafe {
                &*(self.map.memory_map_buffer.as_ptr().add(self.ofs) as *const EfiMemoryDescriptor)
            };
            self.ofs += self.map.descriptor_size;
            Some(entry)
        }
    }
}

#[repr(C)]
struct EfiBootServicesTable {
    _reserved0: [u64; 7],
    get_memory_map: extern "win64" fn(
        memory_map_size: *mut usize,
        memory_map: *mut u8,
        map_key: *mut usize,
        descriptor_size: *mut usize,
        descriptor_version: *mut u32,
    ) -> EfiStatus,

    _reserved1: [u64; 32],

    locate_protocol: extern "win64" fn(
        protocol: *const EfiGuid,
        registration: *const EfiVoid,
        interface: *mut *mut EfiVoid,
    ) -> EfiStatus,
}

impl EfiBootServicesTable {
    fn get_memory_map(&self, map: &mut MemoryMapHolder) -> EfiStatus {
        (self.get_memory_map)(
            &mut map.memory_map_size,
            map.memory_map_buffer.as_mut_ptr(),
            &mut map.map_key,
            &mut map.descriptor_size,
            &mut map.descriptor_version,
        )
    }
}

const _: () = assert!(offset_of!(EfiBootServicesTable, get_memory_map) == 56);
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

    let vw = vram.width;
    let vh = vram.height;

    // 画面全体を黒で塗りつぶす（背景）
    fill_rect(&mut vram, 0x000000, 0, 0, vw, vh).expect("fill_rect failed");

    // 赤い正方形を表示（左上 32x32）
    fill_rect(&mut vram, 0xff0000, 32, 32, 32, 32).expect("fill_rect failed");

    // 緑の正方形を表示（左上 64x64, サイズ 64x64）
    fill_rect(&mut vram, 0x00ff00, 64, 64, 64, 64).expect("fill_rect failed");

    // 青の正方形を表示（左上 128x128, サイズ 128x128）
    fill_rect(&mut vram, 0x0000ff, 128, 128, 128, 128).expect("fill_rect failed");

    // グラデーションで対角線上に点を打つ（0x010101 * i で明度が上がるグレー）
    for i in 0..256 {
        let _ = draw_point(&mut vram, 0x010101 * i as u32, i, i);
    }

    let grid_size: i64 = 32;
    let rect_size: i64 = grid_size * 8; // 8x8 のグリッドを描く

    // グリッド線の描画（水平・垂直線）
    for i in (0..=rect_size).step_by(grid_size as usize) {
        // 水平線（赤）
        let _ = draw_line(&mut vram, 0xff0000, 0, i, rect_size, i);
        // 垂直線（赤）
        let _ = draw_line(&mut vram, 0xff0000, i, 0, i, rect_size);
    }

    // グリッドの中心点を計算
    let cx = rect_size / 2;
    let cy = rect_size / 2;

    // 中心点から四隅に向かう放射線を描く
    for i in (0..=rect_size).step_by(grid_size as usize) {
        // 左方向：中心 → (0, i)（黄）
        let _ = draw_line(&mut vram, 0xffff00, cx, cy, 0, i);
        // 上方向：中心 → (i, 0)（シアン）
        let _ = draw_line(&mut vram, 0x00ffff, cx, cy, i, 0);
        // 右方向：中心 → (rect_size, i)（マゼンタ）
        let _ = draw_line(&mut vram, 0xff00ff, cx, cy, rect_size, i);
        // 下方向：中心 → (i, rect_size)（白）
        let _ = draw_line(&mut vram, 0xffffff, cx, cy, i, rect_size);
    }

    for (i, c) in "ABCDEF".chars().enumerate() {
        draw_font_fg(&mut vram, i as i64 * 16 + 256, i as i64 * 16, 0xffffff, c);
    }

    draw_str_fg(&mut vram, 256, 256, 0xffffff, "Hello, World!");

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

    // 各メモリ領域を表示
    for descriptor in memory_map.iter() {
        writeln!(w, "{:?}", descriptor).unwrap();
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

/// 安全でないピクセル描画（座標チェックなし）
///
/// # Safety
/// 呼び出し元で (x, y) が有効範囲内であることを保証しなければならない。
unsafe fn unchecked_draw_point<T: Bitmap>(buf: &mut T, color: u32, x: i64, y: i64) {
    *buf.unchecked_pixel_at_mut(x, y) = color;
}

/// 座標チェック付きの安全なピクセル描画
fn draw_point<T: Bitmap>(buf: &mut T, color: u32, x: i64, y: i64) -> Result<()> {
    // pixel_at_mut は範囲チェック済みのポインタ取得
    *(buf.pixel_at_mut(x, y).ok_or("Out of Range")?) = color;
    Ok(())
}

/// 指定した矩形範囲を塗りつぶす（座標チェックあり）
///
/// # 引数
/// - `buf`: 描画対象のバッファ（Bitmap実装）
/// - `color`: 描画色（ARGB）
/// - `px`, `py`: 左上の座標
/// - `w`, `h`: 幅と高さ
fn fill_rect<T: Bitmap>(buf: &mut T, color: u32, px: i64, py: i64, w: i64, h: i64) -> Result<()> {
    // 領域全体が有効範囲内かチェック（右下も含む）
    if !buf.is_in_x_range(px)
        || !buf.is_in_y_range(py)
        || !buf.is_in_x_range(px + w - 1)
        || !buf.is_in_y_range(py + h - 1)
    {
        return Err("Out of Range");
    }

    // 矩形内の各ピクセルを塗りつぶし（高速な unsafe を使用）
    for y in py..py + h {
        for x in px..px + w {
            unsafe {
                unchecked_draw_point(buf, color, x, y);
            }
        }
    }

    Ok(())
}

/// 傾きを考慮して補間位置を計算する関数
/// da = 主軸方向の距離、db = 従軸方向の距離、ia = 主軸上の現在位置
fn calc_slope_point(da: i64, db: i64, ia: i64) -> Option<i64> {
    if da < db {
        // 主軸距離より従軸の方が長い場合は処理しない（分岐先で反転処理するため）
        None
    } else if da == 0 {
        // 主軸距離がゼロなら原点固定
        Some(0)
    } else if (0..=da).contains(&ia) {
        // 中点付き整数補間: y = (2 * db * x + da) / (2 * da)
        Some((2 * db * ia + da) / (2 * da))
    } else {
        None
    }
}

/// 傾きを考慮して直線を描画する関数（高性能・中精度）
fn draw_line<T: Bitmap>(buf: &mut T, color: u32, x0: i64, y0: i64, x1: i64, y1: i64) -> Result<()> {
    // 差分と符号
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = (x1 - x0).signum();
    let sy = (y1 - y0).signum();

    let mut x = x0;
    let mut y = y0;

    // 誤差項
    let mut err = if dx > dy { dx } else { -dy } / 2;
    let mut e2;

    loop {
        // 点を描画（範囲外は無視）
        if let Some(pixel) = buf.pixel_at_mut(x, y) {
            *pixel = color;
        }

        if x == x1 && y == y1 {
            break;
        }

        e2 = err;

        if e2 > -dx {
            err -= dy;
            x += sx;
        }

        if e2 < dy {
            err += dx;
            y += sy;
        }
    }

    Ok(())
}

fn lookup_font(c: char) -> Option<[[char; 8]; 16]> {
    const FONT_SOURCE: &str = include_str!("./font.txt");

    if let Ok(c) = u8::try_from(c) {
        let mut lines = FONT_SOURCE.lines().peekable();

        while let Some(line) = lines.next() {
            if let Some(hex) = line.strip_prefix("0x") {
                if let Ok(idx) = u8::from_str_radix(hex, 16) {
                    if idx != c {
                        // スキップ：次の 16 行を読み飛ばす
                        for _ in 0..16 {
                            lines.next();
                        }
                        continue;
                    }

                    // 対象のフォント行だけを読み取る
                    let mut font = [[' '; 8]; 16];
                    for (y, font_line) in lines.by_ref().take(16).enumerate() {
                        for (x, ch) in font_line.chars().take(8).enumerate() {
                            font[y][x] = ch;
                        }
                    }
                    return Some(font);
                }
            }
        }
    }

    None
}

fn draw_font_fg<T: Bitmap>(buf: &mut T, x: i64, y: i64, color: u32, c: char) {
    if let Some(font) = lookup_font(c) {
        for (dy, row) in font.iter().enumerate() {
            for (dx, pixel) in row.iter().enumerate() {
                if *pixel == '*' {
                    let _ = draw_point(buf, color, x + dx as i64, y + dy as i64);
                }
            }
        }
    }
}

fn draw_str_fg<T: Bitmap>(buf: &mut T, x: i64, y: i64, color: u32, s: &str) {
    for (i, c) in s.chars().enumerate() {
        draw_font_fg(buf, x + i as i64 * 8, y, color, c);
    }
}

/// VRAM に文字列を描画するためのラッパー
struct VramTextWriter<'a> {
    vram: &'a mut VramBufferInfo,
    cursor_x: i64, // カーソルのX座標
    cursor_y: i64, // カーソルのY座標
}

impl<'a> VramTextWriter<'a> {
    /// 新しい VramTextWriter を作成
    fn new(vram: &'a mut VramBufferInfo) -> Self {
        Self {
            vram,
            cursor_x: 0,
            cursor_y: 0,
        }
    }
}

impl fmt::Write for VramTextWriter<'_> {
    /// `core::fmt::Write` を実装することで `write!` マクロなどが使用可能に
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if c == '\n' {
                // 改行文字の場合はカーソルを次の行に移動
                self.cursor_x = 0;
                self.cursor_y += 16; // 1行分の高さ（16px）を加算
                continue;
            }
            draw_font_fg(self.vram, self.cursor_x, self.cursor_y, 0xffffff, c);
            self.cursor_x += 8; // 1文字分の幅（8px）を加算
        }
        Ok(())
    }
}
