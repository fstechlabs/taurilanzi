//! How busy the GPU is, from the counters Task Manager shows: the 3D engine of each adapter,
//! split by the processes that use it.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW,
};
use windows::core::{PCWSTR, w};

struct Query {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

// SAFETY: the handles are only used under the mutex, one thread at a time.
unsafe impl Send for Query {}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: a query this struct opened.
        unsafe { PdhCloseQuery(self.query) };
    }
}

/// Kept open: each reading is the use since the last one.
static QUERY: Mutex<Option<Query>> = Mutex::new(None);

pub fn gpu_utilization() -> Option<f32> {
    let mut query = QUERY.lock().unwrap_or_else(PoisonError::into_inner);
    if query.is_none() {
        *query = open();
    }
    let query = query.as_ref()?;
    // SAFETY: open handles; the buffer is as large as the first call asks.
    unsafe {
        if PdhCollectQueryData(query.query) != 0 {
            return None;
        }
        let (mut size, mut count) = (0u32, 0u32);
        if PdhGetFormattedCounterArrayW(query.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
            != PDH_MORE_DATA
        {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        if PdhGetFormattedCounterArrayW(
            query.counter,
            PDH_FMT_DOUBLE,
            &mut size,
            &mut count,
            Some(items),
        ) != 0
        {
            return None;
        }
        let items = std::slice::from_raw_parts(items, count as usize);
        let mut adapters: HashMap<String, f64> = HashMap::new();
        for item in items {
            if item.FmtValue.CStatus != 0 {
                continue;
            }
            let name = item.szName.to_string().unwrap_or_default();
            *adapters.entry(adapter(&name).to_owned()).or_default() +=
                item.FmtValue.Anonymous.doubleValue;
        }
        adapters
            .values()
            .copied()
            .reduce(f64::max)
            .map(|percent| (percent / 100.0).clamp(0.0, 1.0) as f32)
    }
}

fn open() -> Option<Query> {
    let mut handle = PDH_HQUERY::default();
    // SAFETY: outputs that outlive the calls; dropping the query closes it if the counter fails.
    unsafe {
        if PdhOpenQueryW(PCWSTR::null(), 0, &mut handle) != 0 {
            return None;
        }
        let mut query = Query {
            query: handle,
            counter: PDH_HCOUNTER::default(),
        };
        if PdhAddEnglishCounterW(
            query.query,
            w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"),
            0,
            &mut query.counter,
        ) != 0
        {
            return None;
        }
        PdhCollectQueryData(query.query);
        Some(query)
    }
}

/// `pid_1234_luid_0x0_0xD1F2_phys_0_eng_0_engtype_3D` is engine 0 of the adapter `luid_0x0_0xD1F2_phys_0`.
fn adapter(instance: &str) -> &str {
    let start = instance.find("luid_").unwrap_or(0);
    let end = instance.find("_eng_").unwrap_or(instance.len());
    &instance[start..end.max(start)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_engines_by_adapter() {
        assert_eq!(
            adapter("pid_1234_luid_0x00000000_0x0000D1F2_phys_0_eng_0_engtype_3D"),
            "luid_0x00000000_0x0000D1F2_phys_0"
        );
        assert_eq!(adapter("odd"), "odd");
    }
}
