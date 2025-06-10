use crate::graphics::draw_font_fg; // フォント描画関数
use crate::graphics::Bitmap; // ビットマップ型
use crate::result::Result;
use core::fmt; // フォーマット関連のトレイト
use core::mem::{offset_of, size_of}; // メモリ操作用
use core::ptr::null_mut; // 独自の Result 型 // ヌルポインタ操作

// UEFI の void 型相当（任意のポインタ）
pub type EfiVoid = u8;
// UEFI のハンドル型（通常はポインタや64bit整数）
pub type EfiHandle = u64;

/// UEFI で使用される GUID（グローバル一意識別子）
///
/// GUID（Globally Unique Identifier）は、UEFI内でプロトコルやインターフェースを
/// 一意に識別するために使用される 128ビット の識別子です。
/// 例: Graphics Output Protocol や File System Protocol を区別するために使う。
///
/// 構造としては、標準的な Microsoft GUID フォーマットと同じです。
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct EfiGuid {
    /// GUID の最初の32ビット部分（例: 0xDEADBEEF）
    pub data0: u32,

    /// GUID の次の16ビット部分
    pub data1: u16,

    /// GUID の次の16ビット部分
    pub data2: u16,

    /// GUID の最後の64ビット部分（8バイト）
    /// UEFI ではこの配列形式で持つ
    pub data3: [u8; 8],
}

// Graphics Output Protocol の GUID（GOPを特定するために使用）
//
// この GUID は、UEFI 環境で「Graphics Output Protocol（GOP）」を特定するために使用されます。
// GOP は、UEFI 上でフレームバッファ（VRAM）にアクセスしたり画面解像度情報を取得するための標準プロトコルです。
// `locate_protocol()` にこの GUID を渡すことで、GOP のインターフェースを取得できます。

const EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data0: 0x9042a9de, // GUID の先頭 32bit
    data1: 0x23dc,     // 次の 16bit
    data2: 0x4a38,     // 次の 16bit
    data3: [
        // 残りの 64bit（8バイト）
        0x96, 0xfb, // 最初の 2バイト（上位 16bit）
        0x7a, 0xde, 0xd0, 0x80, 0x51, 0x6a, // 残りの 6バイト
    ],
};

/// UEFI 関数の返却値を示すステータス列挙体（現時点では Success のみ定義）
///
/// UEFI の各関数は `EfiStatus` 型の値を返し、操作の成功/失敗を示します。
/// 一般的には複数のエラーコード（例: `EFI_LOAD_ERROR`, `EFI_OUT_OF_RESOURCES`）がありますが、
/// ここでは簡略化のために「成功（Success）」だけを定義しています。
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
// `Debug`: {:?} で表示できるようにする
// `PartialEq`, `Eq`: 比較演算子（==）が使えるようにする
// `Copy`, `Clone`: 値をコピー可能にする（値渡しの列挙型のため）
#[must_use]
// 関数が `EfiStatus` を返すとき、その戻り値を無視すると警告が出る。
// ステータスコードは通常、必ずチェックすべきなので有用。
#[repr(u64)]
// C言語の `enum` と同様に、基礎型を明示（UEFI仕様では `u64` が使われる）
pub enum EfiStatus {
    Success = 0, // 操作が正常に完了したことを示す
}

/// UEFI がメモリ領域を分類するための列挙型
///
/// 各値は EFI メモリマップ上のメモリ領域の種類を示します。
/// UEFI ファームウェアが OS に提供する重要な情報の一部です。
#[repr(u32)] // UEFI仕様に合わせ、各値は32ビットの整数として表現される
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)] // UPPER_SNAKE_CASE を許容（UEFI名に準拠）

pub enum EfiMemoryType {
    RESERVED = 0,                // 使用されていない予約領域
    LOADER_CODE,                 // OSローダによる実行コード（読み出し専用）
    LOADER_DATA,                 // OSローダによるデータ領域（読み書き可能）
    BOOT_SERVICES_CODE,          // ブートサービス中の実行コード（起動中のみ有効）
    BOOT_SERVICES_DATA,          // ブートサービス中のデータ領域
    RUNTIME_SERVICES_CODE,       // ランタイムサービスの実行コード（OS起動後も維持される）
    RUNTIME_SERVICES_DATA,       // ランタイムサービスのデータ（同上）
    CONVENTIONAL_MEMORY,         // 通常の空きメモリ（OSが利用可能な主なメモリ領域）
    UNUSABLE_MEMORY,             // 使用不可（ハードウェア不良など）
    ACPI_RECLAIM_MEMORY,         // ACPIテーブル用領域（OS起動後に再利用可能）
    ACPI_MEMORY_NVS,             // ACPIのNVS領域（OS起動後も保持が必要なデータ）
    MEMORY_MAPPED_IO,            // メモリマップトI/O領域（デバイスアクセス用）
    MEMORY_MAPPED_IO_PORT_SPACE, // I/Oポート領域（従来のI/Oアクセス）
    PAL_CODE,                    // Itanium プロセッサ向けPALコード（ほとんどの環境では未使用）
    PERSISTENT_MEMORY,           // 再起動後も内容が保持される永続メモリ（NVDIMM等）
}

/// UEFI が返すメモリマップの1エントリ（メモリ領域の記述）
///
/// UEFI の `GetMemoryMap()` 関数が返すバッファはこの構造体の配列として解釈されます。
/// 各エントリは1つの物理メモリ領域を記述しており、種類やサイズ、属性などを示します。
#[repr(C)] // C言語との互換性を確保（UEFI仕様のABIに準拠）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EfiMemoryDescriptor {
    /// メモリ領域の種類（EfiMemoryType 列挙型）
    pub memory_type: EfiMemoryType,

    /// メモリ領域の物理アドレス（開始位置）
    ///
    /// この値はページサイズ（通常 4 KiB）単位で整列されています。
    pub physical_start: u64,

    /// メモリ領域の仮想アドレス（ランタイムサービス向け）
    ///
    /// 通常のブートローダーやOSカーネルでは無視されることも多い。
    pub virtual_start: u64,

    /// このメモリ領域のページ数（1ページ = 4096バイト）
    ///
    /// 実際のサイズは `number_of_pages * 4096` バイトです。
    pub number_of_pages: u64,

    /// メモリ属性（キャッシュ属性などのビットフラグ）
    ///
    /// 例えば、`EFI_MEMORY_WB`（書き込みバック）、`EFI_MEMORY_UC`（キャッシュ不可）など。
    pub attribute: u64,
}

impl EfiMemoryDescriptor {
    /// メモリ領域の種類を返す
    ///
    /// 例：CONVENTIONAL_MEMORY や BOOT_SERVICES_CODE など、
    /// UEFI が分類する用途別のメモリタイプ。
    pub fn memory_type(&self) -> EfiMemoryType {
        self.memory_type
    }

    /// このメモリ領域が保持するページ数を返す
    ///
    /// 1ページは 4KiB（通常）。この値を 4096 倍することでバイト単位のサイズになる。
    pub fn number_of_pages(&self) -> u64 {
        self.number_of_pages
    }
}

/// メモリマップ取得用バッファのサイズ（32 KiB）
///
/// `GetMemoryMap()` 関数を呼び出す際に渡すバッファのサイズを定義します。
/// バッファは `EfiMemoryDescriptor` 構造体の配列として扱われ、
/// UEFIファームウェアが現在のメモリ状態を記録します。
///
/// サイズは余裕を持って大きめに確保する必要があり、
/// 通常は 4 KiB ～ 数十 KiB が推奨されます。
/// ここでは 0x8000 バイト（32 KiB）を指定しています。
const MEMORY_MAP_BUFFER_SIZE: usize = 0x8000;

/// メモリマップ取得と管理に使用する構造体
///
/// UEFI の `GetMemoryMap()` 関数を呼び出す際に必要な引数および結果を
/// 格納するためのコンテナです。
///
/// - `memory_map_buffer` : メモリマップ情報を書き込むバッファ
/// - `memory_map_size`   : バッファのサイズ（バイト単位）。また、呼び出し後は実際に使用されたサイズが入る
/// - `map_key`           : メモリマップの一貫性を保つためのキー。`ExitBootServices()` 呼び出し時に必要
/// - `descriptor_size`   : 1エントリあたりの `EfiMemoryDescriptor` のサイズ（バイト）
/// - `descriptor_version`: `EfiMemoryDescriptor` のバージョン情報
pub struct MemoryMapHolder {
    /// メモリマップ情報を書き込むバッファ（最大 MEMORY_MAP_BUFFER_SIZE バイト）
    pub memory_map_buffer: [u8; MEMORY_MAP_BUFFER_SIZE],

    /// 実際に取得したメモリマップサイズ（GetMemoryMap 呼び出しで更新される）
    pub memory_map_size: usize,

    /// ExitBootServices の呼び出しに必要な整合性確認用キー
    pub map_key: usize,

    /// メモリマップ1エントリのサイズ（通常は size_of::<EfiMemoryDescriptor>()）
    pub descriptor_size: usize,

    /// メモリ記述子のバージョン情報（通常は 1）
    pub descriptor_version: u32,
}

impl MemoryMapHolder {
    /// コンストラクタ：初期化済みの `MemoryMapHolder` を返す
    ///
    /// 初期状態ではメモリマップバッファは全てゼロで初期化され、
    /// バッファサイズは固定の `MEMORY_MAP_BUFFER_SIZE` に設定される。
    ///
    /// 他のフィールドは `GetMemoryMap()` の呼び出しで適切な値に更新される。
    pub const fn new() -> Self {
        Self {
            // 空のメモリマップバッファ（初期は全ゼロ）
            memory_map_buffer: [0; MEMORY_MAP_BUFFER_SIZE],

            // 呼び出し時に必要なバッファサイズ（初期は最大サイズ）
            memory_map_size: MEMORY_MAP_BUFFER_SIZE,

            // ExitBootServices 呼び出し時に使う識別キー（初期は0）
            map_key: 0,

            // 1エントリあたりの記述子サイズ（GetMemoryMap で更新される）
            descriptor_size: 0,

            // 記述子のバージョン番号（GetMemoryMap で更新される）
            descriptor_version: 0,
        }
    }

    /// メモリマップイテレータを返す
    ///
    /// メモリマップの中身を順に走査するためのイテレータを生成。
    /// `for` ループなどで使うことができる。
    pub fn iter(&self) -> MemoryMapIterator {
        MemoryMapIterator {
            map: self, // このインスタンスへの参照を渡す
            ofs: 0,    // 最初の位置から開始
        }
    }
}

impl Default for MemoryMapHolder {
    /// デフォルトの `MemoryMapHolder` を生成する
    ///
    /// `Default` トレイトを使うことで `MemoryMapHolder::default()` のような
    /// 共通の初期化方法が利用可能になる。
    fn default() -> Self {
        Self::new() // デフォルトでは `new()` メソッドを使用して初期化
    }
}

/// メモリマップを反復処理するためのイテレータ構造体
///
/// `MemoryMapHolder` のバッファを走査して、
/// 各 `EfiMemoryDescriptor` を順番に返すための仕組み。
pub struct MemoryMapIterator<'a> {
    /// 参照先のメモリマップ（走査対象）
    map: &'a MemoryMapHolder,

    /// 現在のオフセット（バイト単位）
    ///
    /// `memory_map_buffer` のどの位置を参照しているかを保持する。
    /// 各 `EfiMemoryDescriptor` のサイズ（descriptor_size）分ずつ進める。
    ofs: usize,
}

/// `MemoryMapIterator` に対して `Iterator` トレイトを実装
/// `MemoryMapIterator` に対して `Iterator` トレイトを実装
impl<'a> Iterator for MemoryMapIterator<'a> {
    type Item = &'a EfiMemoryDescriptor;
    fn next(&mut self) -> Option<&'a EfiMemoryDescriptor> {
        if self.ofs >= self.map.memory_map_size {
            None
        } else {
            let e: &EfiMemoryDescriptor = unsafe {
                &*(self.map.memory_map_buffer.as_ptr().add(self.ofs) as *const EfiMemoryDescriptor)
            };
            self.ofs += self.map.descriptor_size;
            Some(e)
        }
    }
}

/// UEFI システムテーブルから Graphics Output Protocol (GOP) を取得する関数
///
/// この関数は、UEFI の `locate_protocol` サービスを使って、
/// 画面描画に必要な Graphics Output Protocol を取得し、
/// 安全な Rust 参照として返します。
///
/// # 引数
/// - `efi_system_table`: UEFI ファームウェアから提供されるシステムテーブル構造体への参照
///
/// # 戻り値
/// - `Result<&EfiGraphicsOutputProtocol>`:
///     成功すれば GOP の参照、失敗すればエラーメッセージを返します。
pub fn locate_graphic_protocol<'a>(
    efi_system_table: &EfiSystemTable,
) -> Result<&'a EfiGraphicsOutputProtocol<'a>> {
    // GOP の出力先ポインタを null で初期化
    // locate_protocol がこのポインタに正しいアドレスを書き込む
    let mut graphic_output_protocol = null_mut::<EfiGraphicsOutputProtocol>();

    // UEFI の locate_protocol を呼び出して、GOP のポインタを取得する
    // - 第一引数: 取得したいプロトコルの GUID（GOP の GUID）
    // - 第二引数: 通常 null（フィルタリングに使われるが今回は未使用）
    // - 第三引数: プロトコルの実体を受け取る out 引数（二重ポインタ）
    let status = (efi_system_table.boot_services.locate_protocol)(
        &EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
        null_mut::<EfiVoid>(),
        &mut graphic_output_protocol as *mut *mut EfiGraphicsOutputProtocol as *mut *mut EfiVoid,
    );

    // 取得に失敗したらエラーとして返す
    if status != EfiStatus::Success {
        return Err("Failed to locate graphics output protocol");
    }

    // 成功したら raw ポインタを安全な参照に変換して返却
    Ok(unsafe { &*graphic_output_protocol })
}

/// UEFI のブートサービステーブル構造体（EFI_BOOT_SERVICES）
#[repr(C)]
pub struct EfiBootServicesTable {
    /// 予約領域（未使用エントリ）。仕様により最初のいくつかの関数ポインタは使用されない。
    _reserved0: [u64; 7],

    /// メモリマップを取得する関数。
    /// UEFI アプリケーションが `ExitBootServices` を呼び出す前に必要。
    /// - `memory_map_size`: 入出力でマップのサイズ（バイト）
    /// - `memory_map`: メモリマップバッファ（NULLなら必要サイズのみ取得）
    /// - `map_key`: マップの一貫性を保証するキー
    /// - `descriptor_size`: 各エントリ（`EfiMemoryDescriptor`）のサイズ
    /// - `descriptor_version`: デスクリプタのバージョン
    pub get_memory_map: extern "win64" fn(
        memory_map_size: *mut usize,
        memory_map: *mut u8,
        map_key: *mut usize,
        descriptor_size: *mut usize,
        descriptor_version: *mut u32,
    ) -> EfiStatus,

    /// `get_memory_map` 以降の予約領域（未使用エントリ）
    _reserved1: [u64; 21],

    /// ブートサービスを終了し、OSが制御を引き継ぐための関数。
    /// - `image_handle`: 呼び出し元の UEFI イメージハンドル
    /// - `map_key`: `get_memory_map` で取得したキー（内容の一貫性を保証）
    pub exit_boot_services: extern "win64" fn(image_handle: EfiHandle, map_key: usize) -> EfiStatus,

    /// `exit_boot_services` 以降の予約領域
    _reserved4: [u64; 10],

    /// プロトコルインターフェースを取得する関数。
    /// Graphics Output Protocol（GOP）などのアクセスに使われる。
    /// - `protocol`: 取得したいプロトコルの GUID
    /// - `registration`: イベント登録用（通常 NULL）
    /// - `interface`: 成功時にプロトコルインターフェースへのポインタが入る
    locate_protocol: extern "win64" fn(
        protocol: *const EfiGuid,
        registration: *const EfiVoid,
        interface: *mut *mut EfiVoid,
    ) -> EfiStatus,
}

impl EfiBootServicesTable {
    /// UEFI の `GetMemoryMap` を呼び出して、現在のメモリマップを取得する。
    ///
    /// # 引数
    /// - `map`: 結果を格納する `MemoryMapHolder`（バッファやサイズなどを含む）
    ///
    /// # 戻り値
    /// - `EfiStatus::Success`：取得成功
    /// - その他：失敗（メモリ不足、引数不正など）
    pub fn get_memory_map(&self, map: &mut MemoryMapHolder) -> EfiStatus {
        // ブートサービステーブルにある関数ポインタ `get_memory_map` を呼び出す
        (self.get_memory_map)(
            &mut map.memory_map_size,           // マップサイズ（入出力）
            map.memory_map_buffer.as_mut_ptr(), // マップを格納するバッファ先頭
            &mut map.map_key,                   // 結果として返されるマップキー
            &mut map.descriptor_size,           // 各エントリのサイズ
            &mut map.descriptor_version,        // デスクリプタのバージョン
        )
    }
}

// `get_memory_map` 関数ポインタのオフセットが 56 バイトであることを検証。
// これは、`_reserved0: [u64; 7]`（8バイト × 7 = 56バイト）直後に配置されるべきであることを確認している。
const _: () = assert!(offset_of!(EfiBootServicesTable, get_memory_map) == 56);

// `exit_boot_services` 関数ポインタのオフセットが 232 バイトであることを検証。
// _reserved1 は 21 要素なので 21 × 8 = 168バイト。
// 56（前半）+ 8（get_memory_map）+ 168（_reserved1） = 232バイト目に位置することを確認。
const _: () = assert!(offset_of!(EfiBootServicesTable, exit_boot_services) == 232);

// `locate_protocol` 関数ポインタのオフセットが 320 バイトであることを検証。
// _reserved4 は 10 要素なので 10 × 8 = 80バイト。
// 232（前半）+ 8（exit_boot_services）+ 80（_reserved4） = 320バイト目に位置することを確認。
const _: () = assert!(offset_of!(EfiBootServicesTable, locate_protocol) == 320);

/// UEFI のシステムテーブル構造体（UEFI が提供するすべてのサービスへのエントリポイント集）
///
/// この構造体は、UEFI ファームウェアから渡される引数 `efi_system_table` に対応しており、
/// UEFI アプリケーションがブートサービスやランタイムサービスへアクセスするための中心的な構造体。
#[repr(C)] // C 言語と互換のメモリレイアウトにする（UEFI 仕様に準拠）
pub struct EfiSystemTable {
    _reserved0: [u64; 12], // 先頭の 12 個のフィールドは UEFI 仕様上予約されており、ここでは使用しない

    /// ブートサービス構造体へのポインタ（UEFI の機能を呼び出すための関数ポインタ群が格納されている）
    ///
    /// 例：メモリマップ取得、プロトコルの検索、ExitBootServices などの操作がここから可能。
    /// 多くの UEFI 機能はこの `boot_services` 経由で呼び出される。
    pub boot_services: &'static EfiBootServicesTable,
}

// boot_services のオフセットが 96 バイトであることをコンパイル時に検証
//
// `EfiSystemTable` 構造体において、`boot_services` フィールドの
// メモリ上の位置（オフセット）が 96 バイトである必要があることを
// Rust のコンパイル時アサートで保証する。
//
// これは、UEFI 仕様で `boot_services` が System Table の 13 番目のフィールド
// （各フィールドが u64（8バイト）なので 8 × 12 = 96 バイト）に
// 位置するという仕様に基づいている。
//
// もし誤って構造体のフィールド順を変更した場合や、
/// repr(C) を忘れて Rust 独自の再配置が発生した場合に
// ビルド時にエラーで検出できるようにするための安全装置。
const _: () = assert!(offset_of!(EfiSystemTable, boot_services) == 96);

/// 画面情報の構造体（GOPモード情報の一部）
///
/// UEFI の Graphics Output Protocol (GOP) における画面モード情報を保持する構造体。
/// この構造体は、現在選択されているビデオモードの解像度やピクセル密度などを示す。
///
/// `#[repr(C)]` により、C言語と互換なメモリレイアウトが保証される。
#[repr(C)]
#[derive(Debug)]
pub struct EfiGraphicsOutputProtocolPixelInfo {
    /// バージョン番号（通常は0、将来の拡張のためのフィールド）
    version: u32,

    /// 水平方向の解像度（画面の幅をピクセル単位で表す）
    pub horizontal_resolution: u32,

    /// 垂直方向の解像度（画面の高さをピクセル単位で表す）
    pub vertical_resolution: u32,

    /// 未使用のパディング領域（UEFI仕様によって予約されているが意味は定義されていない）
    _padding0: [u32; 5],

    /// 1スキャンライン（1行）あたりのピクセル数
    ///
    /// これは画面幅とは一致しない場合があり、VRAM の行間のパディングを含む。
    pub pixels_per_scan_line: u32,
}

// サイズが36バイトであることを確認（UEFI ABI 要件）
//
// UEFIのABI仕様により、EfiGraphicsOutputProtocolPixelInfo構造体は
// 正確に36バイトでなければならない。
// これは他のファームウェアやOSローダーとの互換性を保つためであり、
// フィールドの並びや型が変更されていないことを保証する。
// コンパイル時に検証され、サイズが異なる場合はビルドエラーになる。
const _: () = assert!(size_of::<EfiGraphicsOutputProtocolPixelInfo>() == 36);

/// GOP（Graphics Output Protocol）モード情報構造体
///
/// UEFI のグラフィックス機能（GOP）で現在の表示モードや
/// フレームバッファの情報を保持するために使用される構造体。
#[repr(C)]
#[derive(Debug)]
pub struct EfiGraphicsOutputProtocolMode<'a> {
    /// サポートされているモード数（モード番号の最大値 + 1）
    pub max_mode: u32,

    /// 現在選択されているモード番号（0 から max_mode-1 の範囲）
    pub mode: u32,

    /// 現在のモードに対応する解像度などの詳細情報へのポインタ
    /// EfiGraphicsOutputProtocolPixelInfo を参照する
    pub info: &'a EfiGraphicsOutputProtocolPixelInfo,

    /// info のサイズ（バイト数）。仕様上は sizeof(EfiGraphicsOutputProtocolPixelInfo)
    pub size_of_info: u64,

    /// フレームバッファの物理メモリ上の先頭アドレス
    /// ここにピクセルデータを書き込むことで画面描画が可能になる
    pub frame_buffer_base: usize,

    /// フレームバッファのサイズ（バイト単位）
    /// frame_buffer_base からこのサイズ分が有効なメモリ領域となる
    pub frame_buffer_size: usize,
}

/// GOP（Graphics Output Protocol）の構造体
///
/// UEFI 仕様に基づき、GOP インターフェースはこの構造体を通して提供される。
/// その中でも `mode` フィールドを使って現在のグラフィックモード情報にアクセスできる。
#[repr(C)]
#[derive(Debug)]
pub struct EfiGraphicsOutputProtocol<'a> {
    /// 未使用の予約領域（UEFI仕様により3つの64bit値が予約されている）
    reserved: [u64; 3],

    /// 現在のグラフィックモードに関する情報（解像度、VRAMの場所など）
    /// この `mode` ポインタを介して `frame_buffer_base` を取得できる
    pub mode: &'a EfiGraphicsOutputProtocolMode<'a>,
}

/// VRAM 情報を保持する構造体（Uefi Graphics Output Protocol を元に構築）
///
/// この構造体は、UEFI の Graphics Output Protocol (GOP) により提供される
/// フレームバッファ（VRAM）へのアクセス情報を保持します。
/// `Bitmap` トレイトを実装することで、描画処理で統一的に利用できます。
#[derive(Clone, Copy)]
pub struct VramBufferInfo {
    /// ピクセルデータのバッファ先頭アドレス（VRAMの先頭）
    buf: *mut u8,

    /// 論理的な画面の幅（ピクセル単位）
    pub width: i64,

    /// 論理的な画面の高さ（ピクセル単位）
    pub height: i64,

    /// 1スキャンラインあたりのピクセル数（物理行幅）
    ///
    /// ※ 一般的に width と等しいか、それよりも大きくなることがある（アライメント調整のため）
    pixels_per_line: i64,
}

/// VramBufferInfo に対して Bitmap トレイトを実装
///
/// これにより、VramBufferInfo を一般的なビットマップ描画関数に渡せるようになります。
impl Bitmap for VramBufferInfo {
    /// 1ピクセルあたりのバイト数を返す
    /// ARGB（8bit × 4チャンネル）や BGRA フォーマットを想定して常に 4バイトとしています。
    fn bytes_per_pixel(&self) -> i64 {
        4
    }

    /// 1行あたりのピクセル数（物理的な横幅）を返す
    /// UEFIのGOPではアライメントの都合で `width` よりも大きくなる場合があります。
    fn pixels_per_line(&self) -> i64 {
        self.pixels_per_line
    }

    /// 論理的な画面の横幅（ピクセル単位）を返す
    /// 実際に描画に使用すべき範囲の横幅です。
    fn width(&self) -> i64 {
        self.width
    }

    /// 論理的な画面の縦幅（ピクセル単位）を返す
    fn height(&self) -> i64 {
        self.height
    }

    /// VRAMバッファへの可変ポインタ（先頭アドレス）を返す
    fn buf_mut(&mut self) -> *mut u8 {
        self.buf
    }
}

/// Graphics Output Protocol を取得して VRAM 情報を構築する関数
///
/// この関数は UEFI の System Table から Graphics Output Protocol (GOP) を取得し、
/// VRAM に関する情報（解像度、バッファ先頭アドレスなど）を `VramBufferInfo` として返します。
///
/// # 引数
/// - `efi_system_table`: UEFI のシステムテーブル構造体への参照
///
/// # 戻り値
/// - 成功: VRAM 情報を格納した `VramBufferInfo`
/// - 失敗: `Result::Err`（GOP を取得できなかった場合など）
pub fn init_vram(efi_system_table: &EfiSystemTable) -> Result<VramBufferInfo> {
    let gp = locate_graphic_protocol(efi_system_table)?;
    Ok(VramBufferInfo {
        buf: gp.mode.frame_buffer_base as *mut u8,
        width: gp.mode.info.horizontal_resolution as i64,
        height: gp.mode.info.vertical_resolution as i64,
        pixels_per_line: gp.mode.info.pixels_per_scan_line as i64,
    })
}

/// VRAM に文字列を描画するためのラッパー構造体
///
/// `core::fmt::Write` トレイトを実装することで、`write!` や `writeln!` マクロを使用して
/// VRAM にテキスト出力できるようにする。
///
/// UEFI 環境で `no_std` のため、通常の標準出力（println!など）は使えない。
/// この構造体を使って、画面に直接テキストを描画する。
pub struct VramTextWriter<'a> {
    /// 描画先の VRAM 情報への可変参照
    vram: &'a mut VramBufferInfo,

    /// 現在の描画位置（次に描画する文字の X 座標）
    cursor_x: i64,

    /// 現在の描画行（次に描画する文字の Y 座標）
    cursor_y: i64,
}

impl<'a> VramTextWriter<'a> {
    /// 新しい `VramTextWriter` インスタンスを作成する関数
    ///
    /// # 引数
    /// - `vram`: VRAM（ビデオメモリ）への可変参照。このバッファにテキストを描画する。
    ///
    /// # 戻り値
    /// - 新しく初期化された `VramTextWriter` 構造体。カーソル位置は画面左上（0,0）に設定される。
    pub fn new(vram: &'a mut VramBufferInfo) -> Self {
        Self {
            vram,        // VRAM への可変参照を保持
            cursor_x: 0, // X座標（左端）から開始
            cursor_y: 0, // Y座標（上端）から開始
        }
    }
}

impl fmt::Write for VramTextWriter<'_> {
    /// `core::fmt::Write` トレイトを実装することで、
    /// `write!` や `writeln!` マクロを使って画面に文字列を描画できるようになる。
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // 入力文字列を1文字ずつ処理
        for c in s.chars() {
            if c == '\n' {
                // 改行文字なら、カーソルを次の行の先頭に移動
                self.cursor_x = 0; // 行頭に戻る
                self.cursor_y += 16; // 1行分（高さ16ピクセル）下に進む
                continue;
            }
            // 通常の文字を描画（白色で描画）
            draw_font_fg(self.vram, self.cursor_x, self.cursor_y, 0xffffff, c);
            self.cursor_x += 8; // 1文字分（幅8ピクセル）右に進める
        }
        Ok(()) // フォーマット処理が成功したことを返す
    }
}

/// ブートサービスを終了して、UEFI から独立した実行状態に移行する
///
/// UEFI アプリケーションが OS に移行する際、ブートサービスを終了する必要がある。
/// `exit_boot_services` を呼び出すには、最新のメモリマップとその map_key を使用しなければならない。
/// もしメモリマップが変更されていた場合は、エラーが返るため再取得して再試行する必要がある。
///
/// # 引数
/// - `image_handle`: 現在の UEFI アプリケーションのハンドル
/// - `efi_system_table`: UEFI システムテーブルへの参照
/// - `memory_map`: 最新のメモリマップ情報を格納する構造体
pub fn exit_boot_services(
    image_handle: EfiHandle,
    efi_system_table: &EfiSystemTable,
    memory_map: &mut MemoryMapHolder,
) {
    loop {
        // 最新のメモリマップを取得
        let status = efi_system_table.boot_services.get_memory_map(memory_map);

        // get_memory_map が失敗した場合は panic
        assert_eq!(status, EfiStatus::Success);

        // メモリマップ取得時に得た map_key を使って exit_boot_services を呼び出す
        let status =
            (efi_system_table.boot_services.exit_boot_services)(image_handle, memory_map.map_key);

        // 成功したらブートサービス終了完了 → ループを抜ける
        if status == EfiStatus::Success {
            break;
        }

        // 失敗した場合、メモリマップの内容が更新された可能性があるため
        // 再度 get_memory_map を呼んで map_key を更新し、リトライ
    }
}
