use np_ui::glass::{GlassPanel, GlassParams};
use np_ui::host::{attach, Surface};
use windows::core::PCWSTR;
use windows::UI::Composition::{Compositor, ContainerVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::Result;
use windows_numerics::Vector2;

pub struct Popup {
    pub hwnd: HWND,
    surface: Surface,
    pub panel: GlassPanel,
    pub content: ContainerVisual,
    pub rect: (i32, i32, i32, i32),
}

impl Popup {
    pub fn new(
        comp: &Compositor,
        class: PCWSTR,
        proc: WNDPROC,
        ex_extra: WINDOW_EX_STYLE,
        glass: GlassParams,
        activatable: bool,
    ) -> Result<Self> {
        unsafe {
            let hi: HINSTANCE = GetModuleHandleW(None)?.into();
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: proc,
                hInstance: hi,
                lpszClassName: class,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP
                    | WS_EX_TOOLWINDOW
                    | if activatable {
                        WINDOW_EX_STYLE(0)
                    } else {
                        WS_EX_NOACTIVATE
                    }
                    | ex_extra,
                class,
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                Some(hi),
                None,
            )?;
            let surface = attach(comp, hwnd, (10.0, 10.0))?;
            let panel = GlassPanel::new(comp, &surface.root, glass)?;
            let content = comp.CreateContainerVisual()?;
            surface.root.Children()?.InsertAtTop(&content)?;
            Ok(Self {
                hwnd,
                surface,
                panel,
                content,
                rect: (0, 0, 10, 10),
            })
        }
    }

    pub fn set_rect(&mut self, x: i32, y: i32, w: i32, h: i32) -> Result<()> {
        self.rect = (x, y, w, h);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                x,
                y,
                w,
                h,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
        self.surface.root.SetSize(Vector2 {
            X: w as f32,
            Y: h as f32,
        })?;
        self.panel.set_rect(0.0, 0.0, w as f32, h as f32)
    }

    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }
}
