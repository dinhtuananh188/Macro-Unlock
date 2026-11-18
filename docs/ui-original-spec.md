# Số đo giao diện gốc (Electron `src/UI/db.html`) ở cửa sổ 1280 × 768

Đo trực tiếp bằng `getBoundingClientRect` và `getComputedStyle` trên trang đang chạy. App Rust phải bám các số này, không diễn giải lại.

## Token
- Nền trang `#090d16`, chữ `#e7edf8`, chữ phụ `#8fa0ba`, teal `#2dd4bf`, tím `#a78bfa`.
- Sidebar nền `#070b12`. Thẻ `#131b2b`, viền `rgba(148,163,184,.4)` dày ~0,57 px, bo 12 px, padding 20 px.
- Nhãn đang chọn của sidebar nền `#1a2436`, chữ teal.
- Font: Hanken Grotesk (`src/UI/assets/vendor/hanken-{400,500,600,700}.ttf`), icon Material Symbols (`material-symbols.ttf`); nhãn nút dùng `Monsterat.ttf`.

## Khung app
| Phần | Số đo |
| --- | --- |
| Sidebar | rộng 280, cao 768 |
| Logo | 144 × 144, tâm ngang, cách đỉnh 24; chữ "Cứt tool" dưới logo |
| Mục điều hướng | rộng 263 (lề 8), cao 48, padding 12 16, gap 12, chữ 16 px; mục đang chọn nền `#1a2436` + vạch teal trái |
| RUN Macro | x 16, y 652, rộng 247, cao 44, bo 12, chữ 13/600, nền teal 7 %, viền teal 45 %, chữ teal 85 % |
| START Game | y 704, cùng cỡ, màu tím `rgb(124,58,237)` 7 % nền, viền 45 %, chữ `#a78bfa` 85 % |
| Vùng nội dung | x 280, rộng 1000, padding 40 |

## Macros
- Tiêu đề "Danh sách Combo" 16/700 ở (320, 41), mô tả xám bên dưới; nút **Reload** (viền) và **Thêm Combo** (nền teal) ở góc phải.
- Lưới 3 cột, gap 24, bắt đầu y 138, mỗi thẻ 291 × 188.
- Trong thẻ: badge "Mặc định" (11/500, nền xanh dương 10 %, chữ `#60a5fa`, bo 4, padding 2 8); pill "Chưa gán phím" (12 mono, nền trắng 5 %, bo 6, padding 4 12); tên "Combo N: …" 16/600 tối đa 2 dòng; nút **Gán phím** (12/500, nền teal 10 %, viền teal 30 %, bo 8, padding 6 12, icon 14 px).

## Trang chủ
Banner ảnh `assets/defbanner.png` phủ trang, chữ "CutTool" cỡ lớn kèm badge phiên bản; chuông cập nhật và "Đổi ảnh bìa" ở góc trên phải; thẻ tin cập nhật bên trái dưới; hàng nút **Combo, Trạng thái, FPS, Game** ở góc dưới phải mở panel nội dung (danh sách combo đã gán, FPS có nút lưu, trạng thái backend, ô Unlocker và đường dẫn game).

## Tùy chỉnh (config.ini của Unlocker + `gamepath.json`)
Lưới công tắc hai cột, nút **Lưu cấu hình** góc phải. Các khóa, mặc định theo `unlocker.js`: `Vsync` 0, `FpsUnlock` 0, `TargetFps` 120 (thanh trượt), `FovUnlock` 0, `FovValue` 60 (thanh trượt), `HideUID`, `DisableCameraMove`, `DisableFog`, `RemoveTeamAnim`, `DisableBurstBlackscreen`, `ShowFPS`, `BlockNetwork`, `EnableNetworkToggle`, `NetworkToggleKey` 122, `ToggleKey` 36, `GamePath`. Ghi dạng `[Khóa]\r\nValue=…`, giữ khóa lạ.

## Dữ liệu macro (không được làm mất)
`src/macro/config.json` (`FPS`, `comboSignKeys`, `customCombos`, trường lạ) và `src/macro/mavuika.json` (timing combo Mavuika). `ConfigStore` của `native/cryss` đã bảo toàn trường lạ và dùng bản đóng gói khi thiếu tệp; mọi chỉnh sửa UI phải đi qua nó, không ghi đè tay.
