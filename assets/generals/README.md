# 将軍イラスト

内蔵 **image_gen** でyyIkusa用に生成した2Dキャラクターイラスト。日本画・浮世絵・水墨画・3D・CGIは採用しない。最初に試作した浮世絵版の巴御前は不採用で、このフォルダーには含めていない。

- `generation.json`：20枠の順序、名前、画風、使用した全プロンプト、生成元ファイル名。
- `expressions/<id>.png`：1人につき5列×2行の生成原画。徴姉妹は2人が一緒に写る共同指揮の1枠。
- `<id>.png`：各原画の先頭セルを梱包時に128pxへ縮小した通常肖像。
- `portraits.bgra`：通常肖像20枚、128×128 BGRA。
- `expressions/expressions.bgra`：戦況用20×10枚、96×96 BGRA。
- `catalog.jpg`：ゲーム用128px肖像の確認一覧。

10表情の順序は `Ready, Advance, Hold, Contact, Danger, Support, Loss, Relief, Capture, Victory`。`src/general_radio.rs` の `Cue` と一致する。通常肖像と表情の人物順は `src/generals.rs` の `GENERALS` と一致する。

```powershell
./tools/pack_generals.ps1
cargo test --locked --lib
```

梱包にはWindowsの.NET/System.Drawingだけを使用する。原画を生成し直さず、セルの切り出し・継ぎ目の1%除去・縮小・BGRA変換を行う。人物自体の加工はしない。生成原画をGitに保持し、ブラウザとWindowsの実行ファイルには梱包済みBGRAだけを埋め込む。

イラストは史料上の肖像の復元ではなく、史実・伝説をもとにした創作キャラクター表現。

## 並行作業で追加された6名の描き直し

婦好、立花誾千代、ンジンガ、ペンテシレイア、上杉謙信、クレオパトラの原画を、現代アニメ系の2Dセル塗りで再生成した。各10表情。日本画、紙の質感、水彩・油彩、写実、3Dは使用しない。

- `generation-additions.json`：追加6名の名前・画風・全プロンプト。
- `expressions/<id>.png`：差し替え済みの生成原画。
- `<id>.png`：追加6名の128px肖像。
- `catalog-additions.jpg`：新しい肖像の一覧。
- `additions/portraits.bgra`, `additions/expressions.bgra`：6名・60表情の追加分だけを梱包したデータ。

```powershell
./tools/pack_general_additions.ps1
```

今回の作業時点では追加6名は原画だけが作業フォルダーにあり、ゲームの `GENERALS` と通常の `generation.json` は20枠のままだった。既存20枠の名簿・性能・梱包済み画像は変更せず、追加分の素材差し替えとして保存している。追加分を名簿へ登録する作業では、この描き直した原画を使う。
