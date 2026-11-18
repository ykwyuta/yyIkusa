# yyIkusa専用ブランド素材

内蔵image_genで生成した2D原画。日本画・浮世絵・水墨画・3Dは使用しない。生成に使った全プロンプトと元ファイル名は `generation.json` に保存する。

- `icon-source.png`：鷹・交差する剣・朱の軍旗を組み合わせた専用アイコン原画。
- `title-source.png`：巴御前・ジャンヌ・ダルク・チンギス・ハン、象兵・騎兵と鷹を描いたタイトル背景原画。左側を操作パネル用の暗い余白にしている。
- `ikusa.ico`：16/24/32/48/64/128/256pxを含むWindowsアイコン。実行ファイルに埋め込み、ブラウザのfaviconにも使う。
- `mark.rgb`：タイトル画面の128px紋章。
- `title.rgb`：Windows・ブラウザ共通の1280×720背景。
- `web/ikusa-192.png`, `web/ikusa-512.png`, `web/ikusa-apple.png`：Webアプリ・ホーム画面用。
- `web/ikusa-cover.jpg`：ブラウザ読み込み画面用背景。

`./tools/pack_brand.ps1` で原画を縮小・梱包する。原画の人物や紋章を描き変える処理はない。`pack_ikusa_art.py` は兵種アトラスだけを再生成するので、このタイトル画像を上書きしない。
