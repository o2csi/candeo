//! The Windows reader: performance counters (PDH), `GlobalMemoryStatusEx` and
//! the display kernel's adapter data. None needs administrator rights or a
//! driver; measured unelevated on the Alienware m18 R1, 2026-09-29
//! (`docs/design/inputs-and-automations.md` §2.4).

use std::ptr::{null, null_mut};

use windows_sys::Wdk::Graphics::Direct3D::{
    D3DKMTCloseAdapter, D3DKMTEnumAdapters2, D3DKMTQueryAdapterInfo, D3DKMT_ADAPTERINFO,
    D3DKMT_ADAPTER_PERFDATA, D3DKMT_CLOSEADAPTER, D3DKMT_ENUMADAPTERS2, D3DKMT_QUERYADAPTERINFO,
    KMTQAITYPE_ADAPTERPERFDATA,
};
use windows_sys::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhGetFormattedCounterValue, PdhOpenQueryW, PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA,
    PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
    PDH_MORE_DATA,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

use super::parse;
use super::{Read, Reading};

/// `% Processor Utility` passes 100 when the processor boosts; Task Manager
/// caps what it shows, the reader caps after reading. Not in `windows-sys`.
const PDH_FMT_NOCAP100: u32 = 0x0000_8000;

/// The counters, by their English names: the names shown are localised
/// (*Informations de zone thermique* on a French Windows).
const CPU: &str = r"\Processor Information(_Total)\% Processor Utility";
const GPU: &str = r"\GPU Engine(*)\Utilization Percentage";
const ZONES: &str = r"\Thermal Zone Information(*)\High Precision Temperature";
const POWER: &str = r"\Energy Meter(*)\Power";

pub struct Reader {
    query: PDH_HQUERY,
    cpu: Option<PDH_HCOUNTER>,
    gpu: Option<PDH_HCOUNTER>,
    zones: Option<PDH_HCOUNTER>,
    power: Option<PDH_HCOUNTER>,
}

// The query is used by the one thread that opened it, or by the one asking
// what this computer offers; never by two at once.
unsafe impl Send for Reader {}

impl Reader {
    pub fn open() -> Reader {
        let mut query: PDH_HQUERY = null_mut();
        // SAFETY: a null data source is the live counters; `query` is written.
        let opened = unsafe { PdhOpenQueryW(null(), 0, &mut query) } == 0;
        let add = |path: &str| -> Option<PDH_HCOUNTER> {
            if !opened {
                return None;
            }
            let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            let mut counter: PDH_HCOUNTER = null_mut();
            // SAFETY: `wide` is NUL-terminated and outlives the call.
            let status = unsafe { PdhAddEnglishCounterW(query, wide.as_ptr(), 0, &mut counter) };
            (status == 0).then_some(counter)
        };
        let reader = Reader {
            cpu: add(CPU),
            gpu: add(GPU),
            zones: add(ZONES),
            power: add(POWER),
            query,
        };
        if !opened {
            tracing::warn!("performance counters not opened: system values limited to memory and GPU temperature");
        }
        reader
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        if !self.query.is_null() {
            // SAFETY: the query was opened by this reader and is closed once.
            unsafe { PdhCloseQuery(self.query) };
        }
    }
}

impl Read for Reader {
    fn read(&mut self) -> Reading {
        if !self.query.is_null() {
            // SAFETY: an open query.
            unsafe { PdhCollectQueryData(self.query) };
        }
        Reading {
            cpu: self.cpu.and_then(value).map(|p| (p / 100.0) as f32),
            memory: memory(),
            gpu: self
                .gpu
                .and_then(|c| items(c, |items| parse::busiest_engine(items))),
            cpu_celsius: self.zones.and_then(|c| {
                items(c, |items| {
                    parse::hottest_zone(items.into_iter().map(|(_, v)| v))
                })
            }),
            gpu_celsius: gpu_celsius(),
            cpu_watts: self
                .power
                .and_then(|c| items(c, |items| parse::package_watts(items))),
        }
    }
}

fn valid(status: u32) -> bool {
    status == PDH_CSTATUS_VALID_DATA || status == PDH_CSTATUS_NEW_DATA
}

/// One counter's value; `None` before a rate has two samples.
fn value(counter: PDH_HCOUNTER) -> Option<f64> {
    let mut out = PDH_FMT_COUNTERVALUE::default();
    // SAFETY: a counter of an open query; `out` is written.
    let status = unsafe {
        PdhGetFormattedCounterValue(
            counter,
            PDH_FMT_DOUBLE | PDH_FMT_NOCAP100,
            null_mut(),
            &mut out,
        )
    };
    // SAFETY: asked as a double.
    (status == 0 && valid(out.CStatus)).then_some(unsafe { out.Anonymous.doubleValue })
}

/// A wildcard counter's instances, by name, handed to `f`; `None` when there are
/// none, or no value yet.
fn items<T>(counter: PDH_HCOUNTER, f: impl FnOnce(Vec<(&str, f64)>) -> Option<T>) -> Option<T> {
    let mut size = 0u32;
    let mut count = 0u32;
    // SAFETY: a first call with no buffer asks for the size.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, null_mut())
    };
    if status != PDH_MORE_DATA || size == 0 {
        return None;
    }
    // Items first, then the names they point to, in the one buffer.
    let words = (size as usize).div_ceil(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
    let mut buffer = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); words];
    // SAFETY: `buffer` holds at least `size` bytes.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut size,
            &mut count,
            buffer.as_mut_ptr(),
        )
    };
    if status != 0 {
        return None;
    }
    let names: Vec<(String, f64)> = buffer[..count as usize]
        .iter()
        .filter(|item| valid(item.FmtValue.CStatus))
        .map(|item| {
            // SAFETY: PDH wrote a NUL-terminated name inside `buffer`, and a double.
            let name = unsafe { wide(item.szName) };
            (name, unsafe { item.FmtValue.Anonymous.doubleValue })
        })
        .collect();
    if names.is_empty() {
        return None;
    }
    f(names.iter().map(|(n, v)| (n.as_str(), *v)).collect())
}

/// # Safety
/// `p` is null or a NUL-terminated UTF-16 string.
unsafe fn wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

fn memory() -> Option<f32> {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    // SAFETY: `dwLength` is set, as the call requires.
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 || status.ullTotalPhys == 0 {
        return None;
    }
    Some((1.0 - status.ullAvailPhys as f64 / status.ullTotalPhys as f64) as f32)
}

/// The hottest adapter that answers, as Task Manager reads it: tenths of a
/// degree, from WDDM 2.4 drivers of dedicated GPUs. An integrated GPU, or a
/// dedicated one switched off, does not answer.
fn gpu_celsius() -> Option<f32> {
    let mut list = D3DKMT_ENUMADAPTERS2::default();
    // SAFETY: a first call with no buffer asks for the count.
    if unsafe { D3DKMTEnumAdapters2(&mut list) } != 0 || list.NumAdapters == 0 {
        return None;
    }
    let mut adapters = vec![D3DKMT_ADAPTERINFO::default(); list.NumAdapters as usize];
    list.pAdapters = adapters.as_mut_ptr();
    // SAFETY: `adapters` holds `NumAdapters` entries.
    if unsafe { D3DKMTEnumAdapters2(&mut list) } != 0 {
        return None;
    }
    let mut hottest: Option<f32> = None;
    for adapter in &adapters[..list.NumAdapters as usize] {
        let mut perf = D3DKMT_ADAPTER_PERFDATA::default();
        let mut query = D3DKMT_QUERYADAPTERINFO {
            hAdapter: adapter.hAdapter,
            Type: KMTQAITYPE_ADAPTERPERFDATA,
            pPrivateDriverData: (&mut perf as *mut D3DKMT_ADAPTER_PERFDATA).cast(),
            PrivateDriverDataSize: std::mem::size_of::<D3DKMT_ADAPTER_PERFDATA>() as u32,
        };
        // SAFETY: `perf` outlives the call and its size is given.
        let answered = unsafe { D3DKMTQueryAdapterInfo(&mut query) } == 0;
        let celsius = perf.Temperature as f32 / 10.0;
        if answered && (1.0..150.0).contains(&celsius) {
            hottest = Some(hottest.map_or(celsius, |h| h.max(celsius)));
        }
        // SAFETY: each handle the enumeration opened is closed once.
        unsafe {
            D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER {
                hAdapter: adapter.hAdapter,
            })
        };
    }
    hottest
}
