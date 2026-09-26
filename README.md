# **Skirk Macro**

## Tính năng

- Giao diện người dùng dễ tiếp cận
- Combo dựng sẵn, gán phím tắt là dùng được:
  - Skirk C0: EQA 120fps, EA 120fps, EQA 60fps (độ trễ tự chỉnh theo FPS nhập vào)
  - Mavuika CDCDCF (Full Combo)
  - Arlecchino Overload N2W (lấy từ [GI-Macro-Manager](https://github.com/3azf55/GI-Macro-Manager), MIT)
- Trình tạo combo dạng timeline nhiều track (kiểu Premiere):
  - Kéo thả clip phím / chuột / khối Skirk lên các track, bấm nhiều nút cùng lúc bằng cách xếp chồng clip
  - Kéo mép clip để chỉnh thời gian giữ, đặt vùng lặp trên thước thời gian
  - Khối Skirk có thời lượng cố định theo FPS
- Hỗ trợ tạo nhiều combo custom và mỗi combo 1 phím tắt
- Timing combo Mavuika nằm trong `mavuika.json` cạnh `config.json`, sửa file là app đọc lại ngay, không cần build
- Người dùng có thể tự do bật tắt Macro để không ảnh hưởng tới công việc khác mà không cần đóng app
- Không yêu cầu về chuột xịn
- Tích hợp unlock Fps và 1 số unlocker (injection) khác

## CÁCH TẢI
- Truy cập vào Link [DOWNLOAD](https://github.com/dinhtuananh188/Macro-Unlock/releases)
- Chọn phiên bản mới nhất
- Đối với WINDOWS: tải file .zip và giải nén, đối với Linux thì theo dõi hướng dẫn ở dưới
- Mở folder được giải nén và chạy file Cryss.exe ngay đầu
- App không tự cập nhật, có bản mới thì tải lại từ trang release

# **DEV**
## Yêu cầu
- Windows hoặc Linux
- Python 3
- Node.js và pnpm

## Cài đặt

Tại thư mục dự án, cài các thư viện Python:

```powershell
python -m pip install -r requirements.txt
```

Cài phụ thuộc cho giao diện:

```powershell
cd src\UI
pnpm install
```

Thư mục `src/unlocker` (DLL unlocker, Launcher) không nằm trong git, lấy từ file release rồi chép vào trước khi build. Khi game cập nhật mà game bị sập sau khi mở, cần thay DLL mới.

## Chạy ở môi trường phát triển

```powershell
cd src\UI
pnpm start
```

Electron sẽ tự khởi động backend Python từ `src/macro/main.py`. Khi Windows hỏi nâng quyền, hãy chấp nhận để macro hoạt động.

## Đóng gói ứng dụng

Sau khi đã cài Python dependencies và `pnpm install`, chạy:

```powershell
.\build.bat
```

Script sẽ đóng gói backend thành `Cryss.exe`, sau đó đóng gói Electron. Bản chạy được nằm tại:

```text
build\app\win-unpacked\Cryss.exe
```

### Build và cập nhật bản đang cài

`release.ps1` build cả backend lẫn Electron rồi chép đè lên thư mục cài đặt, giữ nguyên config combo, `mavuika.json`, gamepath và setting unlocker của người dùng:

```powershell
powershell -ExecutionPolicy Bypass -File release.ps1
```

- `-Install <đường dẫn>`: thư mục cài đặt (mặc định `D:\Downloads\Macro Skirk\win-unpacked`)
- `-SkipBackend`: bỏ qua bước PyInstaller khi chỉ sửa giao diện

Tắt Cryss và game trước khi chạy, nếu không file sẽ bị khóa và không chép được.

### Linux

Bản Linux nằm trong `src-linux/`, dùng evdev / uinput để giả lập phím. Tạo venv ở thư mục dự án, cài dependency rồi chạy:

```bash
python3 -m venv .venv
.venv/bin/pip install evdev
chmod +x src-linux/run.sh
./src-linux/run.sh
```

Script tự nạp module `uinput` (cần `sudo`) và kiểm tra quyền đọc `/dev/input/event*`.

## Các nguồn tham khảo
- Fufu.UnlockerIsland
- Các setup Macro app Xmouse trên bilibili Trung
- [GI-Macro-Manager](https://github.com/3azf55/GI-Macro-Manager) (combo Arlecchino)
- Upstream: [UnlockerMacroGenshinVN/CUTTOOL](https://github.com/UnlockerMacroGenshinVN/CUTTOOL)

## Liên hệ

- Discord: `rururu_11`
- Facebook: [HoanfGZang.UwU](https://www.facebook.com/HoanfGZang.UwU)

Đây là repo do người Việt tạo ra, kết hợp với AI coding để xây dựng giao diện, mọi thứ trên ứng dụng đều là open source và học hỏi từ các open source free khác, vui lòng không sử dụng cho mục đích thương mại. Có thể hỗ trợ, đóng góp ý kiến hoặc donate thông qua thông tin liên hệ đã để lại
