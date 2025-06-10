#![feature(offset_of)] // 構造体内のフィールドのオフセット取得に必要な nightly 機能
#![feature(custom_test_frameworks)] // 独自のテストランナーを使うための機能（nightly）
#![test_runner(crate::test_runner::test_runner)] // 使用するテストランナーを指定
#![reexport_test_harness_main = "run_unit_tests"] // `cargo test` 用の main 関数を再定義
#![cfg_attr(not(test), no_std)] // テストモード以外では標準ライブラリを無効化

// モジュール定義
pub mod graphics; // 描画処理（文字列描画やピクセル操作）
pub mod qemu; // QEMU デバッグ操作や仮想デバイスとのやり取り
pub mod result; // 独自の Result 型など（文字列エラー）
pub mod uefi; // UEFI 関連構造体・プロトコル・API呼び出し
pub mod x86; // x86向けの低レベル操作（hlt命令など）

// テスト用ランナー定義
#[cfg(test)]
pub mod test_runner;

// テストモード時のエントリポイント
#[cfg(test)]
#[no_mangle]
pub fn efi_main() {
    run_unit_tests()
}
