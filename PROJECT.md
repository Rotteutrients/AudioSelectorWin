# Windows Audio Device Selector

## 1. プロジェクト概要

Windows PCの既定オーディオ入出力デバイスを、独立した物理コントローラから切り替える個人用デバイスを開発する。

コントローラは以下を備える。

* ESP32-D系 MCU
* ST7789系 240×320 カラーLCD
* 5ポジションジョグスイッチ

  * UP
  * DOWN
  * LEFT
  * RIGHT
  * CENTER
* Classic Bluetooth SPP通信
* 外部電源

Windows側にはRust製常駐アプリケーションを配置し、Windows Core Audio APIを利用してオーディオEndpointの列挙・状態監視・既定Endpointの変更を行う。

コントローラとWindows間の通常通信にはESP32のClassic Bluetooth SPPを使用する。

ESP32ボード上のCH340Cはファームウェア書き込み・開発・デバッグ用途に限定し、通常運用時の通信には使用しない。

---

# 2. 目的

本デバイスの主目的は、Windowsのサウンド設定画面を開くことなく、物理コントローラから以下を操作できるようにすることである。

* 現在の既定出力デバイスを確認する
* 現在の既定入力デバイスを確認する
* 利用可能な出力デバイスを選択する
* 利用可能な入力デバイスを選択する
* 選択したデバイスをWindowsの既定Endpointとして設定する

入力と出力は独立して選択可能とする。

---

# 3. システム構成

```text
                       Classic Bluetooth
                             SPP
                ┌─────────────────────────┐
                │                         │
                ▼                         ▼

┌─────────────────────────┐     ┌──────────────────────────┐
│ Audio Selector          │     │ Windows PC               │
│                         │     │                          │
│ ┌─────────────────────┐ │     │ ┌──────────────────────┐ │
│ │ ESP32-D             │ │     │ │ Rust Application     │ │
│ │                     │ │     │ │                      │ │
│ │ Bluetooth Classic   │◀┼─────┼▶│ Bluetooth SPP       │ │
│ └──────────┬──────────┘ │     │ │                      │ │
│            │            │     │ │ Core Audio API       │ │
│     ┌──────┴──────┐     │     │ └──────────┬───────────┘ │
│     │             │     │     │            │             │
│    SPI           GPIO   │     │            ▼             │
│     │             │     │     │     Windows Audio        │
│     ▼             ▼     │     │      Endpoints           │
│ ┌────────┐   ┌────────┐ │     │                          │
│ │ST7789  │   │5-pos   │ │     └──────────────────────────┘
│ │240×320 │   │Jog SW  │ │
│ └────────┘   └────────┘ │
│                         │
│ External Power          │
└─────────────────────────┘
```

---

# 4. ハードウェア

## 4.1 MCU

ESP32-D系ボードを使用する。

使用予定ボード：

`https://ja.aliexpress.com/item/1005006456519790.html`

確認済み構成：

* ESP32-D系 MCU
* CH340C USB-UART Bridge

ESP32のClassic Bluetooth機能を通常通信に使用する。

CH340Cは以下の用途に限定する。

* Firmware書き込み
* Serial Debug
* 開発時ログ取得

Windows Audio Selectorとの通常通信にはCH340Cを使用しない。

これにより、PCに接続された他のCH340系Arduino等のシリアルポートを誤ってopen、reset、または操作することを防止する。

---

# 5. 電源

通常運用時は外部電源を使用可能とする。

したがってWindows PCとのUSB接続は必須ではない。

通常運用：

```text
External Power
      │
      ▼
    ESP32
      │
      └── Bluetooth SPP ── Windows
```

USB接続：

```text
USB
 │
 ▼
CH340C
 │
 ▼
ESP32 UART
```

は主にFirmware書き込み・デバッグ時に使用する。

---

# 6. LCD

## 6.1 基本仕様

| 項目          | 仕様                |
| ----------- | ----------------- |
| Controller  | ST7789系           |
| Resolution  | 240 × 320         |
| Orientation | Landscape         |
| Interface   | 4-wire SPI        |
| Display     | Color Graphic LCD |

画面方向は以下とする。

```text
Width  : 320 px
Height : 240 px
```

## 6.2 SPI

基本信号：

* SCLK
* MOSI
* CS
* DC
* RESET
* Backlight

LCDからのデータ読み出しは原則使用しない。

具体的なGPIO割り当てはハードウェア実装時に決定する。

---

# 7. 操作デバイス

5ポジションジョグスイッチを1個使用する。

```text
        UP
         ▲
         │
LEFT ◀── ● ──▶ RIGHT
         │
         ▼
       DOWN

● = CENTER
```

---

# 8. 基本操作

## 8.1 UP / DOWN

操作対象項目を移動する。

Version 1の主要項目：

```text
OUTPUT
INPUT
```

例：

```text
> OUTPUT
  INPUT
```

DOWN：

```text
  OUTPUT
> INPUT
```

---

## 8.2 LEFT / RIGHT

現在フォーカスされている項目について、利用可能なEndpointを順番に選択する。

例：

```text
> OUTPUT

< Speakers (Realtek) >
```

RIGHT：

```text
> OUTPUT

< Headphones (USB DAC) >
```

この時点ではWindowsの設定を変更しない。

---

## 8.3 CENTER

現在表示されている候補Endpointを確定する。

確定後、ESP32からWindowsアプリケーションへ設定要求を送信する。

Windows側で変更に成功した後、ESP32へ最新状態を返す。

## 8.4 入力判定

各入力はLOW有効とし、25 ms連続して同じ状態を検出した時点で押下または解放を確定する。

方向キーは押下確定時に1回だけ入力イベントを発生させる。CENTERは800 ms未満で解放した場合を短押し、800 ms以上継続した場合を長押しとする。長押し成立後の解放では短押しイベントを発生させない。

複数方向の同時押し、および1つのキーを離さず別のキーへ移る操作は無効とする。その後、すべてのキーが解放されるまで新しい入力イベントを受け付けない。起動時にいずれかのキーが押されていた場合も、最初の全解放まで同様に無効とする。

---

# 9. CurrentとCandidate

各Input / Outputについて以下を区別する。

```text
Current
Candidate
```

### Current

現在Windowsで実際に既定Endpointとなっているデバイス。

### Candidate

ジョグスイッチで現在選択中だが、まだ確定していないデバイス。

LEFT / RIGHTではCandidateのみ変更する。

CENTER押下でCandidateをWindowsへ設定要求する。

成功後にCurrentが更新される。

---

# 10. LCD UI

基本画面例：

```text
┌────────────────────────┐
│ AUDIO SELECTOR         │
│                        │
│ > OUTPUT               │
│                        │
│   Headphones           │
│   USB DAC              │
│                        │
│   INPUT                │
│                        │
│   Microphone           │
│   USB Audio            │
│                        │
│                        │
│              CONNECTED │
└────────────────────────┘
```

実際のGUIでは色・フォント・アイコン等を利用して以下を視覚的に区別する。

* フォーカス中の項目
* Current Endpoint
* Candidate Endpoint
* 接続状態
* 未確定状態
* エラー状態

---

# 11. 長いデバイス名

Windows Audio EndpointのFriendly Nameは240px幅を超える可能性がある。

フォーカス中の長い名称についてはHorizontal Marqueeを使用可能とする。

Horizontal Marqueeはフォーカスまたは候補の変更後に最大3回まで実行し、その後は末尾を省略した静止表示とする。フォーカスまたは候補が再度変更された場合は回数をリセットする。

非フォーカス時は省略表示を許可する。

例：

```text
Headphones (USB Audio...)
```

PCからESP32へ送信する文字列はUTF-8とする。

---

# 12. Windowsアプリケーション

## 12.1 実装言語

Rust

## 12.2 動作形態

Windowsバックグラウンド常駐アプリケーションとする。

Version 1ではGUIを必須としない。

将来的にSystem Tray UIを追加可能な設計とする。

---

# 13. Windows Audio

Windows Core Audio APIをRustから直接利用する。

SoundVolumeView等の外部オーディオ制御プログラムには依存しない。

---

# 14. Audio Endpoint

以下のData Flowを扱う。

### Output

```text
eRender
```

### Input

```text
eCapture
```

Windows側で利用可能なActive Endpointを列挙する。

最低限以下の情報を管理する。

```text
Endpoint ID
Friendly Name
Data Flow
Device State
```

内部識別にはFriendly NameではなくEndpoint IDを使用する。

これにより同名デバイスが複数存在しても識別可能とする。

---

# 15. Default Endpoint

本デバイスで選択されたEndpointをWindowsの既定Endpointとして設定する。

Input / Outputは独立して設定する。

---

# 16. Windows Audio Role

Windows Core Audioの以下3 Roleを対象とする。

```text
eConsole
eMultimedia
eCommunications
```

Version 1ではRoleごとの個別設定は行わない。

選択したEndpointを3 Roleすべてへ同時に設定する。

---

# 17. Output変更

例えばUSB DACをOutputとして確定した場合：

```text
eRender / eConsole
    → USB DAC

eRender / eMultimedia
    → USB DAC

eRender / eCommunications
    → USB DAC
```

---

# 18. Input変更

例えばUSB MicrophoneをInputとして確定した場合：

```text
eCapture / eConsole
    → USB Microphone

eCapture / eMultimedia
    → USB Microphone

eCapture / eCommunications
    → USB Microphone
```

---

# 19. Bluetooth通信

## 19.1 Transport

ESP32 Classic BluetoothのSPPを使用する。

```text
ESP32
  │
  │ Bluetooth Classic
  │ SPP
  ▼
Windows
```

Windows側ではSPPによるシリアル通信Transportとして扱う。

---

# 20. Bluetoothデバイス識別

ESP32側Bluetooth Device Name：

```text
AudioSelector
```

を基本とする。

初回使用時にWindowsとペアリングする。

Rustアプリケーションはペアリング済みAudio Selectorを対象として通信する。

通常運用時に、接続されている全物理COMポートへ探索パケットを送信してはならない。

特にCH340 / CP210x / FTDI等の開発用Serial Portを探索目的でopenしない。

---

# 21. CH340Cの扱い

CH340CはAudio Selector通信Transportとして使用しない。

したがってWindows側Audio Selectorアプリケーションは、CH340CのVID/PIDを用いた自動探索を行わない。

これにより、

```text
CH340 Arduino
CH340 Debug Board
CH340 Audio Selector Programming Port
```

等を誤操作する可能性を排除する。

---

# 22. 通信プロトコル

Bluetooth SPP上で独自バイナリプロトコルを使用する。

テキストベースプロトコルは使用しない。

TransportとProtocolを分離する。

```text
┌────────────────────────────┐
│ Audio Selector Application │
├────────────────────────────┤
│ Audio Selector Protocol    │
├────────────────────────────┤
│ Bluetooth SPP              │
└────────────────────────────┘
```

将来Transportを変更してもProtocolおよびApplication Layerへの影響を最小化する。

---

# 23. Binary Frame

基本フレーム形式：

```text
+--------+---------+------+--------+---------+
| Magic  | Version | Type | Length | Payload |
+--------+---------+------+--------+---------+
| 2 byte | 1 byte  |1 byte| 2 byte | N byte  |
+--------+---------+------+--------+---------+
```

基本Header Size：

```text
6 bytes
```

---

# 24. Magic

Audio Selector Protocolを識別する固定値を使用する。

例：

```text
0x41 0x53
```

ASCII：

```text
"A" "S"
```

---

# 25. Protocol Version

初期Version：

```text
0x01
```

互換性のないProtocol変更を行う場合はVersionを変更する。

---

# 26. Length

Payload Lengthを示す。

整数のByte OrderはProtocol全体で統一する。

Version 1ではLittle Endianを基本とする。

最大Payload Lengthは実装時に安全な上限を定義する。

---

# 27. Payload

PayloadはMessage Typeによって異なるBinary Structureを持つ。

文字列フィールドはUTF-8を使用する。

Endpoint IDの完全なWindows内部表現をESP32へ保持させる必要はない。

Windows側がEndpoint IDとProtocol上の一時IDを対応付ける。

---

# 28. Endpoint Handle

同期時にWindows側から各EndpointへSession内で有効なHandleを割り当てる。

例：

```text
Handle 0x01
    → Speakers (Realtek)

Handle 0x02
    → Headphones (USB DAC)

Handle 0x03
    → HDMI Output
```

ESP32はWindows Endpoint IDを認識する必要がない。

ESP32はHandleと表示名のみ保持する。

Endpoint一覧が再構築された場合、Handleは再割り当て可能とする。

---

# 29. Message Type

Version 1では最低限以下を定義する。

```text
HELLO_REQ
HELLO_RESP

SYNC_REQ
SYNC_BEGIN
SYNC_END

OUTPUT_ENDPOINT
INPUT_ENDPOINT

CURRENT_OUTPUT
CURRENT_INPUT

SET_OUTPUT_REQ
SET_INPUT_REQ

SET_RESULT

PING
PONG

ERROR
```

具体的な数値割り当ては`protocol.md`で定義する。

---

# 30. HELLO

接続確立後、Application LevelでProtocol互換性を確認する。

Windows：

```text
HELLO_REQ
```

ESP32：

```text
HELLO_RESP
```

HELLO_RESPには最低限以下を含める。

```text
Protocol Version
Device Type
Firmware Version
```

Device Type：

```text
AUDIO_SELECTOR
```

---

# 31. Full Sync

接続後はWindows側をAuthoritative SourceとしてFull Syncを実施する。

概念：

```text
SYNC_BEGIN

OUTPUT_ENDPOINT
OUTPUT_ENDPOINT
OUTPUT_ENDPOINT

CURRENT_OUTPUT

INPUT_ENDPOINT
INPUT_ENDPOINT

CURRENT_INPUT

SYNC_END
```

SYNC_END受信後にESP32側の一覧を確定する。

---

# 32. Windows is Authoritative

Windows側の状態を常に正とする。

ESP32はWindows Audio設定のMasterにならない。

```text
Windows
   │
   │ authoritative state
   ▼
ESP32
```

ESP32の役割：

* Display
* Physical Input
* Candidate Selection
* User Interface

Windows側の役割：

* Endpoint Management
* Default Endpoint Management
* Endpoint ID Management
* Audio State Monitoring

---

# 33. Endpoint変更処理

Windowsには既定のAudio Endpointを変更する公開MMDeviceメソッドがないため、Windowsデスクトップ版では非公開COMインターフェース`IPolicyConfig::SetDefaultEndpoint`を使用する。OS更新による互換性リスクを考慮し、呼び出し結果だけを成功判定に使用してはならない。3ロールへの設定試行後、公開API`IMMDeviceEnumerator::GetDefaultAudioEndpoint`で実状態を再取得し、3ロールすべてが要求Endpointと一致した場合だけ成功とする。

CENTER押下時：

```text
CENTER
   ↓
ESP32
   ↓
SET_OUTPUT_REQ
or
SET_INPUT_REQ
   ↓
Windows
   ↓
Default Endpoint変更
   ↓
実状態再取得
   ↓
SET_RESULT
   ↓
State Sync
   ↓
LCD更新
```

ESP32側で成功を推測しない。

Windowsから成功状態を受信してからCurrent表示を更新する。

---

# 34. Audio Device Hot Plug

以下の変更を検出する。

* USB DAC接続
* USB DAC切断
* USB Microphone接続
* USB Microphone切断
* Bluetooth Audio Device接続
* Bluetooth Audio Device切断
* HDMI Audio追加
* HDMI Audio削除
* Device State変更

Windows側でEndpoint変更を検出した場合：

```text
Audio Device Change
        ↓
Endpoint Re-enumeration
        ↓
Current Default取得
        ↓
Full Sync
        ↓
LCD Update
```

---

# 35. 外部からのDefault変更

Windows Settingsや別アプリケーションからDefault Endpointが変更された場合もLCD表示を追従させる。

```text
External Change
      ↓
Windows Core Audio Notification
      ↓
Rust Application
      ↓
Current Endpoint Update
      ↓
ESP32 Sync
```

LCDは常にWindowsの実状態を反映する。

---

# 36. Bluetooth切断

Bluetooth接続が切断された場合、Windowsアプリケーションは終了しない。

ESP32：

```text
PC DISCONNECTED
```

Windows：

```text
DEVICE DISCONNECTED
```

状態へ移行する。

自動再接続を試行可能とする。

---

# 37. Sleep / Resume

PCのSleep / Resume後にBluetooth SPPが正常復帰しない可能性を考慮する。

完全な自動復旧をVersion 1の必須条件とはしない。

以下による手動復旧を許容する。

* Reconnect
* ESP32 Restart
* Windows Application Restart

ただし可能な範囲で自動再接続を実装する。

---

# 38. Settings Menu

通常画面から設定メニューへ移動可能とする。

基本操作：

```text
CENTER Long Press
    → Settings
```

メニュー例：

```text
┌────────────────────────┐
│ SETTINGS               │
│                        │
│ > Reconnect            │
│   Restart Device       │
│   Bluetooth Info       │
│   Firmware Info        │
│   Back                 │
│                        │
└────────────────────────┘
```

---

# 39. Reconnect

`Reconnect`を選択した場合、Audio Selector Applicationとの通信セッションを再確立する。

可能な範囲でBluetooth SPP Transportを再初期化する。

ESP32自体の完全再起動は行わない。

---

# 40. Restart Device

`Restart Device`によりESP32をソフトウェアリセットする。

Bluetooth通信が利用できない状態でもローカルUIのみで実行可能でなければならない。

誤操作防止のため確認画面を表示する。

```text
Restart Device?

> Cancel
  Restart
```

`Restart`を確定するとESP32を再起動する。

---

# 41. Bluetooth Info

以下の情報を表示可能とする。

例：

```text
Bluetooth

Name:
AudioSelector

Status:
Connected
```

必要に応じて追加情報を表示する。

---

# 42. Firmware Info

以下を表示可能とする。

```text
Firmware Version
Protocol Version
Device Type
Build Information
```

---

# 43. エラー処理

最低限以下を扱う。

* Bluetooth disconnected
* SPP connection failure
* Windows application unavailable
* Endpoint disappeared
* Invalid Endpoint Handle
* Default Endpoint change failure
* Protocol version mismatch
* Invalid frame
* Invalid payload length
* Unknown message type

---

# 44. Default変更失敗

変更に失敗した場合：

```text
SET_RESULT = FAILED
```

をESP32へ送信する。

LCDには一時的にエラーを表示する。

例：

```text
SET FAILED
```

その後Windows側で実状態を再取得し、Full Syncを行う。

---

# 45. Protocol Parser

Bluetooth SPPはByte Streamとして扱う。

1回のreadが1 Frameと一致することを前提としてはならない。

Parserは以下に対応する。

* Partial Frame
* Multiple Frames in one read
* Invalid Magic
* Invalid Length
* Unknown Message Type
* Connection interruption

Magicを利用してFrame Boundaryを再同期可能な設計とする。

---

# 46. Firmware責務

ESP32 Firmwareは以下を担当する。

* LCD initialization
* LCD rendering
* Jog Switch input
* Switch debounce
* Long press detection
* Bluetooth Classic initialization
* Bluetooth SPP communication
* Binary Protocol parser
* Endpoint display list
* Candidate management
* UI state
* Settings menu
* Connection state
* Device restart

Windows Audio APIの詳細はFirmware側に持たせない。

---

# 47. Windows Application責務

Rustアプリケーションは以下を担当する。

* Bluetooth Audio Selector discovery
* SPP connection
* Reconnection
* Binary Protocol
* Audio Endpoint enumeration
* Default Endpoint retrieval
* Default Endpoint modification
* Audio Endpoint notification
* Default Endpoint notification
* Endpoint ID management
* Endpoint Handle assignment
* Full Sync
* Error handling

---

# 48. Rust Windowsアプリ構成案

```text
windows/
├── Cargo.toml
└── src/
    ├── main.rs
    │
    ├── audio/
    │   ├── mod.rs
    │   ├── endpoint.rs
    │   ├── default.rs
    │   └── notification.rs
    │
    ├── bluetooth/
    │   ├── mod.rs
    │   ├── discovery.rs
    │   └── transport.rs
    │
    ├── protocol/
    │   ├── mod.rs
    │   ├── frame.rs
    │   ├── message.rs
    │   └── parser.rs
    │
    └── state/
        └── mod.rs
```

---

# 49. Firmware構成案

```text
firmware/
├── src/
│   ├── main.cpp
│   ├── display/
│   ├── input/
│   ├── bluetooth/
│   ├── protocol/
│   └── ui/
│
├── include/
│
└── platformio.ini
```

具体的なFrameworkは実装時に決定する。

---

# 50. Repository構成

```text
audio-selector/
│
├── PROJECT.md
│
├── firmware/
│   ├── src/
│   ├── include/
│   └── platformio.ini
│
├── windows/
│   ├── Cargo.toml
│   └── src/
│
├── protocol/
│   └── protocol.md
│
├── hardware/
│   ├── pinout.md
│   └── schematic/
│
└── docs/
    └── ui.md
```

---

# 51. Version 1 Scope

Version 1では以下を実装する。

* ST7789 240×320 LCD表示
* 5ポジションJog Switch操作
* Output Endpoint一覧取得
* Input Endpoint一覧取得
* Output選択
* Input選択
* Current / Candidate分離
* CENTERによる確定
* Default Output Endpoint変更
* Default Input Endpoint変更
* Console / Multimedia / Communications 3 Role同時変更
* Classic Bluetooth SPP通信
* Binary Protocol
* UTF-8 Device Name
* Endpoint Hot Plug
* Default Endpoint変更追従
* Connection Status表示
* Settings Menu
* Reconnect
* ESP32 Restart
* Bluetooth Info
* Firmware Info

---

# 52. Version 1対象外

以下はVersion 1では対象外とする。

* Application別Audio Routing
* Application別Volume
* Master Volume操作
* Microphone Gain操作
* Mute操作
* Communications Role個別選択
* Console Role個別選択
* Multimedia Role個別選択
* Bluetooth Audio Deviceそのものの接続制御
* Wi-Fi通信
* USB CDCによる通常通信
* CH340Cによる通常通信
* 複数Audio Selector同時利用
* Touch Panel
* Firmware OTA
* Windows GUI設定画面

---

# 53. 将来拡張

設計上、以下を将来追加可能とする。

* Master Volume
* Mute
* Microphone Mute
* Audio Level Meter
* Communications Device分離
* Favorite Endpoint
* Device Icon
* Application Audio Routing
* System Tray UI
* Firmware Update
* Wi-Fi Transport
* USB Transport
* BLE対応可能な将来MCUへの移行

通信ProtocolとTransportを分離することで、将来的なTransport変更時の影響を抑える。

---

# 54. 設計原則

## Windows is authoritative

WindowsのAudio状態を正とする。

## ESP32 is a controller

ESP32は表示・物理操作・Candidate管理を担当する。

## Input / Output are independent

入力と出力を独立して選択可能とする。

## Selection requires confirmation

LEFT / RIGHTだけではWindows設定を変更しない。

CENTERで確定する。

## All Roles change together

Version 1ではConsole / Multimedia / Communicationsを同一Endpointへ設定する。

## No serial-port probing

Audio Selector探索のために無関係なCOM Portをopenしない。

## CH340C is development-only

CH340Cは書き込み・デバッグ用途とし、通常通信には使用しない。

## Protocol is transport-independent

Audio Selector ProtocolはBluetooth SPPに依存しない設計とする。

## Fail to actual state

エラー発生時はESP32側の推測状態を維持せず、Windowsの実状態を再取得して同期する。

---

# 55. 実装前確認事項

## Hardware

* [x] ESP32ボードのGPIO割り当て
* [x] ST7789 LCDの7ピンModule仕様
* [x] LCD Logic Voltage確認
* [x] LCD Backlight仕様確認
* [x] SPI Clock決定（40 MHz）
* [ ] Jog Switch型番（実機動作は確認済み、型番記録なし）
* [x] Jog Switch Pull-up / Pull-down設計
* [x] 外部電源仕様（Version 1はUSB Type-C 5 Vのみ）
* [ ] ケース設計（Version 1ソフトウェア範囲外）

## Firmware

* [x] ESP32 Framework選定
* [x] ST7789 Library選定
* [x] Font選定
* [x] 日本語Glyph対応方針
* [x] Bluetooth SPP Library選定
* [x] Binary Protocol実装

## Windows

* [x] Rust Windows API crate構成
* [x] Core Audio Endpoint列挙実装
* [x] Default Endpoint取得実装
* [x] Default Endpoint設定方式の実装・検証
* [x] Endpoint Notification実装
* [x] Bluetooth SPP Device特定方式の実装・検証
* [x] Windows起動時自動実行方式

## Protocol

* [x] Message Type数値割り当て
* [x] Payload Structure
* [x] Maximum Frame Size
* [x] Maximum Endpoint Count
* [x] Maximum Friendly Name Length
* [x] Error Code定義
* [x] Timeout定義
* [x] PING/PONG Interval
* [x] Handle Lifetime
* [x] Protocol Version negotiation
* [x] 不正Frameからの再同期方式

---

# 56. 最終構成

Version 1の基本構成を以下とする。

```text
ESP32-D
  │
  ├── ST7789
  │     240×320
  │     Landscape
  │     4-wire SPI
  │
  ├── 5-position Jog Switch
  │     UP/DOWN    : Item
  │     LEFT/RIGHT : Endpoint Candidate
  │     CENTER     : Confirm
  │
  ├── Classic Bluetooth SPP
  │          │
  │          │ Binary Protocol
  │          ▼
  │       Windows
  │          │
  │          ▼
  │     Rust Application
  │          │
  │          ▼
  │     Core Audio API
  │          │
  │      ┌───┴────┐
  │      ▼        ▼
  │    Render   Capture
  │      │        │
  │      ▼        ▼
  │    Output    Input
  │
  └── CH340C
        │
        └── Firmware / Debug only
```

この構成をVersion 1の基準仕様とする。
