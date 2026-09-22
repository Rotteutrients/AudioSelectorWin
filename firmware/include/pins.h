#pragma once

namespace audio_selector::pins {

// ST7789 LCD (write-only SPI)
inline constexpr int kLcdSclk = 18;
inline constexpr int kLcdMosi = 23;
inline constexpr int kLcdCs = 16;
inline constexpr int kLcdDc = 21;
inline constexpr int kLcdReset = 22;

// 5-position jog switch (active low with internal pull-up)
inline constexpr int kJogUp = 25;
inline constexpr int kJogDown = 26;
inline constexpr int kJogLeft = 27;
inline constexpr int kJogRight = 33;
inline constexpr int kJogCenter = 13;

}  // namespace audio_selector::pins

