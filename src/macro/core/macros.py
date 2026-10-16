"""
macros.py – Các hàm macro thao tác chuột/bàn phím theo FPS.

BUG FIX (2026-08):
  - skk3as: xóa dòng `frame = frame_values[0]` hardcode làm vô hiệu nội suy.
  - Thêm skk2a, skk5a vào STEP_MAP (thiếu trong phiên bản cũ, custom combo dùng).
  - Pre-compute inv_fps = 1/fps một lần, giảm phép chia lặp trong hot loop.
"""

import bisect
import threading
import time
import pynput

from .config_io import log_debug
from .fps_utils import (
    fps2t, wait_exact,
    T_FPS_3AW, T_FPS_2AS_FIRST, T_FPS_2AS_SECOND,
    T_FPS_2AZ_FIRST, T_FPS_2AZ_END, T_FPS_2AZS_END,
    T_FPS_2AQ_FIRST, T_FPS_2AQ_END,
    T_FPS_5A, T_FPS_5AS_END, T_FRAME_3AS,
)

# ── pynput controllers (singleton, tạo một lần) ──────────────────────────────
mouse    = pynput.mouse.Controller()
keyboard = pynput.keyboard.Controller()
left     = pynput.mouse.Button.left
right    = pynput.mouse.Button.right


# ────────────────────────────────────────────────────────────────────────────
# Primitive macro functions
# ────────────────────────────────────────────────────────────────────────────

def skk3aw(fps):
    """n3w – Spam click trái rồi nhấn W đúng frame."""
    start   = time.perf_counter()
    t       = fps2t(T_FPS_3AW, fps)
    inv_fps = 1.0 / fps

    for _ in range(int(0.6 * fps)):
        if time.perf_counter() - start > 0.6:
            break
        mouse.press(left)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    wait_exact(t, start)
    keyboard.press("w")
    wait_exact(inv_fps)
    keyboard.release("w")


def skk3as(fps):
    """n3d_quick – Spam click trái rồi click phải đúng frame."""
    fps_values   = T_FRAME_3AS[0]
    frame_values = T_FRAME_3AS[1]
    inv_fps      = 1.0 / fps

    # BUG FIX: nội suy frame thực sự thay vì hardcode frame_values[0]
    if fps >= fps_values[-1]:
        frame = frame_values[-1]
    elif fps <= fps_values[0]:
        frame = frame_values[0]
    else:
        i = bisect.bisect_right(fps_values, fps) - 1
        i = max(0, min(i, len(fps_values) - 2))
        w = (fps - fps_values[i]) / (fps_values[i + 1] - fps_values[i])
        frame = frame_values[i] + (frame_values[i + 1] - frame_values[i]) * w

    start = time.perf_counter()

    for _ in range(int(0.6 * fps)):
        if time.perf_counter() - start > 0.6:
            break
        mouse.press(left)
        wait_exact(inv_fps)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    wait_exact(frame * inv_fps)
    mouse.press(right)
    mouse.release(right)


def skk2as(fps):
    """n2d – Click trái spam → right+W."""
    start   = time.perf_counter()
    inv_fps = 1.0 / fps

    for _ in range(int(0.26 * fps)):
        if time.perf_counter() - start > 0.32:
            break
        mouse.press(left)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    t = fps2t(T_FPS_2AS_FIRST, fps)
    wait_exact(t, start)

    mouse.press(right)
    wait_exact(inv_fps)
    mouse.release(right)
    wait_exact(inv_fps)
    keyboard.press('w')
    wait_exact(inv_fps)
    keyboard.release('w')

    t2 = fps2t(T_FPS_2AS_SECOND, fps)
    wait_exact(t2, start)


def skk2a(fps):
    """n2 – Click trái spam, kết thúc tại mốc đầu (không có right+W)."""
    start   = time.perf_counter()
    inv_fps = 1.0 / fps

    for _ in range(int(0.26 * fps)):
        if time.perf_counter() - start > 0.32:
            break
        mouse.press(left)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    t = fps2t(T_FPS_2AS_FIRST, fps)
    wait_exact(t, start)


def skk2az(fps):
    """n2c – Click trái spam → hold left → kết thúc."""
    start   = time.perf_counter()
    inv_fps = 1.0 / fps
    t       = fps2t(T_FPS_2AZ_FIRST, fps)

    for _ in range(int(0.2 * fps)):
        if time.perf_counter() - start > t:
            break
        mouse.press(left)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    wait_exact(t + 2 * inv_fps, start)
    mouse.press(left)
    wait_exact(0.44)
    mouse.release(left)

    t_end = fps2t(T_FPS_2AZ_END, fps)
    wait_exact(t_end, start)


def skk2azs(fps):
    """n2cd – Click trái spam → hold left → right+W → kết thúc."""
    start   = time.perf_counter()
    inv_fps = 1.0 / fps
    t       = fps2t(T_FPS_2AZ_FIRST, fps)

    for _ in range(int(0.2 * fps)):
        if time.perf_counter() - start > t:
            break
        mouse.press(left)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    wait_exact(t + 2 * inv_fps, start)
    mouse.press(left)
    wait_exact(0.44)
    mouse.release(left)

    wait_exact(inv_fps)
    mouse.press(right)
    wait_exact(inv_fps)
    mouse.release(right)
    wait_exact(inv_fps)
    keyboard.press('w')
    wait_exact(inv_fps)
    keyboard.release('w')
    wait_exact(inv_fps)

    t_end = fps2t(T_FPS_2AZS_END, fps)
    wait_exact(t_end, start)


def skk2azs_slow(fps):
    """n2cd_slow – Phiên bản chậm hơn cho FPS >= 105."""
    if fps < 105:
        skk2azs(fps)
        return

    start   = time.perf_counter()
    inv_fps = 1.0 / fps

    for _ in range(int(0.2 * fps)):
        if time.perf_counter() - start > 0.26:
            break
        mouse.press(left)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    wait_exact(0.28, start)
    mouse.press(left)
    wait_exact(0.44)
    mouse.release(left)

    wait_exact(inv_fps)
    wait_exact(0.725, start)

    mouse.press(right)
    wait_exact(inv_fps)
    mouse.release(right)
    wait_exact(inv_fps)
    keyboard.press('w')
    wait_exact(inv_fps)
    keyboard.release('w')
    wait_exact(inv_fps)

    wait_exact(0.87, start)


def skk5as(fps):
    """n5d – Spam 2s → right sau frame target."""
    start   = time.perf_counter()
    inv_fps = 1.0 / fps

    for _ in range(int(2 * fps)):
        if time.perf_counter() - start > 2.1:
            break
        mouse.press(left)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    t = fps2t(T_FPS_5A, fps)
    wait_exact(t + 2 * inv_fps, start)

    t_end = fps2t(T_FPS_5AS_END, fps)
    mouse.press(right)
    wait_exact(inv_fps)
    mouse.release(right)
    wait_exact(inv_fps)
    wait_exact(t_end + 2 * inv_fps, start)


def skk5a(fps):
    """n5 – Spam 2s, không có right."""
    start   = time.perf_counter()
    inv_fps = 1.0 / fps

    for _ in range(int(2 * fps)):
        if time.perf_counter() - start > 2.1:
            break
        mouse.press(left)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    t = fps2t(T_FPS_5A, fps)
    wait_exact(t + 2 * inv_fps, start)


def skk2aq(fps):
    """n2q – Click trái spam → Q → kết thúc."""
    start   = time.perf_counter()
    inv_fps = 1.0 / fps

    for _ in range(int(0.26 * fps)):
        if time.perf_counter() - start > 0.32:
            break
        mouse.press(left)
        mouse.release(left)
        wait_exact(2 * inv_fps)

    t = fps2t(T_FPS_2AQ_FIRST, fps)
    wait_exact(t, start)

    keyboard.press("q")
    wait_exact(inv_fps)
    keyboard.release("q")
    wait_exact(inv_fps)

    t_end = fps2t(T_FPS_2AQ_END, fps)
    wait_exact(t_end, start)


def skke(fps):
    """E – Nhấn E liên tục trong 0.1s, sau đó chờ."""
    start   = time.perf_counter()
    inv_fps = 1.0 / fps

    for _ in range(int(0.1 * fps)):
        if time.perf_counter() - start > 0.1:
            break
        keyboard.press('e')
        wait_exact(inv_fps)
        keyboard.release('e')
        wait_exact(inv_fps)

    wait_exact(0.19)

# ────────────────────────────────────────────────────────────────────────────
# Mavuika – Flamestrider (bike)
#
# Khac macro Skirk: combo Mavuika theo moc ms tuyet doi, khong phu thuoc FPS.
# Toan bo timing nam trong MAV_TIMING ngay ben duoi; sua so xong phai build
# lai (hoac chay lai backend khi dev).
#
# Ba che do chay:
#   "loop"        – GIU nut thi lap lai chuoi; nha nut = dung ngay.
#   "once"        – GIU nut chay het chuoi 1 lan, KHONG lap;
#                   nha nut = NGAT giua chung. Combo dai (10s+) phai dung mode
#                   nay: thuc chien co nhieu bien so, bat buoc ngat duoc.
#   "once_locked" – Chay het chuoi bat ke nha nut, chi alt-tab moi dung.
#
# Combo giu chuot trai rat lau nen bat buoc co rao chan, neu khong se ket phim:
#   1. _mav_lock   – chi 1 nhip chay tai mot thoi diem
#   2. try/finally – nha sach chuot trai + Shift + Q tren MOI duong thoat
#   3. abort som   – dung trong ~1ms khi dieu kien abort thoa man
#   4. chan bind   – xem BLOCKED_HOLD_BINDS trong runtime.py
# ────────────────────────────────────────────────────────────────────────────

shift = pynput.keyboard.Key.shift

_mav_lock = threading.Lock()

# Hook do runtime.py gan vao (tranh import vong):
#   should_abort      – nha nut HOAC mat foreground  (mode loop / once)
#   should_abort_once – chi mat foreground           (mode once_locked)
should_abort = lambda: False
should_abort_once = lambda: False

# Timing tung combo Mavuika (va combo GI dung chung engine).
# event = (moc_ms, hanh_dong). c_down/c_up = giu/nha chuot trai (trong kich),
# d_down/d_up = giu/nha Shift (luot), q_down/q_up = phim Q (chieu no), end = het.
#
# mode:
#   "loop"        GIU nut thi lap lai chuoi; nha nut hoac alt-tab = dung ngay.
#   "once"        GIU nut chay het chuoi 1 lan, KHONG lap; nha nut = ngat giua chung.
#                 Dung cho combo dai: thuc chien co nhieu bien so, phai ngat duoc.
#   "once_locked" Chay het chuoi bat ke nha nut; chi alt-tab khoi game moi dung.
#                 Chi dung khi chac chan khong can ngat (vd danh dummy).
# beat_*: nhip nho dung cho combo tu tao (palette Mavuika), khong hien o danh sach combo.
MAV_TIMING = {
    # Nguon: vulcanizer.netlify.app (zsksc-gen/mavuika-trainer) - COMBOS.cdcdcf
    "C0:  Combo Mavuika CDCDCF (Full Combo)": {
        "mode": "loop",
        "events": [
            (0, "c_down"), (200, "d_down"), (250, "d_up"), (320, "c_up"),
            (370, "c_down"), (570, "d_down"), (620, "d_up"), (1640, "c_up"),
            (2160, "end"),
        ],
    },
    # Nguon: File "Mav OL full rotation.amc" - tach tung nhip
    # Ghi chu: Nhip Q: no 202ms roi cho 1555ms (tong 1757ms).
    "beat_q": {
        "mode": "once",
        "events": [
            (0, "q_down"), (202, "q_up"), (1757, "end"),
        ],
    },
    # Nguon: File "Mav OL full rotation.amc" - tach tung nhip
    # Ghi chu: Nhip C: charge ngan 300ms roi nha, nghi 60ms (tong 360ms).
    "beat_c": {
        "mode": "once",
        "events": [
            (0, "c_down"), (300, "c_up"), (360, "end"),
        ],
    },
    # Nguon: File "Mav OL full rotation.amc" - tach tung nhip
    # Ghi chu: Nhip D: dash 91ms, nghi 90ms (tong 181ms).
    "beat_d": {
        "mode": "once",
        "events": [
            (0, "d_down"), (91, "d_up"), (181, "end"),
        ],
    },
    # Nguon: File "Mav OL full rotation.amc" - tach tung nhip
    # Ghi chu: Nhip CD: giu C, dash cat o 200ms, nha C o 442ms, nghi 90ms (tong 532ms).
    "beat_cd": {
        "mode": "once",
        "events": [
            (0, "c_down"), (200, "d_down"), (291, "d_up"), (442, "c_up"),
            (532, "end"),
        ],
    },
    # Nguon: File "Mav OL full rotation.amc" - tach tung nhip
    # Ghi chu: Nhip CDF: nhu CD nhung giu them 1000ms roi nha ra F, nghi 853ms (tong 2144ms).
    "beat_cdf": {
        "mode": "once",
        "events": [
            (0, "c_down"), (200, "d_down"), (291, "d_up"), (1291, "c_up"),
            (2144, "end"),
        ],
    },
    # Nguon: Do click N1 64ms trong file "mavuika CDN1.amc"
    # Ghi chu: Click 1 cai: nhan chuot trai 64ms roi nha, nghi 60ms (tong 124ms).
    "beat_click": {
        "mode": "once",
        "events": [
            (0, "c_down"), (64, "c_up"), (124, "end"),
        ],
    },
    # Nguon: Do click N1 64ms trong file "mavuika CDN1.amc"
    # Ghi chu: Click 2 cai: moi cai 64ms, cach nhau 100ms, nghi 60ms (tong 288ms). Doi nhip thi sua moc 164.
    "beat_click2": {
        "mode": "once",
        "events": [
            (0, "c_down"), (64, "c_up"), (164, "c_down"), (228, "c_up"),
            (288, "end"),
        ],
    },
    # Nguon: github.com/3azf55/GI-Macro-Manager (MIT)
    # Tooltip: Overload N2W
    "C0:  Combo Arlecchino Overload N2W (GI)": {
        "mode": "loop",
        "events": [
            (0, "c_down"), (40, "c_up"), (80, "c_down"), (120, "c_up"),
            (160, "c_down"), (200, "c_up"), (240, "c_down"), (280, "c_up"),
            (320, "c_down"), (360, "c_up"), (840, "w_down"), (880, "w_up"),
            (920, "w_down"), (960, "w_up"), (960, "end"),
        ],
    },
}


def _mav_release_all():
    """Nha sach moi input ma combo Mavuika co the dang giu."""
    for release in (lambda: mouse.release(left),
                    lambda: keyboard.release(shift),
                    lambda: keyboard.release("q"),
                    lambda: keyboard.release("1"),
                    lambda: keyboard.release("e"),
                    lambda: keyboard.release("w"),
                    lambda: keyboard.release("a"),
                    lambda: keyboard.release("d"),
                    lambda: mouse.release(right)):
        try:
            release()
        except Exception:
            pass


def _wait_or_abort(deadline, start, abort):
    """Cho toi moc `deadline` (giay, tinh tu `start`).

    Giu do chinh xac nhu wait_exact: 2ms cuoi busy-wait.
    Tra True neu bi abort giua chung.
    """
    while True:
        remaining = deadline - (time.perf_counter() - start)
        if remaining <= 0:
            return False
        if remaining > 0.002:
            if abort():
                return True
            time.sleep(0.001)


_MAV_ACTIONS = {
    "c_down": lambda: mouse.press(left),
    "c_up":   lambda: mouse.release(left),
    "d_down": lambda: keyboard.press(shift),
    "d_up":   lambda: keyboard.release(shift),
    "q_down": lambda: keyboard.press("q"),
    "q_up":   lambda: keyboard.release("q"),
    "r_down": lambda: mouse.press(right),
    "r_up":   lambda: mouse.release(right),
    "k1_down": lambda: keyboard.press("1"),
    "k1_up":   lambda: keyboard.release("1"),
    "e_down": lambda: keyboard.press("e"),
    "e_up":   lambda: keyboard.release("e"),
    "w_down": lambda: keyboard.press("w"),
    "w_up":   lambda: keyboard.release("w"),
    # Phim A/D di chuyen that su (khac "d_down/d_up" = Shift/dash o tren).
    "ka_down": lambda: keyboard.press("a"),
    "ka_up":   lambda: keyboard.release("a"),
    "kd_down": lambda: keyboard.press("d"),
    "kd_up":   lambda: keyboard.release("d"),
    "end":    None,
}


def mav_play(events, abort, start_ms=0):
    """Phat chuoi event theo moc thoi gian tuyet doi.

    start_ms: moc goc (ms) cua events, de phat lai mot doan giua chung.
    Tra False neu co nhip Mavuika khac dang chay (bo qua de khong chong input).
    """
    if not _mav_lock.acquire(blocking=False):
        return False
    try:
        start = time.perf_counter()
        for t_ms, action in events:
            if _wait_or_abort((t_ms - start_ms) / 1000.0, start, abort):
                return True
            fn = _MAV_ACTIONS.get(action)
            if fn is not None:
                fn()
        return True
    finally:
        _mav_release_all()
        _mav_lock.release()


# ── Mavuika: nut roi cho combo tu tao ────────────────────────────────────────
# Khac cac nhip dung san: nhung ham nay KHONG tu nha input, nen "C giu" phai
# co "C nha" phia sau. Rao chan cuoi cung nam o build_custom_combo_fn:
# ket thuc combo (ke ca bi ngat giua chung) la goi _mav_release_all().

def mav_press_c(fps):
    """C giu - nhan giu chuot trai de charge, khong nha."""
    mouse.press(left)


def mav_release_c(fps):
    """C nha - nha chuot trai. Charge du lau thi ra finisher (F)."""
    mouse.release(left)


def mav_tap_d(fps):
    """D - dash: go Shift 91ms (dung so lieu file .amc)."""
    keyboard.press(shift)
    wait_exact(0.091)
    keyboard.release(shift)


def mav_tap_q(fps):
    """Q - no nguyen to: go Q 202ms."""
    keyboard.press("q")
    wait_exact(0.202)
    keyboard.release("q")


def mav_wait(ms):
    """Tao buoc cho `ms` mili giay, ngat duoc giua chung (nha nut / alt-tab)."""
    sec = ms / 1000.0

    def step(fps):
        _wait_or_abort(sec, time.perf_counter(), should_abort)

    step.__name__ = f"mav_wait_{ms}"
    return step


for _fn in (mav_press_c, mav_release_c, mav_tap_d, mav_tap_q):
    _fn.uses_held_input = True
del _fn


def mav_combo(name):
    """Tao ham combo doc timing theo ten trong MAV_TIMING."""
    def run(fps):
        cfg = MAV_TIMING.get(name)
        if not cfg:
            log_debug(f"MAV_TIMING: khong tim thay combo {name!r}")
            return
        events = cfg["events"]
        mode = cfg.get("mode", "loop")

        if mode == "once":
            # Nha nut ngat giua chung -> ngat duoc khi thuc chien co bien
            if not mav_play(events, should_abort):
                return
            # loop_from_ms: sau lan chay dau, lap doan tu moc nay cho toi khi nha nut
            loop_from = cfg.get("loop_from_ms")
            if loop_from is not None:
                body = [e for e in events if e[0] >= loop_from]
                while not should_abort():
                    if not mav_play(body, should_abort, start_ms=loop_from):
                        break
            return
        if mode == "once_locked":
            mav_play(events, should_abort_once)
            return

        start = time.perf_counter()
        while time.perf_counter() - start < 20:
            if not mav_play(events, should_abort):
                break            # nhip khac dang chay -> khong spin vo ich
            if should_abort():
                break

    run.__name__ = "mav_" + "".join(ch if ch.isalnum() else "_" for ch in name)[:40]
    run.uses_held_input = True
    return run


# ════════════════════════════════════════════════════════════════════════════
# COMBO DỰNG SẴN – toàn bộ chuỗi combo + bảng tra nằm ở đây.
# Thêm combo mới: viết hàm, đăng ký vào COMBO_MAP, rồi thêm đúng tên đó vào
# BUILTIN_COMBOS trong src/UI/shared.js để giao diện hiện ra.
# ════════════════════════════════════════════════════════════════════════════

# ── Named combo sequences ─────────────────────────────────────────────────────

def skk223_loop(fps):
    """Lặp vô hạn 2-2-3 cho đến khi nhả phím."""
    start = time.perf_counter()
    while time.perf_counter() - start < 20:
        skk2as(fps)
        skk2as(fps)
        skk3aw(fps)
        if should_abort():
            break


def skkC0_EQA_120f(fps):
    """C0: 222q 223 223 22cd23 223  –  tối ưu cho ~120 fps."""
    skke(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2aq(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2azs_slow(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)


def skkC0_QEA_120f(fps):
    """C0: 222c 223 223 22cd23 223  –  biến thể QEA ~120 fps."""
    skke(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2az(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2azs_slow(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)


def skkC0_EQA_60f(fps):
    """C0: 222q 223 223 22c 224  –  tối ưu cho ~60 fps."""
    skke(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2aq(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk3aw(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2az(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk2as(fps)
    if should_abort(): return
    skk5as(fps)


# Ten combo Mavuika = key trong MAV_TIMING. Doi ten phai doi ca 3 noi:
# MAV_TIMING, MAVUIKA_COMBOS (cung file nay), BUILTIN_COMBOS (src/UI/shared.js).
MAVUIKA_COMBOS = (
    "C0:  Combo Mavuika CDCDCF (Full Combo)",
    # ── Lay tu GI-Macro-Manager (github.com/3azf55/GI-Macro-Manager, MIT) ──
    "C0:  Combo Arlecchino Overload N2W (GI)",
)

_MAVUIKA_FNS = {name: mav_combo(name) for name in MAVUIKA_COMBOS}


# ── Combo map & step map ──────────────────────────────────────────────────────

COMBO_MAP = {
    "C0:  Combo Skirk C0 EQA 120fps": skkC0_EQA_120f,
    "C0:  Combo Skirk C0 EA 120fps":  skkC0_QEA_120f,
    "C0:  Combo Skirk C0 EQA 60fps":  skkC0_EQA_60f,
    **_MAVUIKA_FNS,
}

# BUG FIX: thêm skk2a và skk5a (thiếu trong phiên bản cũ, custom combo cần)
STEP_MAP = {
    "skk3aw":       skk3aw,
    "skk2as":       skk2as,
    "skk2a":        skk2a,          # n2  – đã thêm
    "skk3as":       skk3as,
    "skk2az":       skk2az,
    "skk2azs":      skk2azs,
    "skk2azs_slow": skk2azs_slow,
    "skk2aq":       skk2aq,
    "skke":         skke,
    "skk5as":       skk5as,
    "skk5a":        skk5a,          # n5  – đã thêm
    # ── Mavuika: dùng trong custom combo, key = tên trong MAV_TIMING ──
    "mav_cdcdcf":   _MAVUIKA_FNS[MAVUIKA_COMBOS[0]],
    # ── Mavuika: nhịp nhỏ cho combo tự tạo (số liệu từ Mav OL full rotation.amc) ──
    "mav_b_q":      mav_combo("beat_q"),
    "mav_b_c":      mav_combo("beat_c"),
    "mav_b_d":      mav_combo("beat_d"),
    "mav_b_cd":     mav_combo("beat_cd"),
    "mav_b_cdf":    mav_combo("beat_cdf"),
    "mav_b_click":  mav_combo("beat_click"),
    "mav_b_click2": mav_combo("beat_click2"),
    # ── Mavuika: nút rời, tự ghép lấy nhịp ──
    "mav_c_hold":   mav_press_c,
    "mav_c_rel":    mav_release_c,
    "mav_d":        mav_tap_d,
    "mav_q":        mav_tap_q,
    # ── Khối chờ ──
    "wait_50":     mav_wait(50),
    "wait_100":    mav_wait(100),
    "wait_150":    mav_wait(150),
    "wait_200":    mav_wait(200),
    "wait_300":    mav_wait(300),
    "wait_500":    mav_wait(500),
    "wait_1000":   mav_wait(1000),
}
