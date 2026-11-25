use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetLocalTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalNow {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

pub fn local_now() -> LocalNow {
    let st: SYSTEMTIME = unsafe { GetLocalTime() };
    LocalNow {
        year: st.wYear as i32,
        month: st.wMonth as u32,
        day: st.wDay as u32,
        hour: st.wHour as u32,
        minute: st.wMinute as u32,
        second: st.wSecond as u32,
    }
}
