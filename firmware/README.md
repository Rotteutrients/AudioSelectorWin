# Firmware

## 採用構成

- PlatformIO
- Arduino Framework
- LovyanGFX
- Arduino BluetoothSerial（Bluetooth Classic SPP）
- Generic ESP32 Dev Module（`esp32dev`）

Versionは`platformio.ini`で固定しています。LCDとジョグスイッチの配線は[`../hardware/pinout.md`](../hardware/pinout.md)を参照してください。

日本語フォントとBluetooth Classicスタックを同時に収容するため、4 MB Flashの`huge_app`パーティションを使用します。OTA用の第2アプリ領域は使用しません。

## ビルド

```powershell
pio run
```

## 書き込み

CH340C USB端子を接続して実行します。

```powershell
pio run --target upload
```

通常運用時の通信にCH340Cは使用しません。

Espressif Flash Download Toolで結合済みイメージを書き込む場合は、次のファイルをアドレス`0x0000`へ指定します。

```text
.pio/build/esp32dev/audio-selector-full.bin
```

結合イメージは次のコマンドで生成します。

```powershell
pio pkg exec --package tool-esptoolpy -- esptool.py --chip esp32 merge_bin -o .pio/build/esp32dev/audio-selector-full.bin 0x1000 .pio/build/esp32dev/bootloader.bin 0x8000 .pio/build/esp32dev/partitions.bin 0xe000 .pio/build/esp32dev/boot_app0.bin 0x10000 .pio/build/esp32dev/firmware.bin
```

バージョン1のReleaseファームウェアは`0.1.0`、プロトコルバージョンは1です。

## 初回ペアリング

1. ESP32をUSB Type-C 5 Vで起動する。
2. Windowsの「Bluetoothとデバイス」からデバイスを追加する。
3. `AudioSelector`を選び、Windowsの案内に従ってペアリングを完了する。
4. Windowsアプリで`--find-bluetooth`を実行し、`authenticated=true`かつ`spp=true`を確認する。
5. Windowsアプリを`--run`で起動し、LCDに出力／入力一覧が表示されることを確認する。

固定BluetoothアドレスやCOMポート番号は保存しません。ペアリング時に固定PINは設定していません。

## 復旧

接続が復旧しない場合は、次の順に実施します。

1. AudioSelector設定画面の`Reconnect`
2. AudioSelector設定画面の`Restart Device`
3. Windowsアプリの再起動
4. WindowsのBluetooth設定で`AudioSelector`を削除し、初回ペアリングをやり直す

通常運用の電源はUSB Type-C 5 Vだけを使用します。`VN`端子へは電源を接続しません。
