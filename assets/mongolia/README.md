# yyMongoliaから流用したアセット

取得元: git@github.com:ykwyuta/yyMongolia.git

- `steppe.bgra`: 歩兵・弓兵・騎馬槍兵・騎馬弓兵。
- `southern.bgra`: 象兵。
- `japan.bgra`: 鉄砲隊の人物素体、本陣建築。
- `generation.json`: 取得元が記録した素材生成情報。未使用アトラスの情報も含む。
- アプリ・Webアイコンは専用生成した assets/branding/ の素材を使う。

BGRAの元ファイルを保持し、`python tools/pack_ikusa_art.py` でゲーム用RGBA/RLEアトラスとタイトル背景を再生成する。外部Pythonパッケージは不要。元のセルを抽出・縮小・アルファ形式変換して梱包する。

鉄砲は人物や騎兵にコード描画で重ねているため、専用原画としては暫定的。大砲・柵・鷹は `src/game.rs` の図形描画で実装。将軍のイラストは `assets/generals/` に専用生成したものを使う。

YYww2の地形アトラスと描画・入力・シミュレーションを継承。旧作の戦車アトラスと将軍肖像は含めない。

流用元コミット: 457a362751b70eb825ecdc62cbef9cc43733575b
エンジン元YYww2: a8fc52aae4c66661e63f36e70f96abcf6628d6c5
