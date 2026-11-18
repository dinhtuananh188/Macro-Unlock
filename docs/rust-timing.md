# Bộ đếm thời gian Rust

`native/timing` thay phần vòng lặp chờ của `wait_exact()` khi bật `CRYSS_RUST_TIMING=1`. Thư viện không có dependency ngoài thư viện chuẩn Rust. Python tính thời gian còn lại theo `perf_counter()` và truyền số giây qua C ABI. Rust dùng đồng hồ đơn điệu, ngủ tối đa 1 ms mỗi lần và quay chờ khi còn tối đa 1,75 ms.

Windows và Linux dùng cùng thư viện Rust. Các bảng FPS và cách nội suy, thiết bị nhập, logic hủy macro, cấu hình, HTTP loopback và Electron vẫn dùng mã hiện hành. Phạm vi migration dừng ở bộ chờ dùng chung, nơi có thể đo CPU và sai số trực tiếp. Dự án không có máy chủ HTTP bên ngoài hoặc cơ sở dữ liệu.

## Build và chạy

Cần Rust stable từ 1.85, linker cho nền tảng đích và các phụ thuộc Python, Electron đã ghi trong README. Trên Windows, kiến trúc của Rust và Python phải khớp nhau. Từ thư mục repo, build release và chạy các kiểm tra độc lập với game:

```sh
cargo build --release --locked --manifest-path native/timing/Cargo.toml --target-dir build/native
cargo test --release --locked --manifest-path native/timing/Cargo.toml --target-dir build/native
python tests/timing_compat.py
python tests/timing_compat.py --benchmark both
```

- **Windows** — `build.bat` đóng gói DLL vào backend PyInstaller trước khi đóng gói Electron. Trong PowerShell chạy với quyền quản trị, đặt `$env:CRYSS_RUST_TIMING = '1'` trước khi chạy app hoặc `pnpm start` từ `src/UI`.
- **Linux** — `bash build-linux.sh` đóng gói `.so` vào `build/dist/Cryss`. Chạy `CRYSS_RUST_TIMING=1 ./build/dist/Cryss` khi tài khoản có quyền thiết bị nhập. Nếu cần sudo để chạy gói, dùng `sudo CRYSS_RUST_TIMING=1 ./build/dist/Cryss`. Với script dev, đặt biến trước khi chạy `src-linux/run.sh` khi tài khoản đang có quyền thiết bị, vì nhánh tự gọi sudo của script có thể bỏ biến môi trường.
- **Quay về Python** — bỏ biến `CRYSS_RUST_TIMING` và khởi động lại app. Python là mặc định. Khi bật Rust mà thư viện thiếu hoặc không tải được, backend tự dùng Python. Hai chế độ dùng cùng dữ liệu cấu hình.

Để phân phối, dùng đầu ra của script đóng gói tương ứng với nền tảng đích. PyInstaller mang thư viện Rust trong backend, nên máy chạy bản đóng gói không cần cài Rust.

## Benchmark trên Windows x64

Python 3.14.5 và Rust 1.98.0. Mỗi chế độ chạy 360 lần chờ gồm 120 mẫu cho từng mốc 2, 8 và 16 ms. Đo thời gian CPU của tiến trình và sai số giữa thời điểm trả về với deadline yêu cầu. Chạy Python rồi Rust trong cùng tiến trình cho mỗi cặp. Có thể thêm `--rust-first` để đo theo thứ tự ngược lại. Kết quả chỉ đại diện cho bộ chờ chạy trên một luồng. Thời gian luồng bị hệ điều hành tạm dừng giữa lúc Python tính phần còn lại và lúc Rust bắt đầu có thể làm tăng sai số deadline.

| Lượt | CPU Python (giây) | CPU Rust (giây) | p99 Python (ms) | p99 Rust (ms) |
| --- | ---: | ---: | ---: | ---: |
| 1 | 0,406250 | 0,234375 | 0,1438 | 0,0345 |
| 2 | 0,343750 | 0,468750 | 0,1726 | 1,0332 |
| 3 | 0,437500 | 0,406250 | 0,5269 | 0,8534 |
| Trung vị | 0,406250 | 0,406250 | 0,1726 | 0,8534 |

Kết quả thay đổi giữa các lượt. Trung vị CPU bằng nhau và trung vị p99 cao hơn ở Rust. Vì vậy Rust ở chế độ bật chủ động. Kích thước DLL release là 121.856 byte và backend Cryss Windows là 10.109.827 byte. Kích thước backend gốc, API latency, throughput, RAM, startup, kích thước gói Electron, tải frontend và Linux đều **Not measured**.

## Phạm vi kiểm chứng

- Đã build DLL release và chạy test native trên Windows. Test tương thích gọi cả hai module Python trong checkout, kiểm tra deadline, chờ đồng thời, rollback, thiếu DLL và kết quả nội suy FPS.
- Build backend Cryss bằng PyInstaller trên Windows thành công. Đã kiểm tra EXE chứa DLL, module timing và HTTP handler. Smoke test PyInstaller `onefile` độc lập đã nạp DLL được đóng gói và chờ đúng deadline. `bash -n` kiểm tra cú pháp build Linux. Máy thử chưa có môi trường chạy Linux.
- Git xác nhận các tệp Electron, HTTP handler và cấu hình trùng baseline `3030b94`. Chưa chạy visual regression hoặc E2E trong game.
