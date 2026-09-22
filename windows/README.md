# Windowsアプリケーション

Windows Core Audioの状態を管理し、Bluetooth Classic SPPを通じてAudio Selectorと同期する常駐アプリケーションです。

## ビルド

Repository Rootの`rust-toolchain.toml`によりRust 1.94.0を使用します。

```powershell
cargo build --locked
```

## テスト

```powershell
cargo test --locked
```

## ログ

既定のログレベルは`audio_selector=info`です。`RUST_LOG`で変更できます。

```powershell
$env:RUST_LOG = 'audio_selector=debug'
cargo run --locked
```

## Core Audio単体確認

引数なしでは、有効な出力・入力Endpointと各ロールの既定値を一度ログ出力して終了します。

```powershell
cargo run --locked
```

変更通知を指定秒数だけ監視するには`--watch`を使用します。通知が連続した場合は、最後の通知から250 ms後に一度だけEndpointと既定値を再取得します。

```powershell
cargo run --locked -- --watch 30
```

既定Endpointを変更する診断用コマンドです。`HANDLE`には直前の一覧に表示された同じデータフローのハンドルを指定します。

```powershell
cargo run --locked -- --set-output HANDLE
cargo run --locked -- --set-input HANDLE
```

ペアリング済みBluetooth情報から、名前が完全一致する`AudioSelector`とSerial Port Profileサービスを確認します。この診断ではCOMポートを列挙またはオープンしません。

```powershell
cargo run --locked -- --find-bluetooth
```

対象のRFCOMM Serial Portサービスへ直接接続し、2秒以内のHELLOハンドシェイク、Windows Audio状態の完全同期、PING/PONGを行う診断コマンドです。

```powershell
cargo run --locked -- --test-spp
```

指定秒数のあいだSPP通信を維持し、ESP32からの出力／入力設定要求を処理します。3ロールすべてが要求先と一致している場合はCore Audio設定APIを呼ばず、同じEndpointの再設定による音切れを避けます。設定結果を返した後は新しい世代番号でWindowsの実状態を完全同期します。アイドル時は10秒ごとにPING/PONGで生存確認し、PINGが2回連続でタイムアウトするか切断を検出すると、1、2、4、8、16、30秒（以後30秒）の待機時間で再接続します。接続と完全同期に成功すると待機時間は1秒へ戻ります。

```powershell
cargo run --locked -- --run-spp 60
```

PING/PONGの成功ログも表示する場合はDEBUGログを有効にします。

```powershell
$env:RUST_LOG = 'audio_selector=debug'
cargo run --locked -- --run-spp 60
```

スリープ／復帰後の自動再接続を確認する場合は、実行時間を長めに指定してセッション確立後にWindowsをスリープさせ、復帰後の切断検出、再接続、PING/PONGをログで確認します。

```powershell
$env:RUST_LOG = 'audio_selector=debug'
cargo run --locked -- --run-spp 600
```

## 長時間試験

`scripts/run-soak.ps1`はRelease版を指定時間実行し、UTF-8のアプリケーションログと一定間隔のWorking Set／Private Memoryを`soak-logs/`へ保存します。8時間試験は次のコマンドで実行します。

```powershell
cargo +1.94.0 build --release --locked
.\scripts\run-soak.ps1 -DurationSeconds 28800 -SampleSeconds 60
```

## 常駐運用

本番用の`--run`は終了時刻を指定せず常駐し、Ctrl+C、Console Close、ログオフ、シャットダウン通知を受けると通信を閉じて正常終了します。名前付きMutexにより同一ユーザーセッションでの多重起動を拒否します。

```powershell
cargo +1.94.0 run --release -- --run
```

Windowsログオン時の自動実行には、スタートアップフォルダーではなくタスクスケジューラを使用します。Release版をビルドしてから登録してください。実行ファイルは`%LOCALAPPDATA%\AudioSelector\bin`へコピーされます。

```powershell
cargo +1.94.0 build --release --locked
.\scripts\install-startup.ps1 -StartNow
```

ログは`%LOCALAPPDATA%\AudioSelector\logs\audio-selector.log`へUTF-8で保存されます。起動時に過去5世代までローテーションします。登録解除ではログとインストール済みファイルを削除しません。

```powershell
.\scripts\uninstall-startup.ps1
```

## Releaseビルド

```powershell
cargo +1.94.0 fmt --all -- --check
cargo +1.94.0 test --locked
cargo +1.94.0 clippy --all-targets -- -D warnings
cargo +1.94.0 build --release --locked
```
