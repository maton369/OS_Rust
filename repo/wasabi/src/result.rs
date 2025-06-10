// 独自定義の Result 型（エラー型は文字列参照）
pub type Result<T> = core::result::Result<T, &'static str>;
