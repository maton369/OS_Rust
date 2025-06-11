#![feature(offset_of)] // 構造体内のフィールドのオフセット取得に必要な nightly 機能
#![feature(custom_test_frameworks)] // 独自のテストランナーを使うための機能（nightly）
#![test_runner(crate::test_runner::test_runner)] // 使用するテストランナーを指定
#![reexport_test_harness_main = "run_unit_tests"] // `cargo test` 用の main 関数を再定義
#![cfg_attr(not(test), no_std)] // テストモード以外では標準ライブラリを無効化
#![no_main]
// 通常の main 関数を使用しない（UEFI アプリケーションのエントリポイントを定義するため）
#![feature(sync_unsafe_cell)] // `SyncUnsafeCell` を使用するための機能（nightly）
#![feature(const_caller_location)] // `const_caller_location` を使用するための機能（nightly)
#![feature(const_location_fields)] // `const_location_fields` を使用するための機能（nightly）
#![feature(option_get_or_insert_default)] // `Option::get_or_insert_default` を使用するための機能（nightly）

// モジュール定義
pub mod acpi;
pub mod allocator; // メモリアロケータ（カスタムアロケータの実装）
pub mod executor;
pub mod graphics; // 描画処理（文字列描画やピクセル操作）
pub mod hpet; // HPET（High Precision Event Timer）関連の処理
pub mod init; // 初期化処理（UEFI アプリケーションの初期化）
pub mod mutex; // ミューテックス（排他制御のための同期プリミティブ）
pub mod pci; // PCIデバイスの検出と操作
pub mod print; // 文字列出力やフォーマット処理
pub mod qemu; // QEMU デバッグ操作や仮想デバイスとのやり取り
pub mod result; // 独自の Result 型など（文字列エラー）
pub mod serial; // シリアルポート通信（デバッグ用）
pub mod uefi; // UEFI 関連構造体・プロトコル・API呼び出し
pub mod x86; // x86向けの低レベル操作（hlt命令など） // メモリアロケータ（カスタムアロケータの実装） // シリアルポート通信（デバッグ用） // 初期化処理（UEFI アプリケーションの初期化） // タスク実行やスケジューリング // ACPI（Advanced Configuration and Power Interface）関連の処理 // HPET（High Precision Event Timer）関連の処理 // ミューテックス（排他制御のための同期プリミティブ） // PCIデバイスの検出と操作
pub mod xhci; // USBデバイスの検出と操作

// テスト用ランナー定義
#[cfg(test)]
pub mod test_runner;
pub mod tests; // ユニットテスト（mallocのアライメントや解放のテスト）

// テストモード時のエントリポイント
#[cfg(test)]
#[no_mangle]
fn efi_main(image_handle: uefi::EfiHandle, efi_system_table: &uefi::EfiSystemTable) {
    init::init_basic_runtime(image_handle, efi_system_table);
    run_unit_tests()
}
