# Audio Selector

ESP32の物理コントローラーから、Windowsの既定オーディオ入出力エンドポイントを切り替えるプロジェクトです。

仕様は[PROJECT.md](./PROJECT.md)、実装手順と進捗は[todo.md](./todo.md)を参照してください。

## 必要な開発環境

### Firmware

- PlatformIO Core 6系
- Espressif 32 Platform 7.0.0
- Arduino Framework
- LovyanGFX 1.2.29

### Windowsアプリケーション

- Rust 1.94.0（`rust-toolchain.toml`で固定）
- Windows 10またはWindows 11

## ビルド

### Firmware

```powershell
Set-Location firmware
pio run
```

Firmwareを書き込む場合だけESP32のCH340C USB端子を使用します。

```powershell
pio run --target upload
```

### Windowsアプリケーション

```powershell
Set-Location windows
cargo build --locked
cargo test --locked
```

## 通常通信

通常運用時の通信はBluetooth Classic SPPだけを使用します。Windowsアプリケーションは、Audio Selectorを探すために無関係なCOMポートを開きません。

初回ペアリング、ファームウェア書き込み、復旧は[firmware/README.md](./firmware/README.md)を参照してください。Windows常駐アプリのReleaseビルド、自動起動、ログ、解除手順は[windows/README.md](./windows/README.md)を参照してください。

バージョン1のESP32電源はUSB Type-C 5 Vのみです。ボードの`VN`端子は使用しません。

## バージョン1の既知事項

- 8時間連続動作、100回のEndpoint切り替え、50回のBluetooth切断・再接続はユーザー判断で試験をSkipしています。1時間連続動作と個別の切断・復旧経路は確認済みです。
- ジョグスイッチの電気動作と配線は確認済みですが、部品型番は記録されていません。
- ケース設計、`VN`端子給電、OTA、Windows GUI、複数台同時利用はバージョン1の対象外です。
