# METIQ 公式ホームページ

見た目は 1996 年のホームページ。中身は Rust → WebAssembly (SIMD) のレンダリングカーネル。

- **スクショすると普通の昔のページ**になります。演出はすべてマウスの動きに比例する
  1 本のエネルギー値で駆動され、止まると約 2 秒で完全にゼロへ戻ります（DOM 上にも
  transform が 1 つも残りません）。さらに PrintScreen / ⌘⇧ / ⊞⇧ / ウィンドウの
  フォーカス喪失 / タブ非表示 / 印刷 を検知した瞬間に同期的に全演出を停止します。
- JavaScript が無効でも、HTML 3.2 (quirks mode) のただの古いページとして表示されます。

## 演出一覧

| 場所 | 静止時 (スクショ) | 動かすと |
|---|---|---|
| 本文の文字 | 普通のテキスト | 1 文字ずつのバネが震える・カーソルから逃げる・色収差 |
| タイトル METIQ | 虹色 `<FONT>` | 強い反発と回転 |
| リンク・ボタン | 下線付きリンク | 文字が波打つ |
| 地球儀 | 24 コマの GIF 風回転 | 60fps で滑らかに回転、ドラッグで回せる、経緯線が光る |
| アクセスカウンター | 7 桁の数字 | 機械式ドラムが繰り上がり付きで回転、ホバー/クリックで桁が空転 |
| 工事中サイン | 8fps のテープ | テープ加速・回転灯がストロボ |
| 虹色ライン | 静止 | 色相が流れて縄跳びのように波打つ |
| バナー | 88×31 | 光沢が斜めに走る |
| 画面全体 | なし | ノイズ・走査線・VHS のロールバー・ときどき信号の乱れ |
| カーソル | なし | キラキラの軌跡 |
| BGM | — | AudioWorklet 内の 2 個目の wasm インスタンスがチップチューンを合成 |

`prefers-reduced-motion` が有効な環境では、震え・ノイズ系の演出はすべて無効になります。

## 構成

```
core/            Rust (#![no_std], アロケータなし, wasm-bindgen なし)
  src/math.rs      libm なしの sin / exp2 / atan2 / 0x5f3759df
  src/field.rs     文字ごとのバネ場
  src/crt.rs       simd128 で 4 画素同時に xorshift するノイズ
  src/odometer.rs  繰り上がりする機械式カウンター
  src/globe.rs     レイキャスト地球儀 + Bayer ディザ + Web セーフ 216 色
  src/synth.rs     3 声 + LFSR ノイズのシンセ
  ...
site/            公開するファイル一式（静的ホスティングでそのまま動く）
  index.html       本文（「ここを編集」コメントの範囲を書き換えてください）
  js/*.mjs         ES モジュール
  wasm/            ビルド済み metiq_core.wasm
```

## ビルド・確認

```sh
rustup target add wasm32-unknown-unknown
./build.sh                       # core/ → site/wasm/metiq_core.wasm
python3 -m http.server -d site   # http://localhost:8000
```

`file://` では wasm を読み込めないので、ローカルでも HTTP サーバー経由で開いてください。

## 公開

`main` に push すると `.github/workflows/pages.yml` が wasm をビルドして GitHub Pages
へデプロイします（リポジトリ設定の Pages → Source を “GitHub Actions” にしてください）。

## カスタマイズ

- **本文**: `site/index.html` の「▼▼▼ ここから下の本文を編集してください ▼▼▼」の範囲。
  現在は「METIQ は高IQ団体です」以外は「準備中」のプレースホルダーです。
- **アクセスカウンター**: 本物のカウントにはサーバーが必要です。現状はブラウザごとの
  訪問回数 (`localStorage`) + `DATA-BASE` の値を表示します。`#counter` に
  `DATA-ENDPOINT="https://..."` を付けると、その URL に POST して返ってきた
  `{"count": n}` を表示します。
- キリ番（ゾロ目・末尾 000 / 777）を踏むとメッセージが点滅表示されます。
