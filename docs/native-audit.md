# Kiểm kê tương thích CUTTOOL trên Windows

## Vòng đời ứng dụng

- Electron giữ một phiên ứng dụng. Mở lần hai khôi phục, hiện và focus cửa sổ cũ. Cửa sổ chính mở `db.html`, có kích thước ban đầu 1280 × 768, nền `#090d16` và bỏ menu mặc định. Nút X ẩn xuống khay hệ thống. Click, double-click hoặc chọn **Mở ứng dụng** trong khay để hiện lại. **Thoát hoàn toàn** dọn khay và dừng backend. Nguồn `src/UI/main.js#L40-L118`, `#L353-L456`.
- Bản dev chạy `pyw` với `src/macro/main.py`. Bản đóng gói chạy `resources/Cryss.exe`. Cửa sổ hiện khi probe `GET /config` thành công. Sau timeout 30 giây, ứng dụng hiện hộp thoại lỗi rồi vẫn hiện cửa sổ. Khi thoát, shell gửi `POST /shutdown` và có nhánh dọn tiến trình con. Bản Electron Windows khai báo `requireAdministrator`, backend Python có nhánh yêu cầu nâng quyền khi chạy trực tiếp. Nguồn `src/UI/main.js#L122-L243`, `#L406-L428`, `src/UI/package.json#L18-L42`, `src/macro/main.py#L20-L41`.
- Backend hiện tại nạp FPS và binding trước khi khởi động listener bàn phím, chuột và HTTP loopback. Các thao tác lưu, bật/tắt macro và thoát đi qua backend; runtime theo dõi cửa sổ foreground và trạng thái phím. Native app cần giữ các trạng thái khởi động, sẵn sàng và dừng qua giao diện trong tiến trình, đồng thời kiểm chứng dừng macro và nhả đầu vào khi đổi focus, bỏ binding hoặc thoát. Nguồn `src/macro/main.py#L51-L75`, `src/macro/core/server.py#L130-L181`, `#L216-L223`, `#L257-L265`, `src/macro/core/runtime.py#L506-L630`, `src/macro/core/focus.py#L21-L95`.

## Kho dữ liệu và đường dẫn

| Kho | Dữ liệu và cách dùng | Nguồn |
| --- | --- | --- |
| Macro JSON | Dev đọc `src/macro/config.json`; bản frozen đọc `config.json` cạnh EXE backend, tức trong `resources` của gói Electron. Mặc định bản nguồn có `comboSignKeys` rỗng, `FPS` 120, `customCombos` rỗng. | `src/macro/core/config_io.py#L19-L38`, `#L78-L99`, `src/macro/config.json`, `src/UI/main.js#L165-L177` |
| Timing JSON | `mavuika.json` nằm cạnh config, được đóng gói vào `resources`. Backend cũ tải lại khi mtime đổi, giữ bản hợp lệ đang dùng hoặc bản tích hợp khi đọc lỗi. | `src/macro/core/macros.py#L303-L337`, `#L540-L565`, `src/UI/package.json#L36-L41` |
| Renderer localStorage | `comboSignKeys`, `customCombos` và `FPS` hỗ trợ lưu hoặc đọc lại khi backend gián đoạn. `custom_hero_banner` chứa data URL, `macro_run_active` chứa trạng thái nút RUN, `comboClipboard` và `comboTimelineZoom` dùng trong editor nâng cao. `clean_state_v1_1_3` đánh dấu lần dọn localStorage của renderer cũ. | `src/UI/db.js#L60-L115`, `#L445-L523`, `src/UI/cb.js#L40-L74`, `src/UI/shared.js#L1-L8`, `#L118-L121`, `src/UI/cuscombo-advanced.js#L80-L84`, `#L865-L925`, `#L1026` |
| Renderer sessionStorage | `cuscombo_draft` mang bản nháp khi đổi editor, gồm tên, hotkey, timeline và cờ mở import/export khi có. | `src/UI/cuscombo.js#L81-L103`, `#L562-L578`, `src/UI/cuscombo-advanced.js#L362-L396`, `#L2563-L2584` |
| Nhật ký và updater | Electron dùng `app.getPath('userData')` cho `electron-debug.log`, `updater.log` và `updates/{download,staged,backup}`. Updater có fallback `%APPDATA%\Cryss`. Python đặt `debug.log` cạnh config; hộp thoại timeout của shell hiển thị thêm các đường dẫn dưới `%APPDATA%\Cryss`. | `src/UI/main.js#L25-L37`, `#L412-L419`, `src/UI/updater.js#L47-L94`, `src/macro/core/config_io.py#L37-L62` |
| Dữ liệu tích hợp bên ngoài | Shell có kho riêng `gamepath.json` và `Plugins/UnlockerIsland/config.ini` trong cây tài nguyên unlocker. Updater cũ loại các tên tệp cấu hình này và `config.json` khi ghi đè. | `src/UI/unlocker.js#L5-L74`, `src/UI/updater.js#L562-L587` |

Native app cần đọc dữ liệu legacy từ vị trí thực tế, giữ thứ tự binding khi có xung đột và giữ trường JSON chưa biết. Các mục lưu riêng trong Chromium cần kế hoạch chuyển đổi hoặc nhập dữ liệu có kiểm thử. Tệp hỏng cần báo lỗi và giữ nội dung để người dùng khôi phục. Nguồn `src/macro/core/runtime.py#L508-L558`, `src/UI/cb.js#L40-L74`, `native/cryss/src/config.rs`.

## Giao diện và trình chỉnh sửa

- Entry point thực tế là `db.html`. Ba tab **Home**, **Macros**, **Settings** nằm trong cùng trang và chọn qua hash. CSS, Tailwind script, font và icon tải từ tài nguyên đi kèm. `shared.js` điều phối tab và nút RUN; `db.js` lo trạng thái backend, banner, FPS và updater; `cb.js` lo danh sách macro và binding; `cus.js` lo thiết lập tích hợp. Nguồn `src/UI/db.html#L8-L14`, `#L86-L130`, `#L550`, `#L710`, `#L1093-L1098`, `src/UI/shared.js#L47-L115`.
- Modal tùy chỉnh chứa iframe. Editor cơ bản có palette kéo thả, timeline tuần tự, hotkey và chỉnh sửa theo ID. Editor nâng cao có timeline nhiều track, zoom/snap, chỉnh clip, undo/redo, sao chép/dán, ghi/phát xem trước, loop và import/export văn bản. Bản cơ bản lưu `id/name/hotkey/timeline/pythonSequence`; bản nâng cao thêm `clips` và `loop` để mở lại đúng mô hình editor. Hai dạng cần mở, lưu lại và chuyển chế độ mà không mất dữ liệu. Nguồn `src/UI/db.html#L678-L705`, `src/UI/cb.js#L345-L388`, `src/UI/cuscombo.js#L105-L186`, `#L488-L578`, `src/UI/cuscombo-advanced.js#L401-L500`, `#L515-L698`, `#L2396-L2452`, `#L2905`, `#L3304`, `#L3599-L3751`.
- Bridge `preload.js` đưa lệnh IPC của shell vào renderer, còn dữ liệu macro đi qua HTTP localhost. Desktop Rust cần các luồng tương đương cho chọn ảnh banner, mở link được phép, thông báo lỗi, cập nhật dữ liệu và phản hồi thao tác, theo runtime đóng gói đã chọn. Các lệnh liên quan tích hợp bên ngoài nằm ngoài phạm vi được kiểm chứng. Nguồn `src/UI/preload.js`, `src/UI/main.js#L270-L351`, `src/UI/cb.js#L40-L74`.

## Cập nhật và tích hợp bên ngoài

- Updater đọc `version.json`, kiểm tra GitHub Releases và ưu tiên asset `.rar` hoặc `.zip`. Renderer tự kiểm tra sau 3 giây và có luồng thủ công. Download xử lý chuyển hướng, hiển thị tiến trình và hỗ trợ hủy. Kiểm tra staging hiện dựa trên tệp có tồn tại, lớn hơn 1 MiB và có `Cryss.exe` ở gốc hoặc một cấp con. Mã này chưa kiểm tra hash hoặc chữ ký. Nguồn `src/UI/updater.js#L111-L215`, `#L235-L375`, `#L376-L510`, `src/UI/db.js#L158-L442`.
- Chế độ dev dừng ở staging. Bản đóng gói tạo backup, thay tệp cài đặt, giữ cấu hình, rollback khi sao chép lỗi hoặc thiếu EXE và khởi động lại. Nếu port updater, dùng gói phát hành dành cho bản native và kiểm thử toàn bộ luồng bằng release fixture cùng thư mục cài đặt cô lập. Script cũ loại `config.json`, `gamepath.json`, `config.ini` khỏi thao tác sao chép; `mavuika.json` không có trong danh sách loại trừ. Nguồn `src/UI/updater.js#L515-L606`, `#L616-L677`.
- Binary của tích hợp unlocker không có trong Git. `.gitignore` loại `unlocker/` và cấu hình đóng gói tham chiếu thư mục ngoài này. Không tải, chạy hoặc port chức năng injection trong đợt kiểm kê. Giao diện Settings có thể đối chiếu trực quan với fixture, nhưng khả năng chạy tích hợp và tương thích binary chưa xác minh. Nguồn `.gitignore`, `src/UI/package.json#L29-L41`, `README.md#L11`.

## Bằng chứng và ma trận kiểm thử

Harness baseline dùng Electron 43.1.0 để chụp 12 trạng thái gồm Home, Macros, Settings, hai editor, modal editor nhúng, bản tin, cập nhật, gán phím, xóa combo, phím mạng và import/export. Harness dùng hồ sơ Chromium riêng, HTTP loopback giả lập và IPC cố định, chặn request ngoài loopback. Hai editor được mở trực tiếp theo URL nội bộ. Kích thước cửa sổ cấu hình là 1280 × 768; manifest ghi mỗi PNG là 2242 × 1348. Ảnh xác nhận renderer trong phiên fixture. Shell, backend, thao tác cập nhật và tích hợp chạy đầy đủ vẫn cần kiểm thử. Nguồn kiểm chứng `work/cuttool-ui-baseline/capture.cjs` và `captures/manifest.json` trong workspace kiểm kê.

| Hạng mục | Kiểm tra cần có để xác nhận parity |
| --- | --- |
| Cửa sổ và tray | Khởi động sạch, phiên thứ hai focus phiên đầu, nút X ẩn xuống tray, mở từ tray, thoát hoàn toàn và lỗi khởi tạo được hiện đúng. |
| Layout | Đối chiếu các màn hình và modal cùng viewport và DPI; kiểm tra resize, font/icon cục bộ, cuộn, focus bàn phím, modal nhúng và dấu tiếng Việt. |
| Lưu và migration | Fixture JSON cũ có binding trùng, dữ liệu Unicode và trường lạ; localStorage có banner/zoom; cấu hình thiếu, hỏng hoặc bị khóa; khởi động lại và đối chiếu dữ liệu. |
| Macro và editor | Kiểm tra tạo/sửa/xóa, gán và bỏ hotkey, draft qua hai chế độ, import/export, lịch event và loop. Dùng input giả lập để đối chiếu cancellation, focus, nhả đầu vào, thứ tự sự kiện và timing. |
| Updater | Fixture cho update/no-update, lỗi tải, hủy, archive sai, backup, giữ cấu hình, rollback và relaunch trong thư mục cài đặt cô lập. |
| Phân phối và hiệu năng | Chạy gói native trên Windows sạch; kiểm tra khởi động, khai báo quyền, đường dẫn dữ liệu, bộ nhớ và CPU idle, độ trễ đầu vào, timing và kích thước gói. Đo full app riêng với phép đo bộ timing hoặc CLI. |
| Binary bên ngoài | Không có bằng chứng tương thích runtime. Giữ nguyên giới hạn xác minh của các fixture Settings. |

`native/cryss/src/main.rs` hiện công bố `check-config`, `trace` và `benchmark`; `native/cryss/src/engine.rs` nhận abstraction đầu ra. Các điểm vào này hỗ trợ kiểm tra cấu hình, chương trình và timing. Chúng chưa cung cấp cửa sổ, tray, phiên ứng dụng Windows hay editor desktop. `docs/rust-timing.md` mô tả benchmark của thư viện timing, với phạm vi đo riêng.
