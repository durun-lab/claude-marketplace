//! ウィンドウのスクリーンショット。`PrintWindow` でウィンドウ自身に描かせて PNG にする。
//! 画面の矩形を写さないので、重なったほかのウィンドウや利用者の画面は入らない。
//! `PW_RENDERFULLCONTENT` で DWM の合成の内容を描かせるので、子 HWND のサーフェスも子 WebView も入る。

use std::path::Path;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic, PW_RENDERFULLCONTENT};

/// 1 画素のバイト数（BGRA）。
const BYTES_PER_PIXEL: usize = 4;

/// `hwnd` のウィンドウを `path` に PNG で書く。
pub fn capture_window(hwnd: isize, path: &Path) -> Result<(), String> {
    let hwnd = HWND(hwnd as *mut _);
    // 最小化中はタイトルバーほどの大きさの空の絵になるので、撮らずに失敗を返す。
    if unsafe { IsIconic(hwnd) }.as_bool() {
        return Err("ウィンドウが最小化されている".into());
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|e| e.to_string())?;
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if w <= 0 || h <= 0 {
        return Err("ウィンドウの大きさが 0".into());
    }
    let bgra = print_window(hwnd, w, h)?;
    let visible = visible_part(hwnd, &rect);
    let bgra = crop(&bgra, w as usize, visible);
    write_png(path, visible.w as u32, visible.h as u32, &bgra)
}

/// 画像の中の矩形（画素）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Part {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

/// ウィンドウの画像のうち見えている枠の矩形。`GetWindowRect` は見えないリサイズの縁を含み、
/// `PrintWindow` はそこを黒で描くので、DWM の枠で切る。枠を引けなければ全体。
fn visible_part(hwnd: HWND, window: &RECT) -> Part {
    let (w, h) = (window.right - window.left, window.bottom - window.top);
    let mut frame = RECT::default();
    let got = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            std::ptr::from_mut(&mut frame).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    };
    let left = (frame.left - window.left).clamp(0, w);
    let top = (frame.top - window.top).clamp(0, h);
    let right = (frame.right - window.left).clamp(left, w);
    let bottom = (frame.bottom - window.top).clamp(top, h);
    if got.is_err() || right <= left || bottom <= top {
        return Part {
            x: 0,
            y: 0,
            w: w as usize,
            h: h as usize,
        };
    }
    Part {
        x: left as usize,
        y: top as usize,
        w: (right - left) as usize,
        h: (bottom - top) as usize,
    }
}

/// 幅 `width` の BGRA の画像から `part` を切り出す。
fn crop(bgra: &[u8], width: usize, part: Part) -> Vec<u8> {
    let row = width * BYTES_PER_PIXEL;
    let mut out = Vec::with_capacity(part.w * part.h * BYTES_PER_PIXEL);
    for line in bgra.chunks_exact(row).skip(part.y).take(part.h) {
        let from = part.x * BYTES_PER_PIXEL;
        out.extend_from_slice(&line[from..from + part.w * BYTES_PER_PIXEL]);
    }
    out
}

fn print_window(hwnd: HWND, w: i32, h: i32) -> Result<Vec<u8>, String> {
    unsafe {
        // 画面の DC は互換のビットマップを作るのにだけ使い、画素は読まない。
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bitmap.into());
        let printed = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                // 負の高さで、上の行から並べる。
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; w as usize * h as usize * BYTES_PER_PIXEL];
        let lines = GetDIBits(
            mem,
            bitmap,
            0,
            h as u32,
            Some(buf.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(mem, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !printed {
            return Err("PrintWindow が失敗".into());
        }
        if lines == 0 {
            return Err("GetDIBits が 0 行".into());
        }
        Ok(buf)
    }
}

fn write_png(path: &Path, w: u32, h: u32, bgra: &[u8]) -> Result<(), String> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for px in bgra.chunks_exact(BYTES_PER_PIXEL) {
        rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgba).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_を_rgba_の_png_にする() {
        let dir = tempfile::Builder::new()
            .prefix("mm_capture_")
            .tempdir()
            .unwrap();
        let path = dir.path().join("x.png");
        write_png(&path, 2, 1, &[10, 20, 30, 0, 40, 50, 60, 0]).unwrap();
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(buf, [30, 20, 10, 255, 60, 50, 40, 255]);
    }

    #[test]
    fn 見えない縁を除いた矩形だけを切り出す() {
        // 3x2 の画像。画素の B に通し番号を入れる。
        let bgra: Vec<u8> = (0..6u8).flat_map(|i| [i, 0, 0, 0]).collect();
        let part = Part {
            x: 1,
            y: 1,
            w: 2,
            h: 1,
        };
        assert_eq!(crop(&bgra, 3, part), [4, 0, 0, 0, 5, 0, 0, 0]);
    }
}
