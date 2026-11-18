# Lõi CUTTOOL Rust Native trên Windows

Workspace Rust ở thư mục gốc. `native/cryss` chứa executable console và thư viện lõi. Thư viện dùng chung dữ liệu JSON cũ, biên dịch preset và macro tự tạo thành lệnh định thời, quản lý phím/nút đang được giữ và chạy trên worker có thể hủy. Mọi lệnh trong executable chạy bằng Rust. `trace` phát bản ghi JSON để kiểm tra thứ tự, thời điểm và hủy trong môi trường cô lập.

## Build và kiểm thử

Dùng Rust stable với toolchain MSVC x64 và Windows SDK. Từ thư mục repo, chạy các lệnh PowerShell sau.

```powershell
cargo fmt --all --check
cargo clippy --all-targets --locked --target-dir build/rust-native -- -D warnings
cargo test --release --locked --target-dir build/rust-native
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked --target-dir build/rust-native-static
Remove-Item Env:RUSTFLAGS
```

Executable nằm tại `build/rust-native-static/release/cryss-native.exe`. Các lệnh `check-config` và `trace` chỉ đọc tệp cấu hình đã chọn.

```powershell
$nativeExe = './build/rust-native-static/release/cryss-native.exe'
& $nativeExe check-config src/macro/config.json
& $nativeExe trace src/macro/config.json skk2as 1500
& $nativeExe benchmark
```

`ConfigStore` bảo toàn trường JSON chưa biết và mã hóa UTF-8 không BOM. Nó sử dụng thay thế tệp nguyên tử khi ghi, từ chối ghi đè cấu hình không đọc được và dùng dữ liệu `mavuika.json` đóng sẵn nếu tệp bên cạnh bị thiếu. Bản sao lưu khi nâng cấp vẫn là trách nhiệm của quy trình phát hành ứng dụng, hiện chưa triển khai.

## Kết quả benchmark timer trên Windows x64

So sánh Python 3.14.5 với Rust 1.98.0 release. Mỗi lượt có 360 lần chờ theo chu kỳ 2, 8 và 16 ms. Ba cặp chạy lần lượt theo thứ tự Python→Rust, Rust→Python và Python→Rust. CPU là thời gian tiến trình sử dụng CPU. Sai số là thời gian vượt deadline theo đồng hồ đơn điệu. Timer Rust khởi động 24 lần chờ trước khi lấy mẫu.

| Lượt | CPU Python (ms) | CPU Rust (ms) | p99 Python (ms) | p99 Rust (ms) |
| --- | ---: | ---: | ---: | ---: |
| 1 | 828,125 | 0,000 | 0,3048 | 0,2277 |
| 2 | 671,875 | 140,625 | 0,2813 | 0,1011 |
| 3 | 500,000 | 15,625 | 0,1768 | 0,1751 |
| Trung vị | 671,875 | 15,625 | 0,2813 | 0,1751 |

Trung vị p50 Python/Rust là 0,0020/0,0014 ms, p95 là 0,0498/0,0401 ms. Sai số lớn nhất quan sát được là 0,7379/1,2001 ms. Wall-clock trung vị lần lượt là 3.126,147/3.123,534 ms. CPU báo 0 ở lượt đầu Rust có nghĩa bộ đếm chưa ghi nhận thêm CPU trong lượt đó.

Waitable timer thuần đạt p99 trung vị 1,612 ms trong vòng đo đầu. Biến thể spin tối đa 750 µs đạt 0,970 ms. Mỗi biến thể được đo ba lượt trên cùng máy. Các loạt đo có nhiễu lập lịch, nên không thể quy toàn bộ chênh lệch cho giới hạn spin. Bản hiện tại ngủ theo waitable timer và giới hạn spin ở phần thời gian còn lại, tối đa 1,25 ms. Worker kiểm tra tín hiệu hủy ngay trong hai giai đoạn. Thiết lập này không đổi timer resolution toàn hệ thống và dùng cùng deadline Rust qua toàn bộ phép đo. So với kết quả cũ của DLL, thiết kế mới cũng bỏ đoạn truyền thời gian còn lại từ Python sang DLL.

Trong ba lượt Rust, p99 thức dậy sau tín hiệu hủy nằm trong khoảng 0,069–0,098 ms. Sau 5.000 mốc tuyệt đối, sai số mốc cuối là 0,027–0,433 ms, trong khi mốc có sai số lớn nhất đạt 9,519 ms. Mỗi tiến trình có 77 handles cả trước lẫn sau khi thả timer và tín hiệu. Working set kết thúc khoảng 3,75–3,78 MiB. Đây là phép đo riêng timer, chưa đo CPU/RAM lúc idle của ứng dụng desktop hoặc ổn định chạy nhiều giờ.

Chạy baseline Python khi tái lập số liệu bằng `python tests/timing_compat.py --benchmark python`. Python ở đây dùng để đo bản cũ trong môi trường phát triển.

## Phạm vi hiện tại

- Lõi đã có macro Skirk, các preset Mavuika và Arlecchino dựa trên JSON, macro tự tạo, phân giải hotkey theo cấu hình cũ, deadline, worker và cơ chế báo dừng. Đầu ra hiện được kiểm chứng bằng trace. Chưa nối luồng sự kiện bàn phím và chuột Windows thật, focus hoặc tray vào ứng dụng.
- UI desktop, cả hai editor, hồ sơ Chromium, cập nhật và vòng đời ứng dụng Windows vẫn cần triển khai. Bộ ảnh Electron là mốc đối chiếu cho renderer bằng dữ liệu kiểm thử cô lập, chưa có ảnh phía UI native để so sánh. Xem [kiểm kê tương thích](native-audit.md).
- Repo thiếu mã nguồn của launcher và DLL unlocker được ứng dụng cũ tham chiếu. Parity của phần tích hợp đó chưa được kiểm chứng.
- Không có đường fallback Python trong executable Rust. Bản Electron/Python cũ nằm trong checkout để đối chiếu và khôi phục từ lịch sử Git. Chưa có gói desktop native đủ tính năng để phát hành.

## Quyền quản trị

Bản `release` của `native/desktop` nhúng manifest `requireAdministrator` như bản Electron, vì macro chỉ nhận phím và chuột khi game chạy quyền admin nếu chính nó cũng chạy quyền admin. Bản `debug` dùng `asInvoker` để chạy và kiểm thử không bị UAC chặn. `build.rs` chọn theo `PROFILE`. App tự kiểm tra token khi khởi động và báo trên màn hình, trong panel Trạng thái, nếu đang chạy quyền thường.

```powershell
cargo build --release -p cryss-desktop --target-dir build/rust-native
```
