//! Windows keeps a child running when its parent dies, and never tells it. Plugins join a job
//! that ends them when the app's handle to it closes, which happens however the app ends.

use std::os::windows::io::AsRawHandle;
use std::process::Child;
use std::sync::OnceLock;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows::core::PCWSTR;

struct Job(HANDLE);

// SAFETY: a job handle may be used from any thread.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

static JOB: OnceLock<Option<Job>> = OnceLock::new();

/// Never closed on purpose: the system closes it when the app ends.
fn job() -> Option<&'static Job> {
    JOB.get_or_init(|| {
        // SAFETY: plain calls with values that outlive them.
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).ok()?;
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .ok()?;
            Some(Job(job))
        }
    })
    .as_ref()
}

pub fn adopt(child: &Child) {
    let Some(job) = job() else {
        return;
    };
    // SAFETY: the child's handle is open for as long as `child` lives.
    let joined = unsafe { AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())) };
    if let Err(err) = joined {
        log::warn!(
            "plugin process {} is not tied to the app: {err}",
            child.id()
        );
    }
}
