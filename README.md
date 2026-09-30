# ZennTutor

**毎朝 Zenn から「今日のあなたに効く1本」だけを厳選し、AI チューターとの対話で理解まで導く学習アプリ（macOS / Android）。**

情報過多な技術記事を「多読」するのをやめ、1日15〜20分の対話で1つの概念を自分の言葉にする——そのための学習基盤です。

<!-- スクリーンショット / GIF をここに -->
<!-- ![ZennTutor](docs/screenshot.png) -->

## 特徴

- **自動スカウト** — OS ログイン時に起動してトレイに常駐。毎朝、規定時刻を過ぎると裏側で Zenn の RSS を巡回し、記事を1本だけ用意します。
- **文脈ブリッジ** — 記事の要約に加えて「なぜ今あなたが読むべきか」「手持ちの知識の何とつながるか」を3行で提示します。
- **ソクラテス式対話** — AI は答えを教えません。「Flutter でいう〇〇に相当しますが、違いは何だと思いますか？」と問いかけて、考えを整理させます。「例えて」「Dartでいうと？」と聞けば、すぐに噛み砕いて説明します。
- **理解度チェック** — 最後に「一言でいうと？」を自分の言葉で提出します。AI のパスが出れば、その日の学習は完了です。
- **プロファイルの成長** — パスした概念を記録し、翌日以降の記事選定と対話に反映します。
- **無料枠で完結** — ルールベースの1次選抜で候補を数本に絞ってから Gemini API に渡すので、無料枠に収まります。データはすべてローカルの SQLite に保存し、クラウドは不要です。

## 仕組み

```
Zenn トピック別 RSS ──▶ 1次選抜（ルールベース・API呼び出しなし）
  nextjs / react /        ・キーワードシード一致（タイトル／タグ／本文冒頭）
  typescript / aws ...    ・設計・ベストプラクティス系の語を加点
                          ・Django/Flutter との接点を加点
                          ・「入門・環境構築」「エラー解決ログ」「ポエム」を減点
                                  │ 上位 数本
                                  ▼
                         2次選抜（Gemini）──▶ 今日の1本 + 3行ブリッジ
                                  │
                                  ▼
                   ソクラテス式対話 → 一言まとめ → パス判定 → 理解メモ
```

| レイヤー | 採用技術 |
| --- | --- |
| 実行基盤 | Tauri 2（Rust + React / Vite / TypeScript）。デスクトップと Android で同じコードを使用 |
| スケジューリング | アプリ内タイマー（5分ごとに「日付が変わり規定時刻を過ぎたか」を確認） |
| LLM | Gemini API（既定: `gemini-3.6-flash`） |
| 永続化 | ローカル SQLite（既読記事・対話ログ・理解メモ） |

## インストール

[Releases](../../releases) から、お使いの OS 向けのファイルをダウンロードしてください。

| OS | ファイル |
| --- | --- |
| macOS（Apple Silicon） | `ZennTutor_x.y.z_aarch64.dmg` |
| Android（8.0 以降 / arm64・armv7） | `ZennTutor_x.y.z_android.apk` |

> **macOS で「開発元を検証できません」と表示される場合**
> 未署名のビルドでは macOS の警告が出ます。アプリケーションフォルダへコピーしたあと、次のどちらかを行ってください。
> - Finder で ZennTutor を右クリックし「開く」を選ぶ
> - ターミナルで `xattr -cr /Applications/ZennTutor.app` を実行する

> **Android でインストールする場合**
> Google Play 以外からの配布なので、APK を開くと「提供元不明のアプリ」の許可を求められます。ブラウザ（またはファイルアプリ）にインストールを許可してから進めてください。

### Android 版とデスクトップ版の違い

| | デスクトップ | Android |
| --- | --- | --- |
| 自動スカウト | ログイン時に起動してトレイに常駐し、裏側で実行 | アプリを開いている間にタイマーが動作し、開いた時点で当日分を用意 |
| トレイ常駐・自動起動 | あり | なし（OS の制約のため） |
| データ | 端末ごとに独立（同期なし） | 同左 |

## はじめかた

1. [Google AI Studio](https://aistudio.google.com/apikey) で Gemini API キーを無料で発行します。
2. ZennTutor を起動し、**設定 → Gemini API** にキーを貼り付けて「保存」を押し、「接続テスト」で疎通を確認します。
3. **今日の1本 → 今すぐスカウト** を押します（翌日からは自動です）。
4. 「対話を始める」から AI の問いかけに答えていき、最後に「一言まとめを提出」でパスを目指します。

ウィンドウを閉じてもアプリはトレイに常駐します。完全に終了するときは、トレイアイコンのメニューから「終了」を選んでください。

## 設定

設定画面からいつでも変更できます。

| 項目 | 既定値 | 説明 |
| --- | --- | --- |
| モデル | `gemini-3.6-flash` | Gemini のモデル名 |
| 自動スカウト時刻 | 6:00 | この時刻を過ぎたら、その日のスカウトを実行します |
| キーワードシード | Next.js, React, TypeScript, Vercel, Cloudflare, Render, AWS, AIエージェント, システム設計, アーキテクチャ | 1次選抜の加点キーワード |
| Zenn トピック | nextjs, react, typescript, vercel, cloudflare, render, aws, aiagent, systemdesign, architecture | `zenn.dev/topics/{topic}/feed` を取得します |
| 候補数 | 5 | Gemini に渡す1次通過記事の数 |
| 記事の新しさ | 14日 | これより古い記事は対象外です |
| 自動起動 | ON | OS ログイン時にトレイ常駐で起動します |

「1次選抜をプレビュー」を押すと、Gemini を呼ばずにスコアリング結果と加点・減点の理由を確認できます。キーワードを調整するときに便利です。

API キーは環境変数 `GEMINI_API_KEY` でも指定できます。環境変数があれば、そちらが優先されます。

### 学習者プロファイル

現在のプロファイル（Django / Flutter を習得済みで、Next.js と AI 時代の設計へ移行中）は `src-tauri/src/prompts.rs` の `USER_PROFILE` に定義しています。ご自身の経歴に合わせて書き換えてください。

## Gemini 無料枠について

- レート制限（HTTP 429）に達すると、画面にその旨を表示します。自動スカウトは60分後に再試行します。日次上限の場合は翌日に回復します。
- 1日の API 呼び出しは「2次選抜1回 ＋ 対話数回 ＋ 判定1回」程度です。

## ソースからビルド

必要なもの: [Rust](https://rustup.rs/)、Node.js 20 以上、[Tauri の OS 別前提条件](https://tauri.app/start/prerequisites/)

```bash
git clone https://github.com/piasugoihito/zenn-tutor.git
cd zenn-tutor
npm install

npm run tauri dev     # 開発モード
npm run tauri build   # 配布用ビルド（src-tauri/target/release/bundle/ に出力）
```

### Android（APK）

必要なもの: Android Studio（SDK / NDK / 付属の JDK）、Rust の Android ターゲット

```bash
export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export NDK_HOME="$ANDROID_HOME/ndk/<version>"
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

npm run tauri android dev                                         # 実機 / エミュレータで開発
npm run tauri android build -- --apk --target aarch64 --target armv7   # リリース APK
# → src-tauri/gen/android/app/build/outputs/apk/universal/release/
```

リリース署名には `src-tauri/gen/android/keystore.properties` を使います（gitignore 済み。**コミットしないでください**）。

```properties
storeFile=/absolute/path/to/release.jks
storePassword=...
keyAlias=...
password=...
```

鍵は `keytool -genkeypair -v -keystore release.jks -alias <alias> -keyalg RSA -keysize 2048 -validity 10000` で作成できます。このファイルがない場合、リリース APK は未署名で出力されるためインストールできません。

テスト（スコアリングのユニットテスト）:

```bash
cd src-tauri
cargo test
cargo test live_preview -- --ignored --nocapture   # 実際の Zenn RSS で1次選抜を確認
```

## ディレクトリ構成

```
src/                     フロントエンド（React）
  views/Home.tsx         今日の1本・未読バックログ
  views/Session.tsx      ソクラテス式対話・一言まとめ
  views/History.tsx      履歴
  views/Memos.tsx        理解メモ
  views/Settings.tsx     設定
src-tauri/gen/android/   Android プロジェクト（tauri android init で生成）
src-tauri/src/
  lib.rs                 コマンド・スケジューラ・トレイ・自動起動（トレイと自動起動はデスクトップのみ）
  scout.rs               RSS 取得・1次選抜スコアリング・2次選抜
  tutor.rs               対話・理解度判定
  prompts.rs             プロファイルとプロンプト
  gemini.rs              Gemini API クライアント
  db.rs                  SQLite
  settings.rs            設定
```

## データの保存場所

| OS | パス |
| --- | --- |
| macOS | `~/Library/Application Support/dev.zenntutor.app/zenntutor.sqlite3` |
| Windows | `%APPDATA%\dev.zenntutor.app\zenntutor.sqlite3` |
| Linux | `~/.local/share/dev.zenntutor.app/zenntutor.sqlite3` |
| Android | アプリ内領域（アンインストールで削除） |

アンインストール後にデータも消したい場合は、このフォルダを削除してください。

## 注意事項

- 本アプリは Zenn の公式アプリではありません。記事の取得には、Zenn が公開している RSS と、記事本文の取得用 API を利用しています。アクセスは1日数回程度に抑えています。
- 記事の著作権は各著者に帰属します。本アプリは本文をローカルで AI に渡すだけで、再配布はしません。

## ライセンス

[MIT](LICENSE)
